use std::rc::Rc;

use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::IStr;
use jrsonnet_ir::{
	ArgsDesc, AssertStmt, BinaryOpType, BindSpec, CompSpec, Expr, ExprParams, FieldMember,
	FieldName, ForSpecData, IfSpecData, ImportKind, LiteralType, ObjBody, ObjMembers, Spanned,
	function::ParamName,
};
use jrsonnet_types::ValType;
use rustc_hash::FxHashMap;

use self::destructure::destruct;
use crate::{
	Context, Error, ObjValue, ObjValueBuilder, ObjectAssertion, Pending, Result, ResultExt,
	SupThis, Unbound, Val,
	arr::ArrValue,
	bail,
	destructure::evaluate_dest,
	error::{ErrorKind::*, suggest_object_fields},
	evaluate::operator::{evaluate_binary_op_special, evaluate_unary_op},
	function::{CallLocation, FuncDesc, FuncVal},
	gc::WithCapacityExt as _,
	in_frame,
	typed::{FromUntyped, IntoUntyped as _, Typed},
	val::{CachedUnbound, IndexableVal, NumValue, StrValue, Thunk},
	with_state,
};
pub mod destructure;
pub mod operator;

// This is the amount of bytes that need to be left on the stack before increasing the size.
// It must be at least as large as the stack required by any code that does not call
// `ensure_sufficient_stack`.
const RED_ZONE: usize = 100 * 1024; // 100k

// Only the first stack that is pushed, grows exponentially (2^n * STACK_PER_RECURSION) from then
// on. This flag has performance relevant characteristics. Don't set it too high.
const STACK_PER_RECURSION: usize = 1024 * 1024; // 1MB

/// Grows the stack on demand to prevent stack overflow. Call this in strategic locations
/// to "break up" recursive calls. E.g. almost any call to `visit_expr` or equivalent can benefit
/// from this.
///
/// Should not be sprinkled around carelessly, as it causes a little bit of overhead.
#[inline]
pub fn ensure_sufficient_stack<R>(f: impl FnOnce() -> R) -> R {
	stacker::maybe_grow(RED_ZONE, STACK_PER_RECURSION, f)
}

pub fn evaluate_trivial(expr: &Expr) -> Option<Val> {
	fn is_trivial(expr: &Expr) -> bool {
		match expr {
			Expr::Str(_)
			| Expr::Num(_)
			| Expr::Literal(LiteralType::False | LiteralType::True | LiteralType::Null) => true,
			Expr::Arr(a) => a.iter().all(is_trivial),
			_ => false,
		}
	}
	Some(match expr {
		Expr::Str(s) => Val::string(s.clone()),
		Expr::Num(n) => {
			Val::Num(NumValue::new(*n).expect("parser will not allow non-finite values"))
		}
		Expr::Literal(LiteralType::False) => Val::Bool(false),
		Expr::Literal(LiteralType::True) => Val::Bool(true),
		Expr::Literal(LiteralType::Null) => Val::Null,
		Expr::Arr(n) => {
			if n.iter().any(|e| !is_trivial(e)) {
				return None;
			}
			Val::Arr(
				n.iter()
					.map(evaluate_trivial)
					.map(|e| e.expect("checked trivial"))
					.collect(),
			)
		}
		_ => return None,
	})
}

pub fn evaluate_method(ctx: Context, name: IStr, params: ExprParams, body: Rc<Expr>) -> Val {
	Val::Func(FuncVal::Normal(Cc::new(FuncDesc {
		name,
		ctx,
		params,
		body,
	})))
}

pub fn evaluate_field_name(ctx: Context, field_name: &Spanned<FieldName>) -> Result<Option<IStr>> {
	Ok(match &field_name.value {
		FieldName::Fixed(n) => Some(n.clone()),
		FieldName::Dyn(expr) => in_frame(
			CallLocation::new(&field_name.span),
			|| "evaluating field name".to_string(),
			|| {
				let v = evaluate(ctx, expr)?;
				Ok(if matches!(v, Val::Null) {
					None
				} else {
					Some(IStr::from_untyped(v)?)
				})
			},
		)?,
	})
}

