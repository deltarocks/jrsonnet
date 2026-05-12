use std::rc::Rc;

use jrsonnet_gcmodule::{Acyclic, Trace};
use jrsonnet_ir::{IStr, function::FunctionSignature};
use rustc_hash::FxHashSet;

use super::{CallLocation, FuncVal};
use crate::{Result, Thunk, Val, bail, error::ErrorKind::*};

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
	pub fn func(&self) -> &FuncVal {
		&self.fun
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

impl PreparedCall {
	pub fn named(&self) -> &[(usize, usize)] {
		&self.named
	}
	pub fn defaults(&self) -> &[usize] {
		&self.defaults
	}
	pub const fn empty() -> Self {
		Self {
			named: Vec::new(),
			defaults: Vec::new(),
		}
	}
}

pub fn prepare_call(
	params: FunctionSignature,
	unnamed: usize,
	named: &[IStr],
) -> Result<PreparedCall> {
	if unnamed > params.len() {
		bail!(TooManyArgsFunctionHas(params.len(), params))
	}

	// Fast path: positional-only (no named args). Avoids HashMap entirely.
	if named.is_empty() {
		let mut defaults = Vec::new();
		for (param_id, param) in params.iter().enumerate().skip(unnamed) {
			if param.has_default() {
				defaults.push(param_id);
			} else {
				bail!(FunctionParameterNotBoundInCall(
					param.name().clone(),
					params.clone(),
				))
			}
		}
		return Ok(PreparedCall {
			named: Vec::new(),
			defaults,
		});
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
