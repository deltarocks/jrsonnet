use std::{collections::HashMap, path::PathBuf};

use anyhow::Result;
use ast::{AstSrc, lower};
use itertools::Itertools;
use kinds::{KindsSrc, TokenKind};
use proc_macro2::{Ident, Punct, Spacing, Span, TokenStream};
use quote::{format_ident, quote};
use ungrammar::Grammar;
use util::{ensure_file_contents, reformat, to_pascal_case, to_upper_snake_case};

mod ast;
mod kinds;
mod util;

enum SpecialName {
	Literal,
	Meta,
	Error,
}
fn classify_special(name: &str) -> Option<(SpecialName, &str)> {
	let name = name.strip_suffix('!')?;
	Some(if let Some(name) = name.strip_prefix("LIT_") {
		(SpecialName::Literal, name)
	} else if let Some(name) = name.strip_prefix("META_") {
		(SpecialName::Meta, name)
	} else if let Some(name) = name.strip_prefix("ERROR_") {
		(SpecialName::Error, name)
	} else {
		return None;
	})
}

pub fn generate_ungrammar() -> Result<()> {
	let grammar: Grammar = include_str!(concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/../crates/jrsonnet-rowan-parser/jsonnet.ungram"
	))
	.parse()?;

	let mut kinds = kinds::jsonnet_kinds();
	let ast = lower(&kinds, &grammar);

	for token in grammar.tokens() {
		let token = &grammar[token];
		let token = &token.name.clone();
		if !kinds.is_token(token) {
			if let Some((special, name)) = classify_special(token) {
				match special {
					SpecialName::Literal => panic!("literal is not defined: {name}"),
					SpecialName::Meta => {
						eprintln!("implicit meta: {name}");
						kinds.define_token(TokenKind::Meta {
							grammar_name: token.to_owned(),
							name: format!("META_{name}"),
						});
					}
					SpecialName::Error => {
						panic!(
							"error token ERROR_{name} must be explicitly defined in jsonnet_kinds()"
						);
					}
				}
				continue;
			}
			let name = to_upper_snake_case(token);
			eprintln!("implicit kw: {token}");
			kinds.define_token(TokenKind::Keyword {
				code: token.to_owned(),
				name: format!("{name}_KW"),
			});
		}
	}
	for node in &ast.nodes {
		let name = to_upper_snake_case(&node.name);
		kinds.define_node(&name);
	}
	for enum_ in &ast.enums {
		let name = to_upper_snake_case(&enum_.name);
		kinds.define_node(&name);
	}
	for token_enum in &ast.token_enums {
		let name = to_upper_snake_case(&token_enum.name);
		kinds.define_node(&name);
	}

	let syntax_kinds = generate_syntax_kinds(&kinds, &ast, false)?;

	let nodes = generate_nodes(&kinds, &ast)?;
	ensure_file_contents(
		&PathBuf::from(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/../crates/jrsonnet-rowan-parser/src/generated/syntax_kinds.rs",
		)),
		&syntax_kinds,
	);
	ensure_file_contents(
		&PathBuf::from(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/../crates/jrsonnet-rowan-parser/src/generated/nodes.rs",
		)),
		&nodes,
	);

	let lexer_syntax_kinds = generate_syntax_kinds(&kinds, &ast, true)?;
	ensure_file_contents(
		&PathBuf::from(concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/../crates/jrsonnet-lexer/src/generated/syntax_kinds.rs",
		)),
		&lexer_syntax_kinds,
	);
	Ok(())
}