pub fn evaluate_comp(
	ctx: Context,
	specs: &[CompSpec],
	mut guaranteed_reserve: usize,
	callback: &mut impl FnMut(Context, usize) -> Result<()>,
) -> Result<()> {
	match specs.first() {
		None => callback(ctx, guaranteed_reserve)?,
		Some(CompSpec::IfSpec(IfSpecData { cond, span: _ })) => {
			if bool::from_untyped(evaluate(ctx.clone(), cond)?)? {
				evaluate_comp(ctx, &specs[1..], 0, callback)?;
			}
		}
		Some(CompSpec::ForSpec(ForSpecData {
			destruct: into,
			over,
		})) => {
			match evaluate(ctx.clone(), over)? {
				Val::Arr(list) => {
					guaranteed_reserve = guaranteed_reserve.max(1) * list.len();
					for (i, item) in list.iter_lazy().enumerate() {
						let fctx = Pending::new();
						let mut new_bindings = FxHashMap::with_capacity(into.binds_len());
						destruct(into, item, fctx.clone(), &mut new_bindings)?;
						let ctx = ctx.clone().extend_bindings(new_bindings).into_future(fctx);

						let specs = &specs[1..];
						evaluate_comp(
							ctx,
							specs,
							if i == 0 || !specs.is_empty() {
								guaranteed_reserve
							} else {
								0
							},
							callback,
						)?;
					}
				}
				#[cfg(feature = "exp-object-iteration")]
				Val::Obj(obj) => {
					let fields = obj.fields(
						// TODO: Should there be ability to preserve iteration order?
						#[cfg(feature = "exp-preserve-order")]
						false,
					);
					guaranteed_reserve = guaranteed_reserve.max(1) * fields.len();
					for field in fields {
						let fctx = Pending::new();
						let mut new_bindings = FxHashMap::with_capacity(into.binds_len());
						let obj = obj.clone();
						let value = Thunk::evaluated(Val::arr(vec![
							Thunk::evaluated(Val::string(field.clone())),
							obj.get_lazy(field).transpose().expect(
								"field exists, as field name was obtained from object.fields()",
							),
						]));
						destruct(into, value, fctx.clone(), &mut new_bindings)?;
						let ctx = ctx.clone().extend_bindings(new_bindings).into_future(fctx);

						evaluate_comp(ctx, &specs[1..], callback)?;
					}
				}
				_ => bail!(InComprehensionCanOnlyIterateOverArray),
			}
		}
	}
	Ok(())
}

fn evaluate_arr_comp(ctx: Context, expr: &Rc<Expr>, comp_specs: &[CompSpec]) -> Result<ArrValue> {
	'eager: {
		let mut out = Vec::new();

		if evaluate_comp(ctx.clone(), comp_specs, 0, &mut |ctx, reserve| {
			if reserve != 0 {
				out.reserve(reserve);
			}
			out.push(evaluate(ctx, expr)?);
			Ok(())
		})
		.is_err()
		{
			break 'eager;
		}

		return Ok(ArrValue::new(out));
	};
	let mut out = Vec::new();
	evaluate_comp(ctx, comp_specs, 0, &mut |ctx, reserve| {
		if reserve != 0 {
			out.reserve(reserve);
		}
		let expr = expr.clone();
		out.push(Thunk!(move || evaluate(ctx, &expr)));
		Ok(())
	})?;
	Ok(ArrValue::new(out))
}

trait CloneableUnbound<T>: Unbound<Bound = T> + Clone {}
impl<V, T> CloneableUnbound<T> for V where V: Unbound<Bound = T> + Clone {}

fn evaluate_object_locals(
	fctx: Context,
	locals: Rc<Vec<BindSpec>>,
) -> impl CloneableUnbound<Context> {
	#[derive(Trace, Clone)]
	struct UnboundLocals {
		fctx: Context,
		locals: Rc<Vec<BindSpec>>,
	}
	impl Unbound for UnboundLocals {
		type Bound = Context;

		fn bind(&self, sup_this: SupThis) -> Result<Context> {
			let fctx = Context::new_future();
			let mut new_bindings =
				FxHashMap::with_capacity(self.locals.iter().map(BindSpec::binds_len).sum());
			for b in self.locals.iter() {
				evaluate_dest(b, fctx.clone(), &mut new_bindings)?;
			}

			let ctx = self.fctx.clone();

			let ctx = ctx
				.extend_bindings_sup_this(new_bindings, sup_this)
				.into_future(fctx);

			Ok(ctx)
		}
	}

	UnboundLocals { fctx, locals }
}

