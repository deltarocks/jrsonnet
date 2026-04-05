use std::rc::Rc;

use jrsonnet_gcmodule::Trace;

use crate::{
	analyze::{LBind, LDestruct, LDestructField, LDestructRest, LExpr, LocalId},
	bail,
	evaluate::evaluate,
	Context, ContextBuilder, Pending, Result, SupThis, Thunk, Unbound, Val,
};

#[allow(dead_code, reason = "not dead in exp-destruct")]
fn destruct_array(
	start: &[LDestruct],
	rest: Option<LDestructRest>,
	end: &[LDestruct],

	value: Thunk<Val>,
	fctx: Pending<Context>,
	builder: &mut ContextBuilder,
) {
	let min_len = start.len() + end.len();
	let has_rest = rest.is_some();
	let full = Thunk!(move || {
		let v = value.evaluate()?;
		let Val::Arr(arr) = v else {
			bail!("expected array");
		};
		if !has_rest {
			if arr.len() as usize != min_len {
				bail!("expected {} elements, got {}", min_len, arr.len())
			}
		} else if (arr.len() as usize) < min_len {
			bail!(
				"expected at least {} elements, but array was only {}",
				min_len,
				arr.len()
			)
		}
		Ok(arr)
	});

	for (i, d) in start.iter().enumerate() {
		let full = full.clone();
		destruct(
			d,
			Thunk!(move || Ok(full.evaluate()?.get(i as u32)?.expect("length is checked"))),
			fctx.clone(),
			builder,
		);
	}

	let start_len = start.len() as u32;
	let end_len = end.len() as u32;

	if let Some(crate::analyze::LDestructRest::Keep(id)) = rest {
		let full = full.clone();
		builder.bind(
			id,
			Thunk!(move || {
				let full = full.evaluate()?;
				let to = full.len() - end_len;
				Ok(Val::Arr(full.slice(
					Some(start_len as i32),
					Some(to as i32),
					None,
				)))
			}),
		);
	}

	for (i, d) in end.iter().enumerate() {
		let full = full.clone();
		destruct(
			d,
			Thunk!(move || {
				let full = full.evaluate()?;
				Ok(full
					.get(full.len() - end_len + i as u32)?
					.expect("length is checked"))
			}),
			fctx.clone(),
			builder,
		);
	}
}

#[allow(dead_code, reason = "not dead in exp-destruct")]
fn destruct_object(
	fields: &[LDestructField],
	rest: Option<LDestructRest>,

	value: Thunk<Val>,
	fctx: Pending<Context>,
	builder: &mut ContextBuilder,
) {
	use jrsonnet_interner::IStr;
	use rustc_hash::FxHashSet;

	use crate::{bail, ObjValueBuilder};

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
			let len = obj.len();
			if len as usize > field_names.len() {
				bail!("too many fields, and rest not found");
			}
		}
		Ok(obj)
	});

	if let Some(crate::analyze::LDestructRest::Keep(id)) = rest {
		let full = full.clone();
		builder.bind(
			id,
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
		let default: Option<(Pending<Context>, Rc<LExpr>)> =
			field.default.as_ref().map(|e| (fctx.clone(), e.clone()));
		let field_full = full.clone();
		let value_thunk = Thunk!(move || {
			let obj = field_full.evaluate()?;
			obj.get(field_name)?.map_or_else(
				|| {
					let (fctx, expr) = default.as_ref().expect("shape is checked");
					evaluate(fctx.unwrap(), expr)
				},
				Ok,
			)
		});

		if let Some(into) = &field.into {
			destruct(into, value_thunk, fctx.clone(), builder);
		} else {
			unreachable!("analyzer lowers object-destruct shorthands into `into`");
		}
	}
}

/// Bind a pre-built thunk to an [`LDestruct`] pattern, inserting one
/// binding per [`LocalId`] the pattern introduces.
///
/// `fctx` is needed for object-destruct defaults (feature `exp-destruct`).
#[allow(unused_variables)]
pub fn destruct(
	d: &LDestruct,
	value: Thunk<Val>,
	fctx: Pending<Context>,
	builder: &mut ContextBuilder,
) {
	match d {
		LDestruct::Full(id) => builder.bind(*id, value),
		#[cfg(feature = "exp-destruct")]
		LDestruct::Skip => {}
		#[cfg(feature = "exp-destruct")]
		LDestruct::Array { start, rest, end } => destruct_array(start, rest, end, value, fctx, builder),
		#[cfg(feature = "exp-destruct")]
		LDestruct::Object { fields, rest } => destruct_object(fields, rest, value, fctx, builder),
	}
}

/// Bind one [`LBind`] as a lazy thunk that evaluates in the given
/// future context. Mirrors the old `evaluate_dest` — one entry per
/// binding in a `local … ;` frame.
pub fn evaluate_dest(bind: &LBind, fctx: Pending<Context>, builder: &mut ContextBuilder) {
	let value = bind.value.clone();
	let fctx_clone = fctx.clone();
	let thunk = Thunk!(move || {
		let ctx = fctx_clone.unwrap();
		evaluate(ctx, &value)
	});
	destruct(&bind.destruct, thunk, fctx, builder);
}

/// Bind each LBind's value as a lazy thunk. Mutually recursive locals
/// resolve lazily through the shared Pending<Context>.
pub fn evaluate_locals(parent: Context, binds: &[LBind]) -> Context {
	if binds.is_empty() {
		return parent;
	}
	let fctx = Context::new_future();
	let mut builder =
		ContextBuilder::extend(parent, binds.iter().map(|b| b.destruct.ids().len()).sum());
	for bind in binds {
		evaluate_dest(bind, fctx.clone(), &mut builder);
	}
	builder.build().into_future(fctx)
}

pub trait CloneableUnbound<T>: Unbound<Bound = T> + Clone {}
impl<V, T> CloneableUnbound<T> for V where V: Unbound<Bound = T> + Clone {}

pub fn evaluate_locals_unbound(
	fctx: Context,
	locals: Rc<Vec<LBind>>,
	this_id: Option<LocalId>,
) -> impl CloneableUnbound<Context> {
	#[derive(Trace, Clone)]
	struct UnboundLocals {
		fctx: Context,
		locals: Rc<Vec<LBind>>,
		this_id: Option<LocalId>,
	}
	impl Unbound for UnboundLocals {
		type Bound = Context;

		fn bind(&self, sup_this: SupThis) -> Result<Context> {
			let parent = self.fctx.clone();

			let fctx = Context::new_future();
			let mut builder = ContextBuilder::extend(
				parent,
				self.locals.iter().map(|b| b.destruct.ids().len()).sum(),
			);
			for b in self.locals.iter() {
				evaluate_dest(b, fctx.clone(), &mut builder);
			}
			if let Some(this_id) = self.this_id {
				builder.bind(this_id, Thunk::evaluated(Val::Obj(sup_this.this().clone())));
			}
			let ctx = builder.build_sup_this(sup_this).into_future(fctx);
			Ok(ctx)
		}
	}

	UnboundLocals {
		fctx,
		locals,
		this_id,
	}
}
