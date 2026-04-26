use std::rc::Rc;

use jrsonnet_types::ValType;

use super::{
	destructure::{self, evaluate_locals, evaluate_locals_unbound},
	evaluate_field_member_static, evaluate_field_member_unbound,
};
use crate::{
	Context, ContextBuilder, ObjValue, ObjValueBuilder, Pending, Result, Thunk, Val,
	analyze::{LArrComp, LBind, LCompSpec, LDestruct, LExpr, LFieldMember, LObjComp, LocalId},
	arr::ArrValue,
	bail,
	error::ErrorKind::*,
	evaluate::evaluate,
};

trait CompCollector {
	fn reserve(&mut self, _guaranteed: usize) {}
	fn collect(&mut self, ctx: Context) -> Result<()>;
}

struct EagerArrCollector<'a> {
	out: &'a mut Vec<Val>,
	value: &'a LExpr,
}
impl CompCollector for EagerArrCollector<'_> {
	fn reserve(&mut self, size_hint: usize) {
		self.out.reserve(size_hint);
	}
	fn collect(&mut self, ctx: Context) -> Result<()> {
		self.out.push(evaluate(ctx, self.value)?);
		Ok(())
	}
}

struct LazyArrCollector<'a> {
	out: &'a mut Vec<Thunk<Val>>,
	value: &'a Rc<LExpr>,
}
impl CompCollector for LazyArrCollector<'_> {
	fn reserve(&mut self, size_hint: usize) {
		self.out.reserve(size_hint);
	}
	fn collect(&mut self, ctx: Context) -> Result<()> {
		let value_expr = self.value.clone();
		self.out.push(Thunk!(move || evaluate(ctx, &value_expr)));
		Ok(())
	}
}

struct ObjCompCollectorStatic<'a> {
	builder: &'a mut ObjValueBuilder,
	locals: &'a [LBind],
	field: &'a LFieldMember,
}
impl CompCollector for ObjCompCollectorStatic<'_> {
	fn reserve(&mut self, guaranteed: usize) {
		self.builder.reserve_fields(guaranteed);
	}
	fn collect(&mut self, inner_ctx: Context) -> Result<()> {
		let value_ctx = evaluate_locals(inner_ctx.clone(), self.locals);
		evaluate_field_member_static(self.builder, inner_ctx, value_ctx, self.field)
	}
}

struct ObjCompCollectorUnbound<'a> {
	builder: &'a mut ObjValueBuilder,
	locals: Rc<Vec<LBind>>,
	this_id: Option<LocalId>,
	field: &'a LFieldMember,
}
impl CompCollector for ObjCompCollectorUnbound<'_> {
	fn reserve(&mut self, guaranteed: usize) {
		self.builder.reserve_fields(guaranteed);
	}
	fn collect(&mut self, inner_ctx: Context) -> Result<()> {
		let uctx = evaluate_locals_unbound(inner_ctx.clone(), self.locals.clone(), self.this_id);
		evaluate_field_member_unbound(self.builder, inner_ctx, uctx, self.field)
	}
}

pub fn evaluate_obj_comp(
	super_obj: Option<ObjValue>,
	ctx: Context,
	comp: &LObjComp,
) -> Result<Val> {
	let mut builder = ObjValueBuilder::new();
	if let Some(super_obj) = super_obj {
		builder.with_super(super_obj);
	}

	let cached_overs = cache_overs(&ctx, &comp.compspecs)?;
	if comp.this.is_some() || comp.uses_super {
		evaluate_compspecs(
			ctx,
			&comp.compspecs,
			&cached_overs,
			0,
			0,
			&mut ObjCompCollectorUnbound {
				builder: &mut builder,
				locals: comp.locals.clone(),
				this_id: comp.this,
				field: &comp.field,
			},
		)?;
	} else {
		evaluate_compspecs(
			ctx,
			&comp.compspecs,
			&cached_overs,
			0,
			0,
			&mut ObjCompCollectorStatic {
				builder: &mut builder,
				locals: &comp.locals,
				field: &comp.field,
			},
		)?;
	}

	Ok(Val::Obj(builder.build()))
}

pub fn evaluate_arr_comp(ctx: Context, comp: &LArrComp) -> Result<Val> {
	let cached_overs = cache_overs(&ctx, &comp.compspecs)?;

	// In eager evaluation, Context is not captured, thus updates in CoW fashion will likely to success
	'eager: {
		let mut out = Vec::new();

		if comp.compspecs.iter().all(|c| {
			matches!(
				c,
				LCompSpec::If(_)
					| LCompSpec::For {
						destruct: LDestruct::Full(_),
						..
					}
			)
		}) && evaluate_compspecs_eager(
			ctx.clone(),
			&comp.compspecs,
			&cached_overs,
			0,
			0,
			&mut EagerArrCollector {
				out: &mut out,
				value: &comp.value,
			},
		)
		.is_err()
		{
			break 'eager;
		}
		return Ok(Val::arr(out));
	}

	let mut items: Vec<Thunk<Val>> = Vec::new();
	evaluate_compspecs(
		ctx,
		&comp.compspecs,
		&cached_overs,
		0,
		0,
		&mut LazyArrCollector {
			out: &mut items,
			value: &comp.value,
		},
	)?;
	Ok(Val::arr(items))
}