pub fn evaluate_field_member<B: Unbound<Bound = Context> + Clone>(
	builder: &mut ObjValueBuilder,
	ctx: Context,
	uctx: B,
	field: &FieldMember,
) -> Result<()> {
	let name = evaluate_field_name(ctx, &field.name)?;
	let Some(name) = name else {
		return Ok(());
	};

	match field {
		FieldMember {
			plus,
			params: None,
			visibility,
			value,
			..
		} => {
			#[derive(Trace)]
			struct UnboundValue<B: Trace> {
				uctx: B,
				value: Rc<Expr>,
				name: IStr,
			}
			impl<B: Unbound<Bound = Context>> Unbound for UnboundValue<B> {
				type Bound = Val;
				fn bind(&self, sup_this: SupThis) -> Result<Val> {
					evaluate_named(self.uctx.bind(sup_this)?, &self.value, self.name.clone())
				}
			}

			builder
				.field(name.clone())
				.with_add(*plus)
				.with_visibility(*visibility)
				.with_location(field.name.span.clone())
				.bindable(UnboundValue {
					uctx,
					value: value.clone(),
					name,
				})?;
		}
		FieldMember {
			params: Some(params),
			visibility,
			value,
			..
		} => {
			#[derive(Trace)]
			struct UnboundMethod<B: Trace> {
				uctx: B,
				value: Rc<Expr>,
				params: ExprParams,
				name: IStr,
			}
			impl<B: Unbound<Bound = Context>> Unbound for UnboundMethod<B> {
				type Bound = Val;
				fn bind(&self, sup_this: SupThis) -> Result<Val> {
					Ok(evaluate_method(
						self.uctx.bind(sup_this)?,
						self.name.clone(),
						self.params.clone(),
						self.value.clone(),
					))
				}
			}

			builder
				.field(name.clone())
				.with_visibility(*visibility)
				// .with_location(value.span())
				.bindable(UnboundMethod {
					uctx,
					value: value.clone(),
					params: params.clone(),
					name,
				})?;
		}
	}
	Ok(())
}

#[derive(Trace, Clone)]
struct DirectUnbound(Context);
impl Unbound for DirectUnbound {
	type Bound = Context;
	fn bind(&self, sup_this: SupThis) -> Result<Context> {
		Ok(self
			.0
			.clone()
			.extend_bindings_sup_this(FxHashMap::new(), sup_this))
	}
}

#[allow(clippy::too_many_lines)]
pub fn evaluate_member_list_object(
	super_obj: Option<ObjValue>,
	ctx: Context,
	members: &ObjMembers,
) -> Result<ObjValue> {
	#[derive(Trace)]
	struct ObjectAssert<B: Trace> {
		uctx: B,
		asserts: Rc<Vec<AssertStmt>>,
	}
	impl<B: Unbound<Bound = Context>> ObjectAssertion for ObjectAssert<B> {
		fn run(&self, sup_this: SupThis) -> Result<()> {
			let ctx = self.uctx.bind(sup_this)?;
			for assert in &*self.asserts {
				evaluate_assert(ctx.clone(), assert)?;
			}
			Ok(())
		}
	}

	let mut builder = ObjValueBuilder::new();
	if let Some(super_obj) = super_obj {
		builder.with_super(super_obj);
	}

	if members.locals.is_empty() {
		// We can use the same context for all field evaluation, it doesn't depends on locals, only on this/super
		let uctx = DirectUnbound(ctx.clone());
		for field in &members.fields {
			evaluate_field_member(&mut builder, ctx.clone(), uctx.clone(), field)?;
		}
		if !members.asserts.is_empty() {
			builder.assert(ObjectAssert {
				uctx,
				asserts: members.asserts.clone(),
			});
		}
	} else {
		let locals = members.locals.clone();
		// We have single context for all fields, so we can cache them together
		let uctx = CachedUnbound::new(evaluate_object_locals(ctx.clone(), locals));
		for field in &members.fields {
			evaluate_field_member(&mut builder, ctx.clone(), uctx.clone(), field)?;
		}
		if !members.asserts.is_empty() {
			builder.assert(ObjectAssert {
				uctx,
				asserts: members.asserts.clone(),
			});
		}
	}

	Ok(builder.build())
}

