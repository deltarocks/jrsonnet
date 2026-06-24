use std::rc::Rc;

#[cfg(feature = "exp-object-iteration")]
use jrsonnet_interner::IStr;
use jrsonnet_types::ValType;

use super::{
	destructure::{destruct, evaluate_locals_unbound, fill_letrec_binds},
	evaluate_field_member_static, evaluate_field_member_unbound,
};
use crate::{
	Context, ObjValue, ObjValueBuilder, Result, Thunk, Val,
	analyze::{
		ClosureShape, LArrComp, LBind, LCompSpec, LDestruct, LExpr, LFieldMember, LObjComp,
		LocalSlot,
	},
	arr::ArrValue,
	bail,
	error::ErrorKind::*,
	evaluate::{evaluate, evaluate_trivial},
};

enum CachedOver {
	Arr(ArrValue),
	#[cfg(feature = "exp-object-iteration")]
	Obj(ObjValue),
}

trait CompCollector {
	fn reserve(&mut self, _guaranteed: usize) {}
	fn collect(&mut self, ctx: Context) -> Result<()>;
}

struct EagerArrCollector<'a> {
	out: &'a mut Vec<Val>,
	value_shape: &'a ClosureShape,
	value: &'a LExpr,
}
impl CompCollector for EagerArrCollector<'_> {
	fn reserve(&mut self, size_hint: usize) {
		self.out.reserve(size_hint);
	}
	fn collect(&mut self, ctx: Context) -> Result<()> {
		if let Some(v) = evaluate_trivial(self.value) {
			self.out.push(v);
			return Ok(());
		}
		if let LExpr::Slot(slot) = self.value {
			self.out.push(ctx.slot(*slot).evaluate()?);
			return Ok(());
		}
		let env = Context::enter_using(&ctx, self.value_shape);
		self.out.push(evaluate(env, self.value)?);
		Ok(())
	}
}

struct LazyArrCollector<'a> {
	out: &'a mut Vec<Thunk<Val>>,
	value_shape: &'a ClosureShape,
	value: &'a Rc<LExpr>,
}
impl CompCollector for LazyArrCollector<'_> {
	fn reserve(&mut self, size_hint: usize) {
		self.out.reserve(size_hint);
	}
	fn collect(&mut self, ctx: Context) -> Result<()> {
		if let Some(v) = evaluate_trivial(self.value) {
			self.out.push(Thunk::evaluated(v));
			return Ok(());
		}
		if let LExpr::Slot(slot) = self.value.as_ref() {
			self.out.push(ctx.slot(*slot));
			return Ok(());
		}
		let env = Context::enter_using(&ctx, self.value_shape);
		let value_expr = self.value.clone();
		self.out.push(Thunk!(move || evaluate(env, &value_expr)));
		Ok(())
	}
}

struct ObjCompCollectorStatic<'a> {
	builder: &'a mut ObjValueBuilder,
	frame_shape: &'a ClosureShape,
	locals: &'a [LBind],
	field: &'a LFieldMember,
}
impl CompCollector for ObjCompCollectorStatic<'_> {
	fn reserve(&mut self, guaranteed: usize) {
		self.builder.reserve_fields(guaranteed);
	}
	fn collect(&mut self, inner_ctx: Context) -> Result<()> {
		// Build the object's A-frame fresh per iteration: captures from
		// the comp's iter ctx, locals = `this` (slot 0, unfilled in the
		// static path) + member-locals via letrec.
		let value_ctx = inner_ctx
			.pack_captures_sup_this(self.frame_shape)
			.enter(|fill, ctx| {
				fill_letrec_binds(fill, ctx, self.locals);
			});
		evaluate_field_member_static(self.builder, inner_ctx, value_ctx, self.field)
	}
}

struct ObjCompCollectorUnbound<'a> {
	builder: &'a mut ObjValueBuilder,
	frame_shape: Rc<ClosureShape>,
	locals: Rc<Vec<LBind>>,
	this_slot: Option<LocalSlot>,
	field: &'a LFieldMember,
}
impl CompCollector for ObjCompCollectorUnbound<'_> {
	fn reserve(&mut self, guaranteed: usize) {
		self.builder.reserve_fields(guaranteed);
	}
	fn collect(&mut self, inner_ctx: Context) -> Result<()> {
		let uctx = evaluate_locals_unbound(
			&inner_ctx,
			&self.frame_shape,
			self.this_slot,
			self.locals.clone(),
		);
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
				frame_shape: comp.frame_shape.clone(),
				locals: comp.locals.clone(),
				this_slot: comp.this,
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
				frame_shape: &comp.frame_shape,
				locals: &comp.locals,
				field: &comp.field,
			},
		)?;
	}

	Ok(Val::Obj(builder.build()))
}

