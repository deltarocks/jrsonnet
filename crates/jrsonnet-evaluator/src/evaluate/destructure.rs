use std::rc::Rc;

use jrsonnet_gcmodule::Trace;

use crate::{
	Context, LocalsFrame, PackedContext, Result, SupThis, Thunk, Unbound, Val,
	analyze::{
		ClosureShape, LBind, LDestruct, LDestructField, LDestructRest, LLocalExpr, LocalSlot,
	},
	bail,
	evaluate::evaluate,
};

#[allow(dead_code, reason = "not dead in exp-destruct")]
fn destruct_array(
	start: &[LDestruct],
	rest: Option<&LDestructRest>,
	end: &[LDestruct],

	fill: &LocalsFrame,
	value: Thunk<Val>,
	ctx: &Context,
) {
	let min_len = start.len() + end.len();
	let has_rest = rest.is_some();
	let full = Thunk!(move || {
		let v = value.evaluate()?;
		let Val::Arr(arr) = v else {
			bail!("expected array");
		};
		if !has_rest {
			if arr.len() != min_len {
				bail!("expected {} elements, got {}", min_len, arr.len32())
			}
		} else if arr.len() < min_len {
			bail!(
				"expected at least {} elements, but array was only {}",
				min_len,
				arr.len32()
			)
		}
		Ok(arr)
	});

	for (i, d) in start.iter().enumerate() {
		let full = full.clone();
		destruct(
			d,
			fill,
			Thunk!(move || Ok(full.evaluate()?.get(i)?.expect("length is checked"))),
			ctx,
		);
	}

	let start_len = start.len();
	let end_len = end.len();

	if let Some(LDestructRest::Keep(slot)) = rest {
		let full = full.clone();
		fill.set(
			*slot,
			Thunk!(move || {
				let full = full.evaluate()?;
				let to = full.len() - end_len;
				Ok(Val::Arr(full.slice(start_len..to)))
			}),
		);
	}

	for (i, d) in end.iter().enumerate() {
		let full = full.clone();
		destruct(
			d,
			fill,
			Thunk!(move || {
				let full = full.evaluate()?;
				Ok(full
					.get(full.len() - end_len + i)?
					.expect("length is checked"))
			}),
			ctx,
		);
	}
}

#[allow(dead_code, reason = "not dead in exp-destruct")]
fn destruct_object(
	fields: &[LDestructField],
	rest: Option<&LDestructRest>,

	fill: &LocalsFrame,
	value: Thunk<Val>,
	ctx: &Context,
) {
	use jrsonnet_interner::IStr;
	use rustc_hash::FxHashSet;

	use crate::{ObjValueBuilder, bail};

	let captured_fields: FxHashSet<IStr> = fields.iter().map(|f| f.name.clone()).collect();
	let field_names: Vec<(IStr, bool)> = fields
		.iter()
		.map(|f| (f.name.clone(), f.default.is_some()))
		.collect();
	let has_rest = rest.is_some();
	let full = Thunk!(move || {
		let v = value.evaluate()?;
		let Val::Obj(obj) = v else {
			bail!("expected object");
		};
		for (field, has_default) in &field_names {
			if !has_default && !obj.has_field_ex(field.clone(), true) {
				bail!("missing field: {field}");
			}
		}
		if !has_rest {
			let len = obj.len32();
			if len as usize > field_names.len() {
				bail!("too many fields, and rest not found");
			}
		}
		Ok(obj)
	});

	if let Some(LDestructRest::Keep(slot)) = rest {
		let full = full.clone();
		fill.set(
			*slot,
			Thunk!(move || {
				let full = full.evaluate()?;
				let mut out = ObjValueBuilder::new();
				out.extend_with_core(full.as_standalone());
				out.with_fields_omitted(captured_fields);
				Ok(Val::Obj(out.build()))
			}),
		);
	}

	for field in fields {
		let field_name = field.name.clone();
		let default_thunk: Option<Thunk<Val>> = field.default.as_ref().map(|(shape, expr)| {
			let expr = expr.clone();
			let env = Context::enter_using(ctx, shape);
			Thunk!(move || evaluate(env, &expr))
		});

		let field_full = full.clone();
		let value_thunk = Thunk!(move || {
			let obj = field_full.evaluate()?;
			obj.get(field_name)?.map_or_else(
				|| default_thunk.as_ref().expect("shape is checked").evaluate(),
				Ok,
			)
		});

		if let Some(into) = &field.into {
			destruct(into, fill, value_thunk, ctx);
		} else {
			unreachable!("analyzer lowers object-destruct shorthands into `into`");
		}
	}
}

#[allow(unused_variables)]
pub fn destruct(d: &LDestruct, fill: &LocalsFrame, value: Thunk<Val>, a_ctx: &Context) {
	match d {
		LDestruct::Full(slot) => fill.set(*slot, value),
		#[cfg(feature = "exp-destruct")]
		LDestruct::Skip => {}
		#[cfg(feature = "exp-destruct")]
		LDestruct::Array { start, rest, end } => {
			destruct_array(start, rest.as_ref(), end, fill, value, a_ctx)
		}
		#[cfg(feature = "exp-destruct")]
		LDestruct::Object { fields, rest } => destruct_object(fields, rest.as_ref(), fill, value, a_ctx),
	}
}

pub fn fill_letrec_binds(fill: &LocalsFrame, ctx: &Context, binds: &[LBind]) {
	for bind in binds {
		let expr = bind.value.clone();
		let env = Context::enter_using(ctx, &bind.value_shape);
		destruct(
			&bind.destruct,
			fill,
			Thunk!(move || evaluate(env, &expr)),
			ctx,
		);
	}
}

pub fn evaluate_local_expr(parent: Context, l: &LLocalExpr) -> Result<Val> {
	let ctx = parent
		.pack_captures_sup_this(&l.frame_shape)
		.enter(|fill, ctx| {
			fill_letrec_binds(fill, ctx, &l.binds);
		});
	evaluate(ctx, &l.body)
}

pub trait CloneableUnbound<T>: Unbound<Bound = T> + Clone {}
impl<V, T> CloneableUnbound<T> for V where V: Unbound<Bound = T> + Clone {}

pub fn evaluate_locals_unbound(
	outer: &Context,
	frame_shape: &ClosureShape,
	this_slot: Option<LocalSlot>,
	locals: Rc<Vec<LBind>>,
) -> impl CloneableUnbound<Context> {
	#[derive(Trace, Clone)]
	struct UnboundLocals {
		captures: PackedContext,
		this_slot: Option<LocalSlot>,
		locals: Rc<Vec<LBind>>,
	}
	impl Unbound for UnboundLocals {
		type Bound = Context;

		fn bind(&self, sup_this: SupThis) -> Result<Context> {
			Ok(self.captures.clone().enter(sup_this, |fill, ctx| {
				if let Some(slot) = self.this_slot {
					let this_obj = ctx.sup_this().expect("sup_this set above").this().clone();
					fill.set(slot, Thunk::evaluated(Val::Obj(this_obj)));
				}
				fill_letrec_binds(fill, ctx, &self.locals);
			}))
		}
	}

	UnboundLocals {
		captures: outer.pack_captures(frame_shape),
		this_slot,
		locals,
	}
}