pub fn evaluate_object(
	super_obj: Option<ObjValue>,
	ctx: Context,
	object: &ObjBody,
) -> Result<ObjValue> {
	Ok(match object {
		ObjBody::MemberList(members) => evaluate_member_list_object(super_obj, ctx, members)?,
		ObjBody::ObjComp(obj) => {
			let mut builder = ObjValueBuilder::new();
			if let Some(super_obj) = super_obj {
				builder.with_super(super_obj);
			}
			let locals = obj.locals.clone();
			evaluate_comp(ctx, &obj.compspecs, 0, &mut |ctx, reserve| {
				let uctx = evaluate_object_locals(ctx.clone(), locals.clone());
				builder.reserve_cores(reserve);

				evaluate_field_member(&mut builder, ctx, uctx, &obj.field)
			})?;

			builder.build()
		}
	})
}

pub fn evaluate_apply(
	ctx: Context,
	value: &Expr,
	args: &ArgsDesc,
	loc: CallLocation<'_>,
	tailstrict: bool,
) -> Result<Val> {
	let value = evaluate(ctx.clone(), value)?;
	Ok(match value {
		Val::Func(f) => {
			let body = || f.evaluate(ctx, loc, args, tailstrict);
			if tailstrict {
				body()?
			} else {
				in_frame(loc, || format!("function <{}> call", f.name()), body)?
			}
		}
		v => bail!(OnlyFunctionsCanBeCalledGot(v.value_type())),
	})
}

pub fn evaluate_assert(ctx: Context, assertion: &AssertStmt) -> Result<()> {
	let value = &assertion.0;
	let msg = &assertion.1;
	let assertion_result = in_frame(
		CallLocation::new(&value.span),
		|| "assertion condition".to_owned(),
		|| bool::from_untyped(evaluate(ctx.clone(), value)?),
	)?;
	if !assertion_result {
		in_frame(
			CallLocation::new(&value.span),
			|| "assertion failure".to_owned(),
			|| {
				if let Some(msg) = msg {
					bail!(AssertionFailed(evaluate(ctx, msg)?.to_string()?));
				}
				bail!(AssertionFailed(Val::Null.to_string()?));
			},
		)?;
	}
	Ok(())
}

pub fn evaluate_named_param(ctx: Context, expr: &Expr, name: ParamName) -> Result<Val> {
	match name {
		ParamName::Named(name) => evaluate_named(ctx, expr, name),
		ParamName::Unnamed => evaluate(ctx, expr),
	}
}

pub fn evaluate_named(ctx: Context, expr: &Expr, name: IStr) -> Result<Val> {
	use Expr::*;
	Ok(match expr {
		Function(params, body) => evaluate_method(ctx, name, params.clone(), body.clone()),
		_ => evaluate(ctx, expr)?,
	})
}