pub fn evaluate_arr_comp(ctx: Context, comp: &LArrComp) -> Result<Val> {
	let cached_overs = cache_overs(&ctx, &comp.compspecs)?;

	// Eager fast-path: when the comp has only `if` and `for { destruct: Full(_) }`
	// specs, allocate one iter frame per for-spec and re-set the slot
	// per iteration as long as the frame's refcount stays at 1.
	'eager: {
		if !comp.compspecs.iter().all(|c| {
			matches!(
				c,
				LCompSpec::If(_)
					| LCompSpec::For {
						destruct: LDestruct::Full(_),
						..
					}
			)
		}) {
			break 'eager;
		}
		let mut out = Vec::new();
		if evaluate_compspecs_eager(
			ctx.clone(),
			&comp.compspecs,
			&cached_overs,
			0,
			0,
			&mut EagerArrCollector {
				out: &mut out,
				value_shape: &comp.value_shape,
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
			value_shape: &comp.value_shape,
			value: &comp.value,
		},
	)?;
	Ok(Val::arr(items))
}

fn cache_overs(ctx: &Context, specs: &[LCompSpec]) -> Result<Vec<Option<CachedOver>>> {
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
					Some(CachedOver::Arr(arr))
				}
				#[cfg(feature = "exp-object-iteration")]
				LCompSpec::ForObj {
					over,
					loop_invariant: true,
					..
				} => {
					let val = evaluate(ctx.clone(), over)?;
					let Val::Obj(obj) = val else {
						bail!(TypeMismatch(
							"object iteration over",
							vec![jrsonnet_types::ValType::Obj],
							val.value_type(),
						))
					};
					Some(CachedOver::Obj(obj))
				}
				_ => None,
			})
		})
		.collect::<Result<_>>()
}

fn evaluate_compspecs_eager(
	ctx: Context,
	specs: &[LCompSpec],
	cached_overs: &[Option<CachedOver>],
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
		LCompSpec::For {
			frame_shape,
			destruct,
			over,
			..
		} => {
			let arr = if let Some(CachedOver::Arr(cached)) = &cached_overs[idx] {
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
				LDestruct::Full(slot) => {
					Context::enter_iter(&ctx, frame_shape, |it| {
						for (i, item) in arr.iter().enumerate() {
							let item = item?;
							let ctx = it.create(|f| {
								f.set(*slot, Thunk::evaluated(item));
							})?;
							evaluate_compspecs_eager(
								ctx,
								specs,
								cached_overs,
								idx + 1,
								if i == 0 { inner_reserve } else { 0 },
								collector,
							)?;
						}
						Ok(())
					})?;
				}
				// TODO: Should not be eager? CoW won't work here
				#[cfg(feature = "exp-destruct")]
				_ => unreachable!("eager compspecs are not possible with non-full patterns"),
			}
		}
		#[cfg(feature = "exp-object-iteration")]
		LCompSpec::ForObj { .. } => {
			unreachable!("eager compspecs filter rejects ForObj");
		}
	}
	Ok(())
}

#[allow(clippy::too_many_lines)]
fn evaluate_compspecs(
	ctx: Context,
	specs: &[LCompSpec],
	cached_overs: &[Option<CachedOver>],
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
		LCompSpec::For {
			frame_shape,
			destruct: dst,
			over,
			..
		} => {
			let arr = if let Some(CachedOver::Arr(cached)) = &cached_overs[idx] {
				cached.clone()
			} else {
				let arr_val = evaluate(ctx.clone(), over)?;
				let Val::Arr(arr) = arr_val else {
					bail!(InComprehensionCanOnlyIterateOverArray)
				};
				arr
			};
			let inner_reserve = guaranteed_reserve.max(1) * arr.len() as usize;
			for (i, item) in arr.iter_lazy().enumerate() {
				let inner_ctx = ctx.pack_captures_sup_this(frame_shape).enter(|fill, ctx| {
					destruct(dst, fill, item, ctx);
				});
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
		#[cfg(feature = "exp-object-iteration")]
		LCompSpec::ForObj {
			frame_shape,
			key,
			visibility,
			value,
			over,
			..
		} => {
			use jrsonnet_ir::Visibility;
			let obj = if let Some(CachedOver::Obj(cached)) = &cached_overs[idx] {
				cached.clone()
			} else {
				let val = evaluate(ctx.clone(), over)?;
				let Val::Obj(obj) = val else {
					bail!(TypeMismatch(
						"object iteration over",
						vec![ValType::Obj],
						val.value_type(),
					))
				};
				obj
			};
			let fields = obj.fields_with_visibility(
				#[cfg(feature = "exp-preserve-order")]
				false,
			);
			let pairs: Vec<(IStr, Visibility)> = fields
				.into_iter()
				.filter(|(_, v)| match visibility {
					Visibility::Normal => v.is_visible(),
					Visibility::Hidden => !v.is_visible(),
					Visibility::Unhide => true,
				})
				.collect();
			let inner_reserve = guaranteed_reserve.max(1) * pairs.len();
			for (i, (field_name, _)) in pairs.into_iter().enumerate() {
				let key_val = Val::string(field_name.clone());
				let value_thunk = obj
					.get_lazy(field_name.clone())
					.expect("field exists, just enumerated");
				let inner_ctx = ctx.pack_captures_sup_this(frame_shape).enter(|fill, ctx| {
					fill.set(*key, Thunk::evaluated(key_val));
					destruct(value, fill, value_thunk, &ctx);
				});
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