fn cache_overs(ctx: &Context, specs: &[LCompSpec]) -> Result<Vec<Option<ArrValue>>> {
	specs
		.iter()
		.map(|spec| {
			Ok(match spec {
				LCompSpec::For {
					over,
					loop_invariant: true,
					..
				} => {
					let val = evaluate(ctx.clone(), over)?;
					let Val::Arr(arr) = val else {
						bail!(InComprehensionCanOnlyIterateOverArray)
					};
					Some(arr)
				}
				_ => None,
			})
		})
		.collect::<Result<_>>()
}

fn evaluate_compspecs_eager(
	ctx: Context,
	specs: &[LCompSpec],
	cached_overs: &[Option<ArrValue>],
	idx: usize,
	guaranteed_reserve: usize,
	collector: &mut dyn CompCollector,
) -> Result<()> {
	if idx >= specs.len() {
		collector.reserve(guaranteed_reserve);
		return collector.collect(ctx);
	}
	match &specs[idx] {
		LCompSpec::If(cond) => {
			let val = evaluate(ctx.clone(), cond)?;
			let Val::Bool(b) = val else {
				bail!(TypeMismatch(
					"if spec condition",
					vec![ValType::Bool],
					val.value_type()
				))
			};
			if b {
				evaluate_compspecs_eager(ctx, specs, cached_overs, idx + 1, 0, collector)?;
			}
		}
		LCompSpec::For { destruct, over, .. } => {
			let arr = if let Some(cached) = &cached_overs[idx] {
				cached.clone()
			} else {
				let arr_val = evaluate(ctx.clone(), over)?;
				let Val::Arr(arr) = arr_val else {
					bail!(InComprehensionCanOnlyIterateOverArray)
				};
				arr
			};
			let inner_reserve = guaranteed_reserve.max(1) * arr.len() as usize;
			match destruct {
				LDestruct::Full(id) => {
					let id = *id;
					let mut inner_ctx = ContextBuilder::extend(ctx, 1).build();
					for (i, item) in arr.iter().enumerate() {
						// TODO: reuse one ContextBuilder for full evaluate_compspecs pipeline
						inner_ctx.cow_fill_binding(id, Thunk::evaluated(item?));
						evaluate_compspecs_eager(
							inner_ctx.clone(),
							specs,
							cached_overs,
							idx + 1,
							if i == 0 { inner_reserve } else { 0 },
							collector,
						)?;
					}
				}
				// TODO: Should not be eager? CoW won't work here
				#[cfg(feature = "exp-destruct")]
				_ => unreachable!("eager compspecs are not possible with non-full patterns"),
			}
		}
	}
	Ok(())
}

fn evaluate_compspecs(
	ctx: Context,
	specs: &[LCompSpec],
	cached_overs: &[Option<ArrValue>],
	idx: usize,
	guaranteed_reserve: usize,
	collector: &mut dyn CompCollector,
) -> Result<()> {
	if idx >= specs.len() {
		collector.reserve(guaranteed_reserve);
		return collector.collect(ctx);
	}
	match &specs[idx] {
		LCompSpec::If(cond) => {
			let val = evaluate(ctx.clone(), cond)?;
			let Val::Bool(b) = val else {
				bail!(TypeMismatch(
					"if spec condition",
					vec![ValType::Bool],
					val.value_type()
				))
			};
			if b {
				evaluate_compspecs(ctx, specs, cached_overs, idx + 1, 0, collector)?;
			}
		}
		LCompSpec::For { destruct, over, .. } => {
			let arr = if let Some(cached) = &cached_overs[idx] {
				cached.clone()
			} else {
				let arr_val = evaluate(ctx.clone(), over)?;
				let Val::Arr(arr) = arr_val else {
					bail!(InComprehensionCanOnlyIterateOverArray)
				};
				arr
			};
			let inner_reserve = guaranteed_reserve.max(1) * arr.len() as usize;
			for (i, item) in arr.iter().enumerate() {
				let item_val = item?;
				let mut inner_builder = ContextBuilder::extend(ctx.clone(), 1);
				let fctx = Pending::new();
				destructure::destruct(
					destruct,
					Thunk::evaluated(item_val),
					fctx.clone(),
					&mut inner_builder,
				);
				let inner_ctx = inner_builder.build().into_future(fctx);
				evaluate_compspecs(
					inner_ctx,
					specs,
					cached_overs,
					idx + 1,
					if i == 0 { inner_reserve } else { 0 },
					collector,
				)?;
			}
		}
	}
	Ok(())
}
