use std::rc::Rc;

use jrsonnet_gcmodule::{Acyclic, Trace};
use jrsonnet_ir::{ExprParams, IStr, function::FunctionSignature};
use rustc_hash::FxHashSet;

use super::{CallLocation, FuncVal};
use crate::{
	Context, ContextBuilder, Pending, Result, Thunk, Val, bail, destructure::destruct,
	error::ErrorKind::*, evaluate_named_param,
};

#[derive(Debug, Trace, Clone)]
pub struct PreparedFuncVal {
	fun: FuncVal,
	prepared: Rc<PreparedCall>,
}

impl PreparedFuncVal {
	pub fn new(fun: FuncVal, unnamed: usize, named: &[IStr]) -> Result<Self> {
		let prepared = prepare_call(fun.params(), unnamed, named)?;
		Ok(Self {
			fun,
			prepared: Rc::new(prepared),
		})
	}
	pub fn call(
		&self,
		loc: CallLocation<'_>,
		unnamed: &[Thunk<Val>],
		named: &[Thunk<Val>],
	) -> Result<Val> {
		self.fun
			.evaluate_prepared(&self.prepared, loc, unnamed, named, false)
	}
}

#[derive(Acyclic, Debug)]
pub struct PreparedCall {
	// Param, named input.
	named: Vec<(usize, usize)>,
	defaults: Vec<usize>,
}

pub fn prepare_call(
	params: FunctionSignature,
	unnamed: usize,
	named: &[IStr],
) -> Result<PreparedCall> {
	if unnamed > params.len() {
		bail!(TooManyArgsFunctionHas(params.len(), params))
	}

	let expected_defaults = (params.len() - unnamed).saturating_sub(named.len());
	let mut ops = PreparedCall {
		named: Vec::with_capacity(named.len()),
		defaults: Vec::with_capacity(expected_defaults),
	};

	// FIXME: bitmask
	let mut passed: FxHashSet<usize> = (0..unnamed).collect();

	for (input_id, name) in named.iter().enumerate() {
		// FIXME: O(n) for arg existence check
		let Some(param_idx) = params.iter().position(|p| p.name() == name) else {
			bail!(UnknownFunctionParameter(name.clone()));
		};
		if !passed.insert(param_idx) {
			bail!(BindingParameterASecondTime(name.clone()));
		}
		ops.named.push((param_idx, input_id));
	}

	if named.len() + unnamed < params.len() {
		let mut defaults = 0;

		for (param_id, param) in params
			.iter()
			.enumerate()
			.skip(unnamed)
			.filter(|p| p.1.has_default())
		{
			// Skip already passed parameters
			if !param.name().is_anonymous() && passed.contains(&param_id) {
				continue;
			}
			defaults += 1;

			ops.defaults.push(param_id);
		}

		// Some args still weren't filled
		if defaults != expected_defaults {
			for param in params.iter().skip(unnamed) {
				let mut found = false;
				for name in named {
					if param.name() == name {
						found = true;
					}
				}
				if !found {
					bail!(FunctionParameterNotBoundInCall(
						param.name().clone(),
						params
					));
				}
			}
			unreachable!();
		}
	}

	Ok(ops)
}
pub fn parse_prepared_function_call(
	body_ctx: Context,
	prepared: &PreparedCall,
	params: &ExprParams,
	unnamed: &[Thunk<Val>],
	named: &[Thunk<Val>],
) -> Result<Context> {
	let mut ctx = ContextBuilder::extend(body_ctx);

	let destruct_ctx = Pending::new();

	for (param_idx, unnamed) in unnamed.iter().enumerate() {
		destruct(
			&params.exprs[param_idx].destruct,
			unnamed.clone(),
			destruct_ctx.clone(),
			&mut ctx,
		)?;
	}

	for (param_idx, arg_idx) in prepared.named.iter().copied() {
		destruct(
			&params.exprs[param_idx].destruct,
			named[arg_idx].clone(),
			destruct_ctx.clone(),
			&mut ctx,
		)?;
	}

	if prepared.defaults.is_empty() {
		let body_ctx = ctx.build().into_future(destruct_ctx);
		Ok(body_ctx)
	} else {
		let fctx = Context::new_future();
		let mut ctx = ctx.commit();
		for param_idx in prepared.defaults.iter().copied() {
			// let param = params.0.rc_idx(param_idx);
			destruct(
				&params.exprs[param_idx].destruct,
				{
					let ctx = fctx.clone();
					let params = params.clone();
					Thunk!(move || {
						let param = &params.exprs[param_idx];
						let name = param.destruct.name();
						let value = param.default.as_ref().expect("default exists");
						evaluate_named_param(ctx.unwrap(), value, name)
					})
				},
				fctx.clone(),
				&mut ctx,
			)?;
		}

		Ok(ctx.build().into_future(fctx).into_future(destruct_ctx))
	}
}
pub fn parse_prepared_builtin_call(
	prepared: &PreparedCall,
	params: FunctionSignature,
	unnamed: &[Thunk<Val>],
	named: &[Thunk<Val>],
) -> Vec<Option<Thunk<Val>>> {
	let mut passed_args = vec![None; params.len()];

	for (param_idx, unnamed) in unnamed.iter().enumerate() {
		passed_args[param_idx] = Some(unnamed.clone());
	}

	for (param_idx, arg_idx) in prepared.named.iter().copied() {
		passed_args[param_idx] = Some(named[arg_idx].clone());
	}

	passed_args
}
