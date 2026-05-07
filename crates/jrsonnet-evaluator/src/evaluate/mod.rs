use std::rc::Rc;

use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::IStr;
use jrsonnet_ir::ImportKind;
use jrsonnet_types::ValType;

use self::{
	compspec::{evaluate_arr_comp, evaluate_obj_comp},
	destructure::evaluate_locals_unbound,
	operator::evaluate_binary_op_special,
};
use crate::{
	Context, Error, ObjValue, ObjValueBuilder, ObjectAssertion, Result, ResultExt as _, SupThis,
	Unbound, Val,
	analyze::{
		ClosureShape, LArgsDesc, LAssertStmt, LExpr, LFieldMember, LFieldName, LFunction,
		LIndexPart, LObjAsserts, LObjBody, LObjMembers, LSlot,
	},
	arr::ArrValue,
	bail,
	error::{ErrorKind::*, suggest_object_fields},
	evaluate::{destructure::fill_letrec_binds, operator::evaluate_unary_op},
	function::{CallLocation, FuncDesc, FuncVal, prepared::PreparedFuncVal},
	in_frame,
	typed::{BoundedUsize, FromUntyped as _},
	val::{CachedUnbound, Thunk},
	with_state,
};

pub mod compspec;
pub mod destructure;
pub mod operator;

// This is the amount of bytes that need to be left on the stack before increasing the size.
// It must be at least as large as the stack required by any code that does not call
// `ensure_sufficient_stack`.
const RED_ZONE: usize = 100 * 1024;

// Only the first stack that is pushed, grows exponentially (2^n * STACK_PER_RECURSION) from then
// on. This flag has performance relevant characteristics. Don't set it too high.
const STACK_PER_RECURSION: usize = 1024 * 1024;

/// Grows the stack on demand to prevent stack overflow. Call this in strategic locations
/// to "break up" recursive calls. E.g. almost any call to `visit_expr` or equivalent can benefit
/// from this.
///
/// Should not be sprinkled around carelessly, as it causes a little bit of overhead.
#[inline]
pub fn ensure_sufficient_stack<R>(f: impl FnOnce() -> R) -> R {
	stacker::maybe_grow(RED_ZONE, STACK_PER_RECURSION, f)
}

pub fn evaluate_trivial(expr: &LExpr) -> Option<Val> {
	// TODO: Eager trivial array
	Some(match expr {
		LExpr::Str(s) => Val::string(s.clone()),
		LExpr::Num(n) => Val::Num(*n),
		LExpr::Bool(false) => Val::Bool(false),
		LExpr::Bool(true) => Val::Bool(true),
		LExpr::Null => Val::Null,
		_ => return None,
	})
}

pub fn evaluate_method(ctx: Context, name: IStr, func: &Rc<LFunction>) -> Val {
	Val::Func(FuncVal::Normal(Cc::new(FuncDesc {
		name,
		body_captures: ctx.pack_captures_sup_this(&func.body_shape),
		func: func.clone(),
	})))
}

pub fn evaluate_field_name(ctx: Context, field_name: &LFieldName) -> Result<Option<IStr>> {
	Ok(match field_name {
		LFieldName::Fixed(n) => Some(n.clone()),
		LFieldName::Dyn(expr) => in_frame(
			// TODO: Spanned<LFieldName>
			CallLocation::native(),
			|| "evaluating field name".to_string(),
			|| {
				let v = evaluate(ctx.clone(), expr)?;
				Ok(if matches!(v, Val::Null) {
					None
				} else {
					Some(IStr::from_untyped(v)?)
				})
			},
		)?,
	})
}

pub fn evaluate_thunk(ctx: Context, expr: Rc<LExpr>, tailstrict: bool) -> Result<Thunk<Val>> {
	match &*expr {
		LExpr::Slot(LSlot::Local(i)) => return Ok(ctx.local(*i)),
		LExpr::Slot(LSlot::Capture(i)) => return Ok(ctx.capture(*i)),
		_ => {
			if let Some(v) = evaluate_trivial(&expr) {
				return Ok(Thunk::evaluated(v));
			}
		}
	}
	Ok(if tailstrict {
		Thunk::evaluated(evaluate(ctx, &expr)?)
	} else {
		Thunk!(move || { evaluate(ctx, &expr) })
	})
}

