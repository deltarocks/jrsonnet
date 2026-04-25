use std::string::String;

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{
	Attribute, DeriveInput, Error, Expr, ExprClosure, FnArg, GenericArgument, Ident, ItemFn,
	LitStr, Meta, Pat, Path, PathArguments, Result, ReturnType, Token, Type, parenthesized,
	parse::{Parse, ParseStream},
	parse_macro_input,
	punctuated::Punctuated,
	spanned::Spanned,
	token::Comma,
};

use self::typed::{derive_from_untyped_inner, derive_into_untyped_inner, derive_typed_inner};

mod names;
mod typed;

fn try_parse_attr_noargs<I>(attrs: &[Attribute], ident: I) -> Result<bool>
where
	Ident: PartialEq<I>,
{
	let attrs = attrs
		.iter()
		.filter(|a| a.path().is_ident(&ident))
		.collect::<Vec<_>>();
	if attrs.len() > 1 {
		return Err(Error::new(
			attrs[1].span(),
			"this attribute may be specified only once",
		));
	} else if attrs.is_empty() {
		return Ok(false);
	}
	let attr = attrs[0];

	match attr.meta {
		Meta::Path(_) => Ok(true),
		_ => Ok(false),
	}
}
fn parse_attr<A: Parse, I>(attrs: &[Attribute], ident: I) -> Result<Option<A>>
where
	Ident: PartialEq<I>,
{
	let attrs = attrs
		.iter()
		.filter(|a| a.path().is_ident(&ident))
		.collect::<Vec<_>>();
	if attrs.len() > 1 {
		return Err(Error::new(
			attrs[1].span(),
			"this attribute may be specified only once",
		));
	} else if attrs.is_empty() {
		return Ok(None);
	}
	let attr = attrs[0];
	let attr = attr.parse_args::<A>()?;

	Ok(Some(attr))
}
fn remove_attr<I>(attrs: &mut Vec<Attribute>, ident: I)
where
	Ident: PartialEq<I>,
{
	attrs.retain(|a| !a.path().is_ident(&ident));
}

fn path_is(path: &Path, needed: &str) -> bool {
	path.leading_colon.is_none()
		&& !path.segments.is_empty()
		&& path.segments.iter().last().unwrap().ident == needed
}

fn type_is_path<'ty>(ty: &'ty Type, needed: &str) -> Option<&'ty PathArguments> {
	match ty {
		Type::Path(path) if path.qself.is_none() && path_is(&path.path, needed) => {
			let args = &path.path.segments.iter().last().unwrap().arguments;
			Some(args)
		}
		_ => None,
	}
}

fn extract_type_from_option(ty: &Type) -> Result<Option<&Type>> {
	let Some(args) = type_is_path(ty, "Option") else {
		return Ok(None);
	};
	// It should have only on angle-bracketed param ("<String>"):
	let PathArguments::AngleBracketed(params) = args else {
		return Err(Error::new(args.span(), "missing option generic"));
	};
	let generic_arg = params.args.iter().next().unwrap();
	// This argument must be a type:
	let GenericArgument::Type(ty) = generic_arg else {
		return Err(Error::new(
			generic_arg.span(),
			"option generic should be a type",
		));
	};
	Ok(Some(ty))
}

struct Field {
	attrs: Vec<Attribute>,
	name: Ident,
	_colon: Token![:],
	ty: Type,
}
impl Parse for Field {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		Ok(Self {
			attrs: input.call(Attribute::parse_outer)?,
			name: input.parse()?,
			_colon: input.parse()?,
			ty: input.parse()?,
		})
	}
}

mod kw {
	syn::custom_keyword!(fields);
	syn::custom_keyword!(rename);
	syn::custom_keyword!(alias);
	syn::custom_keyword!(flatten);
	syn::custom_keyword!(add);
	syn::custom_keyword!(hide);
	syn::custom_keyword!(method);
	syn::custom_keyword!(ok);
}

