use std::{fmt::Debug, rc::Rc};

use educe::Educe;
use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::IStr;
use jrsonnet_ir::{Destruct, Expr, ExprParams, Span};
pub use jrsonnet_macros::builtin;

use self::{
	builtin::Builtin,
	parse::parse_default_function_call,
	prepared::{PreparedCall, parse_prepared_builtin_call, parse_prepared_function_call},
};
use crate::{
	Context, Result, Thunk, Val, evaluate, evaluate_trivial, function::builtin::BuiltinFunc,
};

pub mod builtin;
mod native;
mod parse;
mod prepared;

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
	/// Context, in which this function was evaluated.
	///
	/// # Example
	/// In
	/// ```jsonnet
	/// local a = 2;
	/// function() ...
	/// ```
	/// context will contain `a`.
	pub ctx: Context,

	/// Function parameter definition
	pub params: ExprParams,
	/// Function body
	pub body: Rc<Expr>,
}
impl FuncDesc {
	/// Create body context, but fill arguments without defaults with lazy error
	pub fn default_body_context(&self) -> Result<Context> {
		parse_default_function_call(self.ctx.clone(), &self.params)
	}

	pub fn evaluate_trivial(&self) -> Option<Val> {
		evaluate_trivial(&self.body)
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

	pub fn params(&self) -> FunctionSignature {
		match self {
			Self::Builtin(i) => i.params(),
			Self::Normal(p) => p.params.signature.clone(),
		}
	}
	/// Amount of non-default required arguments
	pub fn params_len(&self) -> usize {
		self.params().iter().filter(|p| !p.has_default()).count()
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
			FuncVal::Normal(func) => {
				let body_ctx = parse_prepared_function_call(
					func.ctx.clone(),
					prepared,
					&func.params,
					unnamed,
					named,
				)?;
				evaluate(body_ctx, &func.body)
			}
			FuncVal::Builtin(b) => {
				let args = parse_prepared_builtin_call(prepared, b.params(), unnamed, named);
				b.call(loc, &args)
			}
		}
	}

	/// Is this function an indentity function.
	///
	/// Currently only works for builtin `std.id`, aka `Self::Id` value, and `function(x) x`.
	///
	/// This function should only be used for optimization, not for the conditional logic, i.e code should work with syntetic identity function too
	pub fn is_identity(&self) -> bool {
		match self {
			Self::Builtin(b) => b.as_any().downcast_ref::<builtin_id>().is_some(),
			Self::Normal(desc) => {
				if desc.params.len() != 1 {
					return false;
				}
				let param = &desc.params.exprs[0];
				if param.default.is_some() {
					return false;
				}

				#[allow(clippy::infallible_destructuring_match)]
				let id = match &param.destruct {
					Destruct::Full(id) => id,
					#[cfg(feature = "exp-destruct")]
					_ => return false,
				};
				matches!(&*desc.body, Expr::Var(v) if &**v == id)
			}
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