mod names {
	use crate::names;

	names! {
		anonymous: "anonymous",
	}
}

#[allow(clippy::too_many_lines)]
pub fn evaluate(mut ctx: Context, mut expr: &LExpr) -> Result<Val> {
	loop {
		return Ok(match expr {
			LExpr::Null => Val::Null,
			LExpr::Bool(b) => Val::Bool(*b),
			LExpr::Str(s) => Val::string(s.clone()),
			LExpr::Num(n) => Val::Num(*n),
			LExpr::Slot(slot) => ctx.slot(*slot).evaluate()?,
			LExpr::BadLocal(name) => panic!("unresolvable reference: {name}"),
			LExpr::Arr { shape, items } => Val::Arr(ArrValue::expr(ctx, shape, items.clone())),
			LExpr::UnaryOp(op, value) => {
				let value = evaluate(ctx, value)?;
				evaluate_unary_op(*op, &value)?
			}
			LExpr::BinaryOp { lhs, op, rhs } => evaluate_binary_op_special(ctx, lhs, *op, rhs)?,
			LExpr::LocalExpr(l) => {
				ctx = ctx
					.pack_captures_sup_this(&l.frame_shape)
					.enter(|fill, ctx| {
						fill_letrec_binds(fill, ctx, &l.binds);
					});
				expr = &l.body;
				continue;
			}
			LExpr::IfElse {
				cond,
				cond_then,
				cond_else,
			} => {
				let cond_val = evaluate(ctx.clone(), cond)?;
				let Val::Bool(b) = cond_val else {
					bail!(TypeMismatch(
						"if condition",
						vec![ValType::Bool],
						cond_val.value_type()
					))
				};
				if b {
					expr = cond_then;
					continue;
				} else if let Some(e) = cond_else {
					expr = e;
					continue;
				}
				Val::Null
			}
			LExpr::Error(s, e) => in_frame(
				CallLocation::new(s),
				|| "error statement".to_owned(),
				|| bail!(RuntimeError(evaluate(ctx, e)?.to_string()?,)),
			)?,
			LExpr::AssertExpr { assert, rest } => {
				evaluate_assert(ctx.clone(), assert)?;
				expr = rest;
				continue;
			}

			LExpr::Function(func) => evaluate_method(
				ctx,
				func.name.clone().unwrap_or_else(names::anonymous),
				func,
			),
			LExpr::IdentityFunction => Val::Func(FuncVal::identity()),
			LExpr::Apply {
				applicable,
				args,
				tailstrict,
			} => evaluate_apply(
				ctx,
				applicable,
				args,
				CallLocation::new(&args.span),
				*tailstrict,
			)?,
			LExpr::Index { indexable, parts } => evaluate_index(ctx, indexable, parts)?,
			LExpr::Obj(body) => evaluate_obj_body(None, ctx, body)?,
			LExpr::ObjExtend(lhs, body) => {
				let lhs_val = evaluate(ctx.clone(), lhs)?;
				let Val::Obj(lhs_obj) = lhs_val else {
					bail!(TypeMismatch(
						"object extend lhs",
						vec![ValType::Obj],
						lhs_val.value_type(),
					))
				};
				evaluate_obj_body(Some(lhs_obj), ctx, body)?
			}
			LExpr::ArrComp(comp) => evaluate_arr_comp(ctx, comp)?,
			LExpr::Slice(slice) => {
				let val = evaluate(ctx.clone(), &slice.value)?;
				let indexable = val.into_indexable()?;
				let start = slice
					.start
					.as_ref()
					.map(|e| evaluate(ctx.clone(), e))
					.transpose()?
					.map(|v| -> Result<i32> {
						i32::from_untyped(v).description("slice start value")
					})
					.transpose()?;
				let end = slice
					.end
					.as_ref()
					.map(|e| evaluate(ctx.clone(), e))
					.transpose()?
					.map(|v| -> Result<i32> { i32::from_untyped(v).description("slice end value") })
					.transpose()?;
				let step = slice
					.step
					.as_ref()
					.map(|e| evaluate(ctx, e))
					.transpose()?
					.map(|v| -> Result<BoundedUsize<1, { i32::MAX as usize }>> {
						BoundedUsize::from_untyped(v).description("slice step value")
					})
					.transpose()?;
				Val::from(indexable.slice32(start, end, step)?)
			}
			LExpr::Super => Val::Obj(ctx.try_sup_this()?.standalone_super().ok_or(NoSuperFound)?),
			LExpr::Import {
				kind,
				kind_span,
				path,
			} => with_state(|state| {
				let resolved = state.resolve_from(kind_span.0.source_path(), &path.clone())?;
				Ok::<_, Error>(match kind.value {
					ImportKind::Normal => in_frame(
						CallLocation::new(&kind.span),
						|| "import".to_string(),
						|| state.import_resolved(resolved),
					)?,
					ImportKind::Str => Val::string(state.import_resolved_str(resolved)?),
					ImportKind::Bin => Val::arr(state.import_resolved_bin(resolved)?),
				})
			})?,
		});
	}
}