#[allow(clippy::too_many_lines)]
fn generate_syntax_kinds(kinds: &KindsSrc, grammar: &AstSrc, lexer: bool) -> Result<String> {
	let t_macros = kinds.tokens().filter_map(TokenKind::expand_t_macros);
	let token_kinds = kinds.tokens().map(|t| t.expand_kind(lexer));

	let keywords = kinds
		.tokens()
		.filter(|k| matches!(k, TokenKind::Keyword { .. }))
		.map(TokenKind::name)
		.map(|n| format_ident!("{n}"));

	let mut nodes = kinds
		.nodes
		.iter()
		.map(|name| format_ident!("{}", name))
		.collect::<Vec<_>>();

	if lexer {
		nodes.clear();
	}

	let enums = grammar
		.enums
		.iter()
		.map(|e| format_ident!("{}", to_upper_snake_case(&e.name)))
		.chain(
			grammar
				.token_enums
				.iter()
				.map(|e| format_ident!("{}", to_upper_snake_case(&e.name))),
		)
		.collect::<Vec<_>>();
	let is_enum = if lexer {
		quote! {}
	} else {
		quote! {
			pub fn is_enum(self) -> bool {
				match self {
					#(#enums)|* => true,
					_ => false,
				}
			}
		}
	};

	let derive_logos = if lexer {
		quote! {
			, logos::Logos
		}
	} else {
		quote! {}
	};

	let error_desc_arms = kinds.tokens().filter_map(|t| {
		if let TokenKind::Error {
			name, description, ..
		} = t
		{
			let ident = format_ident!("{name}");
			Some(quote! { #ident => ::core::option::Option::Some(#description) })
		} else {
			None
		}
	});

	let display_name_arms = kinds.tokens().map(|t| {
		let ident = format_ident!("{}", t.name());
		let display = t.display_name();
		quote! { #ident => #display }
	});

	let ast = quote! {
		#![allow(bad_style, missing_docs, unreachable_pub, clippy::manual_non_exhaustive, clippy::match_like_matches_macro)]

		/// The kind of syntax node, e.g. `IDENT`, `USE_KW`, or `STRUCT`.
		#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug #derive_logos)]
		#[repr(u16)]
		pub enum SyntaxKind {
			#[doc(hidden)]
			TOMBSTONE,
			#[doc(hidden)]
			EOF,
			#(#token_kinds,)*
			LEXING_ERROR,
			__LAST_TOKEN,
			#(#nodes,)*
			#[doc(hidden)]
			__LAST,
		}
		use self::SyntaxKind::*;

		impl SyntaxKind {
			pub fn is_keyword(self) -> bool {
				match self {
					#(#keywords)|* => true,
					_ => false,
				}
			}

			#is_enum

			pub fn error_description(self) -> Option<&'static str> {
				match self {
					#(#error_desc_arms,)*
					LEXING_ERROR => ::core::option::Option::Some("unexpected character"),
					_ => None,
				}
			}

			pub fn display_name(self) -> &'static str {
				match self {
					#(#display_name_arms,)*
					LEXING_ERROR => "unexpected character",
					_ => "unknown",
				}
			}

			pub fn from_raw(r: u16) -> Self {
				assert!(r < Self::__LAST as u16);
				unsafe { std::mem::transmute(r) }
			}
			pub fn into_raw(self) -> u16 {
				self as u16
			}
		}

		#[macro_export]
		macro_rules! T {#(#t_macros);*}
		#[allow(unused_imports)]
		pub use T;
	};

	reformat(&ast.to_string())
}

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
fn generate_nodes(kinds: &KindsSrc, grammar: &AstSrc) -> Result<String> {
	let (node_defs, node_boilerplate_impls): (Vec<_>, Vec<_>) = grammar
		.nodes
		.iter()
		.map(|node| {
			let name = format_ident!("{}", node.name);
			let kind = format_ident!("{}", to_upper_snake_case(&node.name));
			let traits = node.traits.iter().map(|trait_name| {
				let trait_name = format_ident!("{}", trait_name);
				quote!(impl ast::#trait_name for #name {})
			});

			let mut type_positions: HashMap<String, usize> = HashMap::new();
			let field_positions: Vec<_> = node
				.fields
				.iter()
				.map(|field| {
					let ty_str = field.ty().to_string();
					let pos = *type_positions.get(&ty_str).unwrap_or(&0);
					type_positions.insert(ty_str, pos + 1);
					pos
				})
				.collect();

			let methods = node
				.fields
				.iter()
				.zip(field_positions.iter())
				.map(|(field, &pos)| {
					let method_name = field.method_name(kinds);
					let ty = field.ty();

					if field.is_many() {
						quote! {
							pub fn #method_name(&self) -> AstChildren<#ty> {
								support::children(&self.syntax)
							}
						}
					} else if let Some(token_kind) = field.token_kind(kinds) {
						quote! {
							pub fn #method_name(&self) -> Option<#ty> {
								support::token(&self.syntax, #token_kind)
							}
						}
					} else if field.is_token_enum(grammar) {
						quote! {
							pub fn #method_name(&self) -> Option<#ty> {
								support::token_child(&self.syntax)
							}
						}
					} else if pos == 0 {
						quote! {
							pub fn #method_name(&self) -> Option<#ty> {
								support::children(&self.syntax).next()
							}
						}
					} else {
						quote! {
							pub fn #method_name(&self) -> Option<#ty> {
								support::children(&self.syntax).nth(#pos)
							}
						}
					}
				});
			(
				quote! {
					#[pretty_doc_comment_placeholder_workaround]
					#[derive(Debug, Clone, PartialEq, Eq, Hash)]
					pub struct #name {
						pub(crate) syntax: SyntaxNode,
					}

					#(#traits)*

					impl #name {
						#(#methods)*
					}
				},
				quote! {
					impl AstNode for #name {
						fn can_cast(kind: SyntaxKind) -> bool {
							kind == #kind
						}
						fn cast(syntax: SyntaxNode) -> Option<Self> {
							if Self::can_cast(syntax.kind()) { Some(Self { syntax }) } else { None }
						}
						fn syntax(&self) -> &SyntaxNode { &self.syntax }
					}
				},
			)
		})
		.unzip();

	let (enum_defs, enum_boilerplate_impls): (Vec<_>, Vec<_>) = grammar
		.enums
		.iter()
		.map(|en| {
			let variants: Vec<_> = en
				.variants
				.iter()
				.map(|var| format_ident!("{}", var))
				.collect();
			let name = format_ident!("{}", en.name);
			let kinds: Vec<_> = variants
				.iter()
				.map(|name| format_ident!("{}", to_upper_snake_case(&name.to_string())))
				.collect();
			let traits = en.traits.iter().map(|trait_name| {
				let trait_name = format_ident!("{}", trait_name);
				quote!(impl ast::#trait_name for #name {})
			});

			let ast_node = quote! {
				impl AstNode for #name {
					fn can_cast(kind: SyntaxKind) -> bool {
						match kind {
							#(#kinds)|* => true,
							_ => false,
						}
					}
					fn cast(syntax: SyntaxNode) -> Option<Self> {
						let res = match syntax.kind() {
							#(
							#kinds => #name::#variants(#variants { syntax }),
							)*
							_ => return None,
						};
						Some(res)
					}
					fn syntax(&self) -> &SyntaxNode {
						match self {
							#(
							#name::#variants(it) => &it.syntax,
							)*
						}
					}
				}
			};

			(
				quote! {
					#[pretty_doc_comment_placeholder_workaround]
					#[derive(Debug, Clone, PartialEq, Eq, Hash)]
					pub enum #name {
						#(#variants(#variants),)*
					}

					#(#traits)*
				},
				quote! {
					#(
						impl From<#variants> for #name {
							fn from(node: #variants) -> #name {
								#name::#variants(node)
							}
						}
					)*
					#ast_node
				},
			)
		})
		.unzip();

	let (token_enum_defs, token_enum_boilerplate_impls): (Vec<_>, Vec<_>) = grammar
		.token_enums
		.iter()
		.map(|en| {
			let variants: Vec<_> = en
				.variants
				.iter()
				.map(|token| {
					format_ident!(
						"{}",
						to_pascal_case(kinds.token(token).expect("token exists").name())
					)
				})
				.collect();
			let name = format_ident!("{}", en.name);
			let kind_name = format_ident!("{}Kind", en.name);
			let kinds: Vec<_> = variants
				.iter()
				.map(|name| format_ident!("{}", to_upper_snake_case(&name.to_string())))
				.collect();

			let ast_node = quote! {
				impl AstToken for #name {
					fn can_cast(kind: SyntaxKind) -> bool {
						#kind_name::can_cast(kind)
					}
					fn cast(syntax: SyntaxToken) -> Option<Self> {
						let kind = #kind_name::cast(syntax.kind())?;
						Some(#name { syntax, kind })
					}
					fn syntax(&self) -> &SyntaxToken {
						&self.syntax
					}
				}

				impl #kind_name {
					fn can_cast(kind: SyntaxKind) -> bool {
						match kind {
							#(#kinds)|* => true,
							_ => false,
						}
					}
					pub fn cast(kind: SyntaxKind) -> Option<Self> {
						let res = match kind {
							#(#kinds => Self::#variants,)*
							_ => return None,
						};
						Some(res)
					}
				}
			};

			(
				quote! {
					#[pretty_doc_comment_placeholder_workaround]
					#[derive(Debug, Clone, PartialEq, Eq, Hash)]
					pub struct #name { syntax: SyntaxToken, kind: #kind_name }

					#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
					pub enum #kind_name {
						#(#variants,)*
					}
				},
				quote! {
					#ast_node

					impl #name {
						pub fn kind(&self) -> #kind_name {
							self.kind
						}
					}

					impl std::fmt::Display for #name {
						fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
							std::fmt::Display::fmt(self.syntax(), f)
						}
					}
				},
			)
		})
		.unzip();

	let (any_node_defs, any_node_boilerplate_impls): (Vec<_>, Vec<_>) = grammar
		.nodes
		.iter()
		.flat_map(|node| node.traits.iter().map(move |t| (t, node)))
		.into_group_map()
		.into_iter()
		.sorted_by_key(|(k, _)| *k)
		.map(|(trait_name, nodes)| {
			let name = format_ident!("Any{}", trait_name);
			let trait_name = format_ident!("{}", trait_name);
			let kinds: Vec<_> = nodes
				.iter()
				.map(|name| format_ident!("{}", to_upper_snake_case(&name.name)))
				.collect();

			(
				quote! {
					#[pretty_doc_comment_placeholder_workaround]
					#[derive(Debug, Clone, PartialEq, Eq, Hash)]
					pub struct #name {
						pub(crate) syntax: SyntaxNode,
					}
					impl ast::#trait_name for #name {}
				},
				quote! {
					impl #name {
						#[inline]
						pub fn new<T: ast::#trait_name>(node: T) -> #name {
							#name {
								syntax: node.syntax().clone()
							}
						}
					}
					impl AstNode for #name {
						fn can_cast(kind: SyntaxKind) -> bool {
							match kind {
								#(#kinds)|* => true,
								_ => false,
							}
						}
						fn cast(syntax: SyntaxNode) -> Option<Self> {
							Self::can_cast(syntax.kind()).then(|| #name { syntax })
						}
						fn syntax(&self) -> &SyntaxNode {
							&self.syntax
						}
					}
				},
			)
		})
		.unzip();

	let enum_names = grammar.enums.iter().map(|it| &it.name);
	let node_names = grammar.nodes.iter().map(|it| &it.name);

	let display_impls = enum_names
		.chain(node_names.clone())
		.map(|it| format_ident!("{}", it))
		.map(|name| {
			quote! {
				impl std::fmt::Display for #name {
					fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
						std::fmt::Display::fmt(self.syntax(), f)
					}
				}
			}
		});

	let ast = quote! {
		#![allow(non_snake_case, clippy::match_like_matches_macro)]

		use crate::{
			SyntaxNode, SyntaxToken, SyntaxKind::{self, *},
			ast::{AstNode, AstToken, AstChildren, support},
			T,
		};

		#(#node_defs)*
		#(#enum_defs)*
		#(#token_enum_defs)*
		#(#any_node_defs)*
		#(#node_boilerplate_impls)*
		#(#enum_boilerplate_impls)*
		#(#token_enum_boilerplate_impls)*
		#(#any_node_boilerplate_impls)*
		#(#display_impls)*
	};

	let ast = ast.to_string().replace("T ! [", "T![");

	let mut res = String::with_capacity(ast.len() * 2);

	let mut docs = grammar
		.nodes
		.iter()
		.map(|it| &it.doc)
		.chain(grammar.enums.iter().map(|it| &it.doc));

	for chunk in ast.split("# [pretty_doc_comment_placeholder_workaround] ") {
		res.push_str(chunk);
		if let Some(doc) = docs.next() {
			write_doc_comment(doc, &mut res);
		}
	}

	let res = reformat(&res)?;
	Ok(res.replace("#[derive", "\n#[derive"))
}

fn write_doc_comment(contents: &[String], dest: &mut String) {
	use std::fmt::Write;
	for line in contents {
		writeln!(dest, "///{line}").unwrap();
	}
}

pub fn escape_token_macro(token: &str) -> TokenStream {
	if "{}[]()$".contains(token) {
		let c = token.chars().next().unwrap();
		quote! { #c }
	} else if token.contains('$') {
		quote! { #token }
	} else if token.chars().all(|v: char| v.is_ascii_lowercase()) {
		let i = Ident::new(token, Span::call_site());
		quote! { #i }
	} else {
		let cs = token.chars().map(|c| Punct::new(c, Spacing::Joint));
		quote! { #(#cs)* }
	}
}