#[allow(clippy::too_many_lines)]
pub fn evaluate(ctx: Context, expr: &Expr) -> Result<Val> {
	use Expr::*;

	Ok(match expr {
		Literal(LiteralType::This) => Val::Obj(ctx.try_this()?),
		Literal(LiteralType::Super) => Val::Obj(ctx.try_sup_this()?.standalone_super()?),
		Literal(LiteralType::Dollar) => Val::Obj(ctx.try_dollar()?),
		Literal(LiteralType::True) => Val::Bool(true),
		Literal(LiteralType::False) => Val::Bool(false),
		Literal(LiteralType::Null) => Val::Null,
		Str(v) => Val::string(v.clone()),
		Num(v) => Val::try_num(*v)?,
		// I have tried to remove special behavior from super by implementing standalone-super
		// expresion, but looks like this case still needs special treatment.
		//
		// Note that other jsonnet implementations will fail on `if value in (super)` expression,
		// because the standalone super literal is not supported, that is because in other
		// implementations `in super` treated differently from `in smth_else`.
		BinaryOp(bin)
			if matches!(&bin.rhs, Expr::Literal(LiteralType::Super))
				&& bin.op == BinaryOpType::In =>
		{
			let sup_this = ctx.try_sup_this()?;
			// In jsonnet, "field" in e is eager, LHS expression is always executed regardless of super existence.
			// In jrsonnet, however, this wasn't true, this was kept here for compatibility.
			if !sup_this.has_super() {
				return Ok(Val::Bool(false));
			}
			let field = evaluate(ctx, &bin.lhs)?;
			Val::Bool(sup_this.field_in_super(field.to_string()?))
		}
		BinaryOp(bin) => evaluate_binary_op_special(ctx, &bin.lhs, bin.op, &bin.rhs)?,
		UnaryOp(o, v) => evaluate_unary_op(*o, &evaluate(ctx, v)?)?,
		Var(name) => in_frame(
			CallLocation::new(&name.span),
			|| format!("local <{}> access", &**name),
			|| ctx.binding((**name).clone())?.evaluate(),
		)?,
		Index { indexable, parts } => ensure_sufficient_stack(|| {
			let mut parts = parts.iter();
			let mut indexable = if matches!(&**indexable, Expr::Literal(LiteralType::Super)) {
				let part = parts.next().expect("at least part should exist");
				// sup_this existence check might also be skipped here for null-coalesce...
				// But I believe this might cause errors.
				let sup_this = ctx.try_sup_this()?;
				if !sup_this.has_super() {
					#[cfg(feature = "exp-null-coaelse")]
					if part.null_coaelse {
						return Ok(Val::Null);
					}
					bail!(NoSuperFound)
				}
				let name = evaluate(ctx.clone(), &part.value)?;

				let Val::Str(name) = name else {
					bail!(ValueIndexMustBeTypeGot(
						ValType::Obj,
						ValType::Str,
						name.value_type(),
					))
				};

				let name = name.into_flat();
				match sup_this
					.get_super(name.clone())
					.with_description_src(&part.span, || format!("field <{name}> access"))?
				{
					Some(v) => v,
					#[cfg(feature = "exp-null-coaelse")]
					None if part.null_coaelse => return Ok(Val::Null),
					None => {
						let suggestions = suggest_object_fields(
							&sup_this.standalone_super().expect("super exists"),
							name.clone(),
						);

						bail!(NoSuchField(name, suggestions))
					}
				}
			} else {
				evaluate(ctx.clone(), indexable)?
			};

			for part in parts {
				indexable = match (indexable, evaluate(ctx.clone(), &part.value)?) {
					(Val::Obj(v), Val::Str(key)) => match v
						.get(key.clone().into_flat())
						.with_description_src(&part.span, || format!("field <{key}> access"))?
					{
						Some(v) => v,
						#[cfg(feature = "exp-null-coaelse")]
						None if part.null_coaelse => return Ok(Val::Null),
						None => {
							let suggestions = suggest_object_fields(&v, key.into_flat());

							return Err(Error::from(NoSuchField(
								key.clone().into_flat(),
								suggestions,
							)))
							.with_description_src(&part.span, || format!("field <{key}> access"));
						}
					},
					(Val::Obj(_), n) => bail!(ValueIndexMustBeTypeGot(
						ValType::Obj,
						ValType::Str,
						n.value_type(),
					)),
					(Val::Arr(v), Val::Num(n)) => {
						let n = n.get();
						if n.fract() > f64::EPSILON {
							bail!(FractionalIndex)
						}
						if n < 0.0 {
							#[expect(
								clippy::cast_possible_truncation,
								reason = "it would be truncated anyway"
							)]
							let n = n as isize;
							bail!(ArrayBoundsError(n, v.len()));
						}
						#[expect(
							clippy::cast_possible_truncation,
							clippy::cast_sign_loss,
							reason = "n is checked postive"
						)]
						v.get(n as usize)?
							.ok_or_else(|| ArrayBoundsError(n as isize, v.len()))?
					}
					(Val::Arr(_), Val::Str(n)) => {
						bail!(AttemptedIndexAnArrayWithString(n.into_flat()))
					}
					(Val::Arr(_), n) => bail!(ValueIndexMustBeTypeGot(
						ValType::Arr,
						ValType::Num,
						n.value_type(),
					)),

					(Val::Str(s), Val::Num(n)) => Val::Str({
						let n = n.get();
						if n.fract() > f64::EPSILON {
							bail!(FractionalIndex)
						}
						if n < 0.0 {
							#[expect(
								clippy::cast_possible_truncation,
								reason = "it would be truncated anyway"
							)]
							let n = n as isize;
							bail!(ArrayBoundsError(n, s.into_flat().chars().count()));
						}
						#[expect(
							clippy::cast_sign_loss,
							clippy::cast_possible_truncation,
							reason = "n is positive, overflow will truncate as expected"
						)]
						let n = n as usize;
						let v: IStr = s
							.clone()
							.into_flat()
							.chars()
							.skip(n)
							.take(1)
							.collect::<String>()
							.into();
						if v.is_empty() {
							bail!(StringBoundsError(n, s.into_flat().chars().count()))
						}
						StrValue::Flat(v)
					}),
					(Val::Str(_), n) => bail!(ValueIndexMustBeTypeGot(
						ValType::Str,
						ValType::Num,
						n.value_type(),
					)),
					#[cfg(feature = "exp-null-coaelse")]
					(Val::Null, _) if part.null_coaelse => return Ok(Val::Null),
					(v, _) => bail!(CantIndexInto(v.value_type())),
				};
			}
			Ok(indexable)
		})?,
		LocalExpr(bindings, returned) => {
			let mut new_bindings: FxHashMap<IStr, Thunk<Val>> =
				FxHashMap::with_capacity(bindings.iter().map(BindSpec::binds_len).sum());
			let fctx = Context::new_future();
			for b in bindings {
				evaluate_dest(b, fctx.clone(), &mut new_bindings)?;
			}
			let ctx = ctx.extend_bindings(new_bindings).into_future(fctx);
			evaluate(ctx, returned)?
		}
		Arr(items) => {
			if items.is_empty() {
				Val::arr(())
			} else {
				Val::Arr(ArrValue::expr(ctx, items.clone()))
			}
		}
		ArrComp(expr, comp_specs) => Val::Arr(evaluate_arr_comp(ctx, expr, comp_specs)?),
		Obj(body) => Val::Obj(evaluate_object(None, ctx, body)?),
		ObjExtend(a, b) => {
			let base = evaluate(ctx.clone(), a)?;
			match base {
				Val::Obj(base_obj) => Val::Obj(evaluate_object(Some(base_obj), ctx, b)?),
				_ => bail!("ObjExtend lhs should be an object value"),
			}
		}
		Apply(value, args, tailstrict) => ensure_sufficient_stack(|| {
			evaluate_apply(ctx, value, args, CallLocation::new(&args.span), *tailstrict)
		})?,
		Function(params, body) => {
			evaluate_method(ctx, "anonymous".into(), params.clone(), body.clone())
		}
		AssertExpr(assert) => {
			evaluate_assert(ctx.clone(), &assert.assert)?;
			evaluate(ctx, &assert.rest)?
		}
		ErrorStmt(s, e) => in_frame(
			CallLocation::new(s),
			|| "error statement".to_owned(),
			|| bail!(RuntimeError(evaluate(ctx, e)?.to_string()?,)),
		)?,
		IfElse(if_else) => {
			if in_frame(
				CallLocation::new(&if_else.cond.span),
				|| "if condition".to_owned(),
				|| bool::from_untyped(evaluate(ctx.clone(), &if_else.cond.cond)?),
			)? {
				evaluate(ctx, &if_else.cond_then)?
			} else {
				match &if_else.cond_else {
					Some(v) => evaluate(ctx, v)?,
					None => Val::Null,
				}
			}
		}
		Slice(slice) => {
			fn parse_idx<T: Typed + FromUntyped>(
				ctx: Context,
				expr: Option<&Spanned<Expr>>,
				desc: &'static str,
			) -> Result<Option<T>> {
				if let Some(value) = expr {
					Ok(in_frame(
						CallLocation::new(&value.span),
						|| format!("slice {desc}"),
						|| <Option<T>>::from_untyped(evaluate(ctx, value)?),
					)?)
				} else {
					Ok(None)
				}
			}

			let indexable = evaluate(ctx.clone(), &slice.value)?;

			let start = parse_idx(ctx.clone(), slice.slice.start.as_ref(), "start")?;
			let end = parse_idx(ctx.clone(), slice.slice.end.as_ref(), "end")?;
			let step = parse_idx(ctx, slice.slice.step.as_ref(), "step")?;

			IndexableVal::into_untyped(indexable.into_indexable()?.slice(start, end, step)?)?
		}
		Import(kind, path) => {
			let Expr::Str(path) = &**path else {
				bail!("computed imports are not supported")
			};
			with_state(|s| {
				let span = &kind.span;
				let resolved_path = s.resolve_from(span.0.source_path(), path)?;
				Ok(match &**kind {
					ImportKind::Normal => in_frame(
						CallLocation::new(span),
						|| format!("import {:?}", path.clone()),
						|| s.import_resolved(resolved_path),
					)?,
					ImportKind::Str => Val::string(s.import_resolved_str(resolved_path)?),
					ImportKind::Bin => Val::arr(s.import_resolved_bin(resolved_path)?),
				}) as Result<Val>
			})?
		}
	})
}
