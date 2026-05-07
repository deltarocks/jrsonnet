use std::{fmt::Debug, rc::Rc};

use educe::Educe;
use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::IStr;
use jrsonnet_ir::Span;
pub use jrsonnet_macros::builtin;

use self::{
	builtin::Builtin,
	prepared::{PreparedCall, parse_prepared_builtin_call},
};
use crate::{
	Context, PackedContextSupThis, Result, Thunk, Val,
	analyze::LFunction,
	arr::arridx,
	evaluate::{destructure::destruct, ensure_sufficient_stack, evaluate, evaluate_trivial},
	function::builtin::BuiltinFunc,
};

pub mod builtin;
mod native;
pub(crate) mod prepared;

pub use jrsonnet_ir::function::*;
pub use native::NativeFn;
pub(crate) use prepared::PreparedFuncVal;

/// Function callsite location.
/// Either from other jsonnet code, specified by expression location, or from native (without location).
#[derive(Clone, Copy)]
pub struct CallLocation<'l>(pub Option<&'l Span>);
impl<'l> CallLocation<'l> {
	/// Construct new location for calls coming from specified jsonnet expression location.
	pub const fn new(loc: &'l Span) -> Self {
		Self(Some(loc))
	}
}
impl CallLocation<'static> {
	/// Construct new location for calls coming from native code.
	pub const fn native() -> Self {
		Self(None)
	}
}

/// Represents Jsonnet function defined in code.
#[derive(Trace, Educe)]
#[educe(Debug, PartialEq)]
pub struct FuncDesc {
	/// # Example
	///
	/// In expressions like this, deducted to `a`, unspecified otherwise.
	/// ```jsonnet
	/// local a = function() ...
	/// local a() ...
	/// { a: function() ... }
	/// { a() = ... }
	/// ```
	pub name: IStr,
	pub(crate) body_captures: PackedContextSupThis,

	#[educe(PartialEq(method = Rc::ptr_eq))]
	pub func: Rc<LFunction>,
}

impl FuncDesc {
	pub fn signature(&self) -> FunctionSignature {
		self.func.signature.clone()
	}

	pub fn call(
		&self,
		unnamed: &[Thunk<Val>],
		named: &[Thunk<Val>],
		prepared: &PreparedCall,
	) -> Result<Val> {
		let body_ctx = self.body_captures.clone().enter(|fill, ctx| {
			// Place each provided arg-thunk into its destructured slots.
			for (param_idx, thunk) in unnamed.iter().enumerate() {
				destruct(
					&self.func.params[param_idx].destruct,
					fill,
					thunk.clone(),
					ctx,
				);
			}
			for &(param_idx, arg_idx) in prepared.named() {
				destruct(
					&self.func.params[param_idx].destruct,
					fill,
					named[arg_idx].clone(),
					ctx,
				);
			}

			for &param_idx in prepared.defaults() {
				let param = &self.func.params[param_idx];
				let (shape, expr) = param.default.as_ref().expect("default exists");
				let expr = expr.clone();
				let env = Context::enter_using(ctx, shape);

				destruct(
					&param.destruct,
					fill,
					Thunk!(move || evaluate(env, &expr)),
					ctx,
				);
			}
		});

		ensure_sufficient_stack(|| evaluate(body_ctx, &self.func.body))
	}

	pub fn evaluate_trivial(&self) -> Option<Val> {
		evaluate_trivial(&self.func.body)
	}
}

/// Represents a Jsonnet function value, including plain functions and user-provided builtins.
#[allow(clippy::module_name_repetitions)]
#[derive(Trace, Clone)]
pub enum FuncVal {
	/// Plain function implemented in jsonnet.
	Normal(Cc<FuncDesc>),
	/// User-provided function.
	Builtin(BuiltinFunc),
}

impl Debug for FuncVal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Normal(arg0) => f.debug_tuple("Normal").field(arg0).finish(),
			Self::Builtin(arg0) => f.debug_tuple("Builtin").field(&arg0.name()).finish(),
		}
	}
}

#[allow(clippy::unnecessary_wraps)]
#[builtin]
pub const fn builtin_id(x: Thunk<Val>) -> Thunk<Val> {
	x
}

impl FuncVal {
	pub fn builtin(builtin: impl Builtin) -> Self {
		Self::Builtin(BuiltinFunc::new(builtin))
	}

	pub fn identity() -> Self {
		Self::builtin(builtin_id {})
	}

	pub fn params(&self) -> FunctionSignature {
		match self {
			Self::Builtin(i) => i.params(),
			Self::Normal(p) => p.signature(),
		}
	}
	/// Amount of non-default required arguments
	pub fn params_len32(&self) -> u32 {
		arridx(self.params().iter().filter(|p| !p.has_default()).count())
	}
	/// Function name, as defined in code.
	pub fn name(&self) -> IStr {
		match self {
			Self::Normal(normal) => normal.name.clone(),
			Self::Builtin(builtin) => builtin.name().into(),
		}
	}

	pub(crate) fn evaluate_prepared(
		&self,
		prepared: &PreparedCall,
		loc: CallLocation<'_>,
		unnamed: &[Thunk<Val>],
		named: &[Thunk<Val>],
		_tailstrict: bool,
	) -> Result<Val> {
		match self {
			FuncVal::Normal(func) => func.call(unnamed, named, prepared),
			FuncVal::Builtin(b) => {
				let args = parse_prepared_builtin_call(prepared, b.params(), unnamed, named);
				b.call(loc, &args)
			}
		}
	}

	/// Is this function an identity function.
	///
	/// This function should only be used for optimization, not for the conditional logic, i.e code should work with syntetic identity function too
	pub fn is_identity(&self) -> bool {
		match self {
			Self::Builtin(b) => b.as_any().downcast_ref::<builtin_id>().is_some(),
			Self::Normal(_) => false,
		}
	}

	pub fn evaluate_trivial(&self) -> Option<Val> {
		match self {
			Self::Normal(n) => n.evaluate_trivial(),
			Self::Builtin(_) => None,
		}
	}
}

impl<T> From<T> for FuncVal
where
	T: Builtin,
{
	fn from(value: T) -> Self {
		Self::builtin(value)
	}
}