struct BuiltinAttrs {
	fields: Vec<Field>,
}
impl Parse for BuiltinAttrs {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		if input.is_empty() {
			return Ok(Self { fields: Vec::new() });
		}
		input.parse::<kw::fields>()?;
		let fields;
		parenthesized!(fields in input);
		let p = Punctuated::<Field, Comma>::parse_terminated(&fields)?;
		Ok(Self {
			fields: p.into_iter().collect(),
		})
	}
}

enum Optionality {
	Required,
	Optional,
	Default(Expr),
	TypeDefault,
}

#[allow(
	clippy::large_enum_variant,
	reason = "this macro is not that hot for it to matter"
)]
enum ArgInfo {
	Normal {
		ty: Box<Type>,
		optionality: Optionality,
		name: Option<String>,
		cfg_attrs: Vec<Attribute>,
	},
	Lazy {
		is_option: bool,
		name: Option<String>,
	},
	Context,
	Location,
	This,
}

impl ArgInfo {
	fn parse(name: &str, arg: &mut FnArg) -> Result<Self> {
		let FnArg::Typed(arg) = arg else {
			unreachable!()
		};
		let ident = match &arg.pat as &Pat {
			Pat::Ident(i) => Some(i.ident.clone()),
			_ => None,
		};
		let ty = &arg.ty;
		if type_is_path(ty, "Context").is_some() {
			return Ok(Self::Context);
		} else if type_is_path(ty, "CallLocation").is_some() {
			return Ok(Self::Location);
		} else if type_is_path(ty, "Thunk").is_some() {
			return Ok(Self::Lazy {
				is_option: false,
				name: ident.map(|v| v.to_string()),
			});
		}

		match ty as &Type {
			Type::Reference(r) if type_is_path(&r.elem, name).is_some() => return Ok(Self::This),
			_ => {}
		}

		let (optionality, ty) = if try_parse_attr_noargs(&arg.attrs, "default")? {
			remove_attr(&mut arg.attrs, "default");
			(Optionality::TypeDefault, ty.clone())
		} else if let Some(default) = parse_attr::<_, _>(&arg.attrs, "default")? {
			remove_attr(&mut arg.attrs, "default");
			(Optionality::Default(default), ty.clone())
		} else if let Some(ty) = extract_type_from_option(ty)? {
			if type_is_path(ty, "Thunk").is_some() {
				return Ok(Self::Lazy {
					is_option: true,
					name: ident.map(|v| v.to_string()),
				});
			}

			(Optionality::Optional, Box::new(ty.clone()))
		} else {
			(Optionality::Required, ty.clone())
		};

		let cfg_attrs = arg
			.attrs
			.iter()
			.filter(|a| a.path().is_ident("cfg"))
			.cloned()
			.collect();

		Ok(Self::Normal {
			ty,
			optionality,
			name: ident.map(|v| v.to_string()),
			cfg_attrs,
		})
	}
}

#[proc_macro_attribute]
pub fn builtin(
	attr: proc_macro::TokenStream,
	item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	let attr = parse_macro_input!(attr as BuiltinAttrs);
	let item_fn = parse_macro_input!(item as ItemFn);

	match builtin_inner(attr, item_fn) {
		Ok(v) => v.into(),
		Err(e) => e.into_compile_error().into(),
	}
}

