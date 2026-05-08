use crate::nodes::{BinaryOperatorKind, UnaryOperatorKind};

impl BinaryOperatorKind {
	pub fn binding_power(&self) -> (u8, u8) {
		match self {
			Self::MetaObjectApply => (22, 23),
			Self::Mul | Self::Div | Self::Modulo => (20, 21),
			Self::Plus | Self::Minus => (18, 19),
			Self::Lhs | Self::Rhs => (16, 17),
			Self::Lt | Self::Gt | Self::Le | Self::Ge | Self::InKw => (14, 15),
			Self::Eq | Self::Ne => (12, 13),
			Self::BitAnd => (10, 11),
			Self::BitXor => (8, 9),
			Self::BitOr => (6, 7),
			Self::And => (4, 5),
			Self::NullCoaelse | Self::Or => (2, 3),
			Self::ErrorNoOperator => (0, 1),
		}
	}
}

impl UnaryOperatorKind {
	pub fn binding_power(&self) -> ((), u8) {
		match self {
			Self::Minus | Self::Not | Self::BitNot | Self::Plus => ((), 20),
		}
	}
}