fn evaluate_apply(
	ctx: Context,
	applicable: &LExpr,
	args: &LArgsDesc,
	loc: CallLocation<'_>,
	tailstrict: bool,
) -> Result<Val> {
	let func_val = evaluate(ctx.clone(), applicable)?;
	let Val::Func(func) = func_val else {
		bail!(OnlyFunctionsCanBeCalledGot(func_val.value_type()))
	};

	if func.is_identity() && args.names.is_empty() && args.unnamed.len() == 1 {
		return evaluate_thunk(ctx, args.unnamed[0].clone(), tailstrict)?.evaluate();
	}

	let name = func.name();

	if args.names.is_empty() && args.unnamed.len() == 1 && func.params().len() == 1 {
		use crate::function::prepared::PreparedCall;
		let prepared_inline = PreparedCall::empty();
		let arg = evaluate_thunk(ctx, args.unnamed[0].clone(), tailstrict)?;
		let arg_slice = std::slice::from_ref(&arg);
		return in_frame(
			loc,
			|| format!("function <{name}> call"),
			|| {
				func.evaluate_prepared(
					&prepared_inline,
					CallLocation::native(),
					arg_slice,
					&[],
					tailstrict,
				)
			},
		);
	}

	let unnamed = args
		.unnamed
		.iter()
		.cloned()
		.map(|e| evaluate_thunk(ctx.clone(), e, tailstrict))
		.collect::<Result<Vec<_>>>()?;

	// Fast path: positional-only multi-arg call fully covering the
	// params, no defaults.
	if args.names.is_empty() && unnamed.len() == func.params().len() {
		use crate::function::prepared::PreparedCall;
		let prepared_inline = PreparedCall::empty();
		return in_frame(
			loc,
			|| format!("function <{name}> call"),
			|| {
				func.evaluate_prepared(
					&prepared_inline,
					CallLocation::native(),
					&unnamed,
					&[],
					tailstrict,
				)
			},
		);
	}

	let named = args
		.values
		.iter()
		.cloned()
		.map(|e| evaluate_thunk(ctx.clone(), e, tailstrict))
		.collect::<Result<Vec<_>>>()?;
	let prepare = PreparedFuncVal::new(func, unnamed.len(), &args.names)
		.with_description_src(loc, || format!("function <{name}> preparation"))?;
	in_frame(
		loc,
		|| format!("function <{name}> call"),
		|| prepare.call(CallLocation::native(), &unnamed, &named),
	)
}