#[allow(clippy::too_many_lines)]
fn builtin_inner(attr: BuiltinAttrs, mut fun: ItemFn) -> syn::Result<TokenStream> {
	let ReturnType::Type(_, result) = &fun.sig.output else {
		return Err(Error::new(
			fun.sig.span(),
			"builtin should return something",
		));
	};

	let name = fun.sig.ident.to_string();
	let args = fun
		.sig
		.inputs
		.iter_mut()
		.map(|arg| ArgInfo::parse(&name, arg))
		.collect::<Result<Vec<_>>>()?;

	let params_desc = args.iter().filter_map(|a| match a {
		ArgInfo::Normal {
			optionality,
			name,
			cfg_attrs,
			..
		} => {
			let name = name
				.as_ref()
				.map_or_else(|| quote! {unnamed}, |n| quote! {named(#n)});
			let default = match optionality {
				Optionality::Required => quote!(ParamDefault::None),
				Optionality::Optional | Optionality::TypeDefault => quote!(ParamDefault::Exists),
				Optionality::Default(e) => quote!(ParamDefault::Literal(stringify!(#e))),
			};
			Some(quote! {
				#(#cfg_attrs)*
				[#name => #default],
			})
		}
		ArgInfo::Lazy { is_option, name } => {
			let name = name
				.as_ref()
				.map_or_else(|| quote! {unnamed}, |n| quote! {named(#n)});
			Some(quote! {
				[#name => ParamDefault::exists(#is_option)],
			})
		}
		ArgInfo::Context | ArgInfo::Location | ArgInfo::This => None,
	});

	let mut id = 0usize;
	let pass = args
		.iter()
		.map(|a| match a {
			ArgInfo::Normal { .. } | ArgInfo::Lazy { .. } => {
				let cid = id;
				id += 1;
				(quote! {#cid}, a)
			}
			ArgInfo::Context | ArgInfo::Location | ArgInfo::This => {
				(quote! {compile_error!("should not use id")}, a)
			}
		})
		.map(|(id, a)| match a {
			ArgInfo::Normal {
				ty,
				optionality,
				name,
				cfg_attrs,
			} => {
				let name = name.as_ref().map_or("<unnamed>", String::as_str);
				let eval = quote! {jrsonnet_evaluator::in_description_frame(
					|| format!("argument <{}> evaluation", #name),
					|| <#ty as FromUntyped>::from_untyped(value.evaluate()?),
				)?};
				let value = match optionality {
					Optionality::Required => quote! {{
						let value = parsed[#id].as_ref().expect("args shape is checked");
						#eval
					},},
					Optionality::Optional => quote! {if let Some(value) = &parsed[#id] {
						Some(#eval)
					} else {
						None
					},},
					Optionality::Default(expr) => quote! {if let Some(value) = &parsed[#id] {
						#eval
					} else {
						let v: #ty = #expr;
						v
					},},
					Optionality::TypeDefault => quote! {if let Some(value) = &parsed[#id] {
						#eval
					} else {
						let v: #ty = Default::default();
						v
					},},
				};
				quote! {
					#(#cfg_attrs)*
					#value
				}
			}
			ArgInfo::Lazy { is_option, .. } => {
				if *is_option {
					quote! {if let Some(value) = &parsed[#id] {
						Some(value.clone())
					} else {
						None
					},}
				} else {
					quote! {
						parsed[#id].as_ref().expect("args shape is correct").clone(),
					}
				}
			}
			ArgInfo::Context => quote! {ctx.clone(),},
			ArgInfo::Location => quote! {location,},
			ArgInfo::This => quote! {self,},
		});

	let fields = attr.fields.iter().map(|field| {
		let attrs = &field.attrs;
		let name = &field.name;
		let ty = &field.ty;
		quote! {
			#(#attrs)*
			pub #name: #ty,
		}
	});

	let name = &fun.sig.ident;
	let vis = &fun.vis;
	let static_derive_copy = if attr.fields.is_empty() {
		quote! {, Copy, Default}
	} else {
		quote! {}
	};

	Ok(quote! {
		#fun

		#[doc(hidden)]
		#[allow(non_camel_case_types)]
		#[derive(Clone, jrsonnet_gcmodule::Trace #static_derive_copy)]
		#vis struct #name {
			#(#fields)*
		}
		const _: () = {
			use ::jrsonnet_evaluator::{
				State, Val,
				function::{builtin::Builtin, FunctionSignature, ParamParse, ParamName, ParamDefault, CallLocation},
				Result, Context, typed::{Typed, FromUntyped, IntoUntypedResult},
				Span, params, Thunk,
			};
			params!(
				#(#params_desc)*
			);

			impl Builtin for #name
			where
				Self: 'static
			{
				fn name(&self) -> &str {
					stringify!(#name)
				}
				fn params(&self) -> FunctionSignature {
					PARAMS.with(|p| p.clone())
				}
				#[allow(unused_variables)]
				fn call(&self, location: CallLocation<'_>, parsed: &[Option<Thunk<Val>>]) -> Result<Val> {
					let result: #result = #name(#(#pass)*);
					<_ as IntoUntypedResult>::into_untyped_result(result)
				}
				fn as_any(&self) -> &dyn ::std::any::Any {
					self
				}
			}
		};
	})
}

#[proc_macro_derive(Typed, attributes(typed))]
pub fn derive_typed(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input = parse_macro_input!(item as DeriveInput);

	match derive_typed_inner(input) {
		Ok(v) => v.into(),
		Err(e) => e.to_compile_error().into(),
	}
}
#[proc_macro_derive(IntoUntyped, attributes(typed))]
pub fn derive_into_untyped(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input = parse_macro_input!(item as DeriveInput);

	match derive_into_untyped_inner(input) {
		Ok(v) => v.into(),
		Err(e) => e.to_compile_error().into(),
	}
}
#[proc_macro_derive(FromUntyped, attributes(typed))]
pub fn derive_from_untyped(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input = parse_macro_input!(item as DeriveInput);

	match derive_from_untyped_inner(input) {
		Ok(v) => v.into(),
		Err(e) => e.to_compile_error().into(),
	}
}

struct FormatInput {
	formatting: LitStr,
	arguments: Vec<Expr>,
}
impl Parse for FormatInput {
	fn parse(input: ParseStream) -> Result<Self> {
		let formatting = input.parse()?;
		let mut arguments = Vec::new();

		while input.peek(Token![,]) {
			input.parse::<Token![,]>()?;
			if input.is_empty() {
				// Trailing comma
				break;
			}
			let expr = input.parse()?;
			arguments.push(expr);
		}

		if !input.is_empty() {
			return Err(syn::Error::new(input.span(), "unexpected trailing input"));
		}

		Ok(Self {
			formatting,
			arguments,
		})
	}
}
fn is_format_str(i: &str) -> bool {
	let mut is_plain = true;
	// -1 = {
	// +1 = }
	let mut is_bracket = 0i8;
	for ele in i.chars() {
		match ele {
			'{' if is_bracket == -1 => {
				is_bracket = 0;
			}
			'}' if is_bracket == -1 => {
				is_plain = false;
				break;
			}
			'}' if is_bracket == 1 => {
				is_bracket = 0;
			}
			'{' if is_bracket == 1 => {
				is_plain = false;
				break;
			}
			'{' => {
				is_bracket = -1;
			}
			'}' => {
				is_bracket = 1;
			}
			_ if is_bracket != 0 => {
				is_plain = false;
				break;
			}
			_ => {}
		}
	}
	!is_plain || is_bracket != 0
}
impl FormatInput {
	fn expand(self) -> TokenStream {
		let format = self.formatting;
		if is_format_str(&format.value()) {
			let args = self.arguments;
			quote! {
				::jrsonnet_evaluator::IStr::from(format!(#format #(, #args)*))
			}
		} else {
			if let Some(first) = self.arguments.first() {
				return syn::Error::new(
					first.span(),
					"string has no formatting codes, it should not have the arguments",
				)
				.into_compile_error();
			}
			quote! {
				::jrsonnet_evaluator::IStr::from(#format)
			}
		}
	}
}

/// `IStr` formatting helper
///
/// Using `format!("literal with no codes").into()` is slower than just `"literal with no codes".into()`
/// This macro looks for formatting codes in the input string, and uses
/// `format!()` only when necessary
#[proc_macro]
pub fn format_istr(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input = parse_macro_input!(input as FormatInput);
	input.expand().into()
}

/// Create Thunk using closure syntax
#[proc_macro]
#[allow(non_snake_case)]
pub fn Thunk(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input = parse_macro_input!(input as ExprClosure);

	let span = input.inputs.span();
	let move_check = input.capture.is_none().then(|| {
		quote_spanned! {span => {
			compile_error!("Thunk! needs to be called with move closure");
		}}
	});

	let (env, closure, args) = syn_dissect_closure::split_env(input);

	let trace_check = args.iter().map(|el| {
		let span = el.span();
		quote_spanned! {span => ::jrsonnet_evaluator::gc::assert_trace(&#el);}
	});

	quote! {{
		#move_check
		#(#trace_check)*
		::jrsonnet_evaluator::Thunk::new(::jrsonnet_evaluator::val::MemoizedClosureThunk::new(#env, #closure))
	}}.into()
}
