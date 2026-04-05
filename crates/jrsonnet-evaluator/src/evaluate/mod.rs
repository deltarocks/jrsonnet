use std::rc::Rc;

use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::IStr;
use jrsonnet_ir::ImportKind;
use jrsonnet_types::ValType;

use self::{
	compspec::{evaluate_arr_comp, evaluate_obj_comp},
	destructure::{evaluate_locals, evaluate_locals_unbound},
	operator::evaluate_binary_op_special,
};
use crate::{
	analyze::{
		LArgsDesc, LAssertStmt, LExpr, LFieldMember, LFieldName, LFunction, LIndexPart, LObjBody,
		LObjMembers,
	},
	bail,
	error::{suggest_object_fields, ErrorKind::*},
	evaluate::operator::evaluate_unary_op,
	function::{prepared::PreparedFuncVal, CallLocation, FuncDesc, FuncVal},
	in_frame, runtime_error,
	typed::FromUntyped as _,
	val::{CachedUnbound, Thunk},
	with_state, Context, Error, ObjValue, ObjValueBuilder, ObjectAssertion, Result, ResultExt as _,
	SupThis, Unbound, Val,
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

/// Evaluate a method definition.
pub fn evaluate_method(ctx: Context, name: IStr, func: &Rc<LFunction>) -> Val {
	Val::Func(FuncVal::Normal(Cc::new(FuncDesc {
		name,
		ctx,
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

pub fn evaluate_named(name: &IStr, ctx: Context, expr: &LExpr) -> Result<Val> {
	if let LExpr::Function(f) = &expr {
		return Ok(evaluate_method(
			ctx,
			f.name.clone().unwrap_or_else(|| name.clone()),
			f,
		));
	}
	evaluate(ctx, expr)
}

pub fn evaluate(ctx: Context, expr: &LExpr) -> Result<Val> {
	Ok(match expr {
		LExpr::Null => Val::Null,
		LExpr::Bool(b) => Val::Bool(*b),
		LExpr::Str(s) => Val::string(s.clone()),
		LExpr::Num(n) => Val::Num(*n),
		LExpr::Local(id) => {
			let Some(thunk) = ctx.binding(*id) else {
				bail!("should not happen: unbound local {id:?}");
			};
			thunk.evaluate()?
		}
		LExpr::BadLocal(name) => panic!("unresolvable reference: {name}"),
		LExpr::Arr(items) => Val::Arr(crate::arr::ArrValue::expr(ctx, items.clone())),
		LExpr::UnaryOp(op, value) => {
			let value = evaluate(ctx, value)?;
			evaluate_unary_op(*op, &value)?
		}
		LExpr::BinaryOp { lhs, op, rhs } => evaluate_binary_op_special(ctx, lhs, *op, rhs)?,
		LExpr::LocalExpr { binds, body } => {
			let ctx = evaluate_locals(ctx, binds);
			evaluate(ctx, body)?
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
				evaluate(ctx, cond_then)?
			} else if let Some(e) = cond_else {
				evaluate(ctx, e)?
			} else {
				Val::Null
			}
		}
		LExpr::Error(s, e) => in_frame(
			CallLocation::new(s),
			|| "error statement".to_owned(),
			|| bail!(RuntimeError(evaluate(ctx, e)?.to_string()?,)),
		)?,
		LExpr::AssertExpr { assert, rest } => {
			evaluate_assert(ctx.clone(), assert)?;
			evaluate(ctx, rest)?
		}

		LExpr::Function(func) => evaluate_method(
			ctx,
			func.name.clone().unwrap_or_else(names::anonymous),
			func,
		),
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
			use crate::typed::BoundedUsize;
			let val = evaluate(ctx.clone(), &slice.value)?;
			let indexable = val.into_indexable()?;
			let start = slice
				.start
				.as_ref()
				.map(|e| evaluate(ctx.clone(), e))
				.transpose()?
				.map(|v| -> Result<i32> {
					v.as_num()
						.ok_or_else(|| {
							TypeMismatch("slice start", vec![ValType::Num], v.value_type()).into()
						})
						.map(|n| n as i32)
				})
				.transpose()?;
			let end = slice
				.end
				.as_ref()
				.map(|e| evaluate(ctx.clone(), e))
				.transpose()?
				.map(|v| -> Result<i32> {
					v.as_num()
						.ok_or_else(|| {
							TypeMismatch("slice end", vec![ValType::Num], v.value_type()).into()
						})
						.map(|n| n as i32)
				})
				.transpose()?;
			let step = slice
				.step
				.as_ref()
				.map(|e| evaluate(ctx, e))
				.transpose()?
				.map(|v| -> Result<BoundedUsize<1, { i32::MAX as usize }>> {
					let n = v.as_num().ok_or_else(|| -> crate::Error {
						TypeMismatch("slice step", vec![ValType::Num], v.value_type()).into()
					})?;
					BoundedUsize::new(n as usize)
						.ok_or_else(|| runtime_error!("slice step must be >= 1"))
				})
				.transpose()?;
			Val::from(indexable.slice(start, end, step)?)
		}
		LExpr::Super => Val::Obj(ctx.try_sup_this()?.standalone_super()?),
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
	})
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

	let name = func.name();
	let unnamed = args
		.unnamed
		.iter()
		.cloned()
		.map(|e| evaluate_thunk(ctx.clone(), e, tailstrict))
		.collect::<Result<Vec<_>>>()?;

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

fn evaluate_index(ctx: Context, indexable: &LExpr, parts: &[LIndexPart]) -> Result<Val> {
	let mut value = if let LExpr::Super = indexable {
		let sup_this = ctx.try_sup_this()?;
		// First part must be evaluated to get the super field name
		if parts.is_empty() {
			bail!(RuntimeError("super requires an index".into()))
		}
		let key_val = evaluate(ctx.clone(), &parts[0].value)?;
		let Val::Str(key) = &key_val else {
			bail!(ValueIndexMustBeTypeGot(
				ValType::Obj,
				ValType::Str,
				key_val.value_type(),
			))
		};
		let field = key.clone().into_flat();
		if let Some(v) = sup_this.get_super(field.clone())? {
			// Continue with remaining parts
			let mut value = v;
			for part in &parts[1..] {
				value = index_val(ctx.clone(), CallLocation::new(&part.span), value, part)?;
			}
			return Ok(value);
		}
		let suggestions = suggest_object_fields(sup_this.this(), field.clone());
		bail!(NoSuchField(field, suggestions))
	} else {
		evaluate(ctx.clone(), indexable)?
	};

	for part in parts {
		value = index_val(ctx.clone(), CallLocation::new(&part.span), value, part)?;
	}
	Ok(value)
}

fn index_val(ctx: Context, loc: CallLocation<'_>, value: Val, part: &LIndexPart) -> Result<Val> {
	let key_val = evaluate(ctx, &part.value)?;
	Ok(match (&value, &key_val) {
		(Val::Obj(obj), Val::Str(key)) => {
			let field = key.clone().into_flat();
			if let Some(v) = obj
				.get(field.clone())
				.with_description_src(loc, || format!("field <{field}> access"))?
			{
				v
			} else {
				bail!(NoSuchField(
					field.clone(),
					suggest_object_fields(obj, field)
				))
			}
		}
		(Val::Arr(arr), Val::Num(idx)) => {
			let n = idx.get();
			if n.fract() > f64::EPSILON {
				bail!(FractionalIndex)
			}
			if n < 0.0 {
				bail!(ArrayBoundsError(
					n as isize, // truncation is fine for error display
					arr.len()
				));
			}
			#[expect(
				clippy::cast_possible_truncation,
				clippy::cast_sign_loss,
				reason = "n is checked positive"
			)]
			let i = n as u32;
			arr.get(i)
				.with_description_src(loc, || format!("element <{i}> access"))?
				.ok_or_else(|| ArrayBoundsError(i as isize, arr.len()))?
		}
		(Val::Str(s), Val::Num(idx)) => {
			let n = idx.get();
			if n.fract() > f64::EPSILON {
				bail!(FractionalIndex)
			}
			let flat = s.clone().into_flat();
			if n < 0.0 {
				bail!(ArrayBoundsError(
					n as isize, // truncation is fine for error display
					flat.chars().count() as u32
				));
			}
			#[expect(
				clippy::cast_possible_truncation,
				clippy::cast_sign_loss,
				reason = "n is checked positive, overflow will truncate as expected"
			)]
			let i = n as usize;
			let Some(char) = flat.chars().nth(i) else {
				bail!(StringBoundsError(i, flat.chars().count()))
			};
			Val::string(char)
		}
		_ => bail!(ValueIndexMustBeTypeGot(
			value.value_type(),
			ValType::Str,
			key_val.value_type()
		)),
	})
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
		value: Rc<LExpr>,
		name: IStr,
	}
	impl<B: Unbound<Bound = Context>> Unbound for UnboundValue<B> {
		type Bound = Val;
		fn bind(&self, sup_this: SupThis) -> Result<Val> {
			evaluate(self.uctx.bind(sup_this)?, &self.value)
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

	let value = value.clone();
	builder
		.field(name)
		.with_add(*plus)
		.with_visibility(*visibility)
		.try_thunk(Thunk!(move || { evaluate(value_ctx, &value) }))?;
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
			ctx.clone(),
			members.locals.clone(),
			members.this,
		));
		for field in &members.fields {
			evaluate_field_member_unbound(&mut builder, ctx.clone(), uctx.clone(), field)?;
		}
		if !members.asserts.is_empty() {
			builder.assert(evaluate_object_assertions_unbound(
				uctx,
				members.asserts.clone(),
			));
		}
	} else {
		let field_ctx = ctx;
		let value_ctx = evaluate_locals(field_ctx.clone(), &members.locals);
		for field in &members.fields {
			evaluate_field_member_static(
				&mut builder,
				field_ctx.clone(),
				value_ctx.clone(),
				field,
			)?;
		}
		if !members.asserts.is_empty() {
			builder.assert(evaluate_object_assertions_static(
				value_ctx,
				members.asserts.clone(),
			));
		}
	}

	Ok(Val::Obj(builder.build()))
}

pub fn evaluate_assert(ctx: Context, assertion: &LAssertStmt) -> Result<()> {
	let LAssertStmt { cond, message } = assertion;
	let assertion_result = in_frame(
		CallLocation::native(),
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
	asserts: Rc<Vec<LAssertStmt>>,
) -> impl ObjectAssertion {
	#[derive(Trace)]
	struct ObjectAssert<B: Trace> {
		uctx: B,
		asserts: Rc<Vec<LAssertStmt>>,
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
	ObjectAssert { uctx, asserts }
}
fn evaluate_object_assertions_static(
	ctx: Context,
	asserts: Rc<Vec<LAssertStmt>>,
) -> impl ObjectAssertion {
	#[derive(Trace)]
	struct ObjectAssert {
		ctx: Context,
		asserts: Rc<Vec<LAssertStmt>>,
	}
	impl ObjectAssertion for ObjectAssert {
		fn run(&self, _sup_this: SupThis) -> Result<()> {
			for assert in &*self.asserts {
				evaluate_assert(self.ctx.clone(), assert)?;
			}
			Ok(())
		}
	}
	ObjectAssert { ctx, asserts }
}