#[allow(clippy::too_many_lines)]
fn evaluate_index(ctx: Context, indexable: &LExpr, parts: &[LIndexPart]) -> Result<Val> {
	let mut parts = parts.iter();
	let mut indexable = if matches!(indexable, LExpr::Super) {
		let part = parts.next().expect("at least part should exist");
		// sup_this existence check might also be skipped here for null-coalesce...
		// But I believe this might cause errors.
		let sup_this = ctx.try_sup_this()?;

		if !sup_this.has_super() {
			#[cfg(feature = "exp-null-coaelse")]
			if part.null_coaelse {
				return Ok(Val::Null);
			}
			bail!(NoSuperFound);
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
			.with_description_src(&part.span, || format!("super field <{name}> access"))?
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
		let ctx = ctx.clone();
		let loc = CallLocation::new(&part.span);
		let value = indexable;
		let key_val = evaluate(ctx, &part.value)?;
		indexable = match (&value, &key_val) {
			(Val::Obj(obj), Val::Str(key)) => {
				let key = key.clone().into_flat();
				match obj
					.get(key.clone())
					.with_description_src(loc, || format!("field <{key}> access"))?
				{
					Some(v) => v,
					#[cfg(feature = "exp-null-coaelse")]
					None if part.null_coaelse => return Ok(Val::Null),
					None => {
						return Err(Error::from(NoSuchField(
							key.clone(),
							suggest_object_fields(obj, key.clone()),
						)))
						.with_description_src(loc, || format!("field <{key}> access"));
					}
				}
			}
			(Val::Arr(arr), Val::Num(idx)) => {
				let n = idx.get();
				if n.fract() > f64::EPSILON {
					bail!(FractionalIndex)
				}
				let len = arr.len32();
				if n < 0.0 || n > f64::from(len) {
					bail!(ArrayBoundsError(n, len));
				}
				#[expect(
					clippy::cast_possible_truncation,
					clippy::cast_sign_loss,
					reason = "n is checked range"
				)]
				let i = n as u32;
				arr.get32(i)
					.with_description_src(loc, || format!("element <{i}> access"))?
					.ok_or_else(|| ArrayBoundsError(n, len))?
			}
			(Val::Str(s), Val::Num(idx)) => {
				let n = idx.get();
				if n.fract() > f64::EPSILON {
					bail!(FractionalIndex)
				}
				#[expect(
					clippy::cast_possible_truncation,
					clippy::cast_sign_loss,
					reason = "n is checked positive, overflow will truncate as expected"
				)]
				let i = n as usize;
				let flat = s.clone().into_flat();
				#[allow(clippy::cast_possible_truncation, reason = "string is max 4g")]
				if n >= 0.0
					&& n <= f64::from(u32::MAX)
					&& let Some(char) = flat.chars().nth(i)
				{
					Val::string(char)
				} else {
					let len = flat.chars().count();
					bail!(StringBoundsError(n, len as u32))
				}
			}
			#[cfg(feature = "exp-null-coaelse")]
			(Val::Null, _) if part.null_coaelse => return Ok(Val::Null),
			_ => bail!(ValueIndexMustBeTypeGot(
				value.value_type(),
				ValType::Str,
				key_val.value_type()
			)),
		};
	}
	Ok(indexable)
}

fn evaluate_obj_body(super_obj: Option<ObjValue>, ctx: Context, body: &LObjBody) -> Result<Val> {
	match body {
		LObjBody::MemberList(members) => evaluate_obj_members(super_obj, ctx, members),
		LObjBody::ObjComp(comp) => evaluate_obj_comp(super_obj, ctx, comp),
	}
}

pub fn evaluate_field_member_unbound<B: Unbound<Bound = Context> + Clone>(
	builder: &mut ObjValueBuilder,
	ctx: Context,
	uctx: B,
	field: &LFieldMember,
) -> Result<()> {
	#[derive(Trace)]
	struct UnboundValue<B: Trace> {
		uctx: B,
		value: Rc<(ClosureShape, LExpr)>,
		name: IStr,
	}
	impl<B: Unbound<Bound = Context>> Unbound for UnboundValue<B> {
		type Bound = Val;
		fn bind(&self, sup_this: SupThis) -> Result<Val> {
			let a_ctx = self.uctx.bind(sup_this)?;
			let b_ctx = Context::enter_using(&a_ctx, &self.value.0);
			evaluate(b_ctx, &self.value.1)
		}
	}

	let LFieldMember {
		name,
		plus,
		visibility,
		value,
	} = field;
	let Some(name) = evaluate_field_name(ctx, name)? else {
		return Ok(());
	};

	builder
		.field(name.clone())
		.with_add(*plus)
		.with_visibility(*visibility)
		.bindable(UnboundValue {
			uctx,
			value: value.clone(),
			name,
		})
}
pub fn evaluate_field_member_static(
	builder: &mut ObjValueBuilder,
	field_ctx: Context,
	value_ctx: Context,
	field: &LFieldMember,
) -> Result<()> {
	let LFieldMember {
		name,
		plus,
		visibility,
		value,
	} = field;
	let Some(name) = evaluate_field_name(field_ctx, name)? else {
		return Ok(());
	};

	let env = Context::enter_using(&value_ctx, &value.0);
	let value = value.clone();
	builder
		.field(name)
		.with_add(*plus)
		.with_visibility(*visibility)
		.try_thunk(Thunk!(move || evaluate(env, &value.1)))?;
	Ok(())
}

