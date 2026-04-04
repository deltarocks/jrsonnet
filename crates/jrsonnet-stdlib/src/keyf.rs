use jrsonnet_evaluator::{
	Error, Result, Thunk, Val,
	function::{FuncVal, NativeFn},
	typed::{ComplexValType, FromUntyped, Typed, ValType},
};

type PreparedKeyF = NativeFn!((Thunk<Val>) -> Val);

#[derive(Default, Clone)]
pub enum KeyF {
	#[default]
	Identity,
	Prepared(PreparedKeyF),
	PrepareFailure(Error),
}
impl KeyF {
	pub fn is_identity(&self) -> bool {
		matches!(self, Self::Identity)
	}
	fn new(val: FuncVal) -> Self {
		if val.is_identity() {
			Self::Identity
		} else {
			PreparedKeyF::try_from(val).map_or_else(Self::PrepareFailure, Self::Prepared)
		}
	}
	pub fn eval(&self, val: impl Into<Thunk<Val>>) -> Result<Val> {
		match self {
			KeyF::Identity => val.into().evaluate(),
			KeyF::Prepared(p) => p.call(val.into()),
			KeyF::PrepareFailure(e) => Err(e.clone()),
		}
	}
}

impl Typed for KeyF {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Func);
}
impl FromUntyped for KeyF {
	fn from_untyped(untyped: Val) -> Result<Self> {
		FuncVal::from_untyped(untyped).map(Self::new)
	}
}
