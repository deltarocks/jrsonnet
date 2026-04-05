use std::any::Any;

use jrsonnet_gcmodule::{Trace, TraceBox, cc_dyn};
use jrsonnet_ir::function::{FunctionSignature, ParamDefault, ParamName, ParamParse};

use super::CallLocation;
use crate::{Result, Thunk, Val};

#[macro_export]
macro_rules! params {
	(@name unnamed) => { ParamName::Unnamed };
	(@name named $name:literal) => { ParamName::Named($crate::IStr::from($name)) };
	($($(#[$meta:meta])* [$kind:ident $(($lit:literal))? => $default:expr]),* $(,)?) => {
		thread_local! {
			static PARAMS: FunctionSignature = FunctionSignature::new([
				$($(#[$meta])* ParamParse::new(params!(@name $kind $($lit)?), $default)),*
			].into());
		}
	};
}

#[macro_export]
macro_rules! names {
	($($name:ident: $val:literal),* $(,)?) => {
		struct Names {
			$($name: $crate::IStr,)*
		}
		thread_local! {
			static NAMES: Names = Names {
				$($name: $crate::IStr::from($val)),*
			};
		}
		$(pub fn $name() -> $crate::IStr {
			NAMES.with(|n| n.$name.clone())
		})*
	}
}

cc_dyn!(
	#[derive(Clone)]
	BuiltinFunc,
	Builtin,
	pub(crate) fn new() {...}
);
impl Builtin for BuiltinFunc {
	fn name(&self) -> &str {
		self.0.name()
	}

	fn params(&self) -> FunctionSignature {
		self.0.params()
	}

	fn call(&self, loc: CallLocation<'_>, args: &[Option<Thunk<Val>>]) -> Result<Val> {
		self.0.call(loc, args)
	}

	fn as_any(&self) -> &dyn Any {
		self.0.as_any()
	}
}

/// Description of function defined by native code
///
/// Prefer to use #[builtin] macro, instead of manual implementation of this trait
pub trait Builtin: Trace {
	/// Function name to be used in stack traces
	fn name(&self) -> &str;
	/// Parameter names for named calls
	fn params(&self) -> FunctionSignature;
	/// Call the builtin
	fn call(&self, loc: CallLocation<'_>, args: &[Option<Thunk<Val>>]) -> Result<Val>;

	fn as_any(&self) -> &dyn Any;
}

#[derive(Trace)]
pub struct NativeCallback {
	pub(crate) params: FunctionSignature,
	handler: TraceBox<dyn NativeCallbackHandler>,
}
impl NativeCallback {
	#[deprecated = "prefer using builtins directly, use this interface only for bindings"]
	pub fn new(params: Vec<String>, handler: impl NativeCallbackHandler) -> Self {
		Self {
			params: FunctionSignature::new(
				params
					.into_iter()
					.map(|n| ParamParse::new(ParamName::Named(n.into()), ParamDefault::None))
					.collect(),
			),
			handler: TraceBox(Box::new(handler)),
		}
	}
}

impl Builtin for NativeCallback {
	fn name(&self) -> &'static str {
		// TODO: standard natives gets their names from definition
		// But builitins should already have them
		"<native>"
	}

	fn params(&self) -> FunctionSignature {
		self.params.clone()
	}

	fn call(&self, _loc: CallLocation<'_>, args: &[Option<Thunk<Val>>]) -> Result<Val> {
		let args = args
			.iter()
			.map(|a| a.as_ref().expect("legacy natives have no default params"))
			.map(Thunk::evaluate)
			.collect::<Result<Vec<Val>>>()?;
		self.handler.call(&args)
	}

	fn as_any(&self) -> &dyn Any {
		self
	}
}

pub trait NativeCallbackHandler: Trace {
	fn call(&self, args: &[Val]) -> Result<Val>;
}