fn evaluate_obj_members(
	super_obj: Option<ObjValue>,
	ctx: Context,
	members: &LObjMembers,
) -> Result<Val> {
	let mut builder = ObjValueBuilder::with_capacity(members.fields.len());
	if let Some(sup) = super_obj {
		builder.with_super(sup);
	}

	let needs_unbound = members.this.is_some() || members.uses_super;

	if needs_unbound {
		let uctx = CachedUnbound::new(evaluate_locals_unbound(
			&ctx,
			&members.frame_shape,
			members.this,
			members.locals.clone(),
		));
		for field in &members.fields {
			evaluate_field_member_unbound(&mut builder, ctx.clone(), uctx.clone(), field)?;
		}
		if let Some(asserts_block) = &members.asserts {
			builder.assert(evaluate_object_assertions_unbound(
				uctx,
				asserts_block.clone(),
			));
		}
	} else {
		let a_ctx = ctx
			.pack_captures_sup_this(&members.frame_shape)
			.enter(|fill, ctx| {
				fill_letrec_binds(fill, ctx, &members.locals);
			});
		for field in &members.fields {
			evaluate_field_member_static(&mut builder, ctx.clone(), a_ctx.clone(), field)?;
		}
		if let Some(asserts_block) = &members.asserts {
			builder.assert(evaluate_object_assertions_static(
				a_ctx,
				asserts_block.clone(),
			));
		}
	}

	Ok(Val::Obj(builder.build()))
}

pub fn evaluate_assert(ctx: Context, assertion: &LAssertStmt) -> Result<()> {
	let LAssertStmt { cond, message } = assertion;
	let assertion_result = in_frame(
		CallLocation::new(&cond.span),
		|| "assertion condition".to_owned(),
		|| bool::from_untyped(evaluate(ctx.clone(), cond)?),
	)?;
	if !assertion_result {
		in_frame(
			CallLocation::new(&cond.span),
			|| "assertion failure".to_owned(),
			|| {
				if let Some(msg) = message {
					bail!(AssertionFailed(evaluate(ctx, msg)?.to_string()?));
				}
				bail!(AssertionFailed(Val::Null.to_string()?));
			},
		)?;
	}
	Ok(())
}

fn evaluate_object_assertions_unbound<B: Unbound<Bound = Context>>(
	uctx: B,
	asserts: Rc<LObjAsserts>,
) -> impl ObjectAssertion {
	#[derive(Trace)]
	struct ObjectAssert<B: Trace> {
		uctx: B,
		asserts: Rc<LObjAsserts>,
	}
	impl<B: Unbound<Bound = Context>> ObjectAssertion for ObjectAssert<B> {
		fn run(&self, sup_this: SupThis) -> Result<()> {
			let a_ctx = self.uctx.bind(sup_this)?;
			let assert_env = Context::enter_using(&a_ctx, &self.asserts.shape);
			for assert in &self.asserts.asserts {
				evaluate_assert(assert_env.clone(), assert)?;
			}
			Ok(())
		}
	}
	ObjectAssert { uctx, asserts }
}
fn evaluate_object_assertions_static(
	a_ctx: Context,
	asserts: Rc<LObjAsserts>,
) -> impl ObjectAssertion {
	#[derive(Trace)]
	struct ObjectAssert {
		assert_env: Context,
		asserts: Rc<LObjAsserts>,
	}
	impl ObjectAssertion for ObjectAssert {
		fn run(&self, _sup_this: SupThis) -> Result<()> {
			for assert in &self.asserts.asserts {
				evaluate_assert(self.assert_env.clone(), assert)?;
			}
			Ok(())
		}
	}
	let assert_env = Context::enter_using(&a_ctx, &asserts.shape);
	ObjectAssert {
		assert_env,
		asserts,
	}
}
