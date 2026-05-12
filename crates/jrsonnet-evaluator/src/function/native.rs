use std::marker::PhantomData;

use jrsonnet_gcmodule::Trace;
use jrsonnet_types::{ComplexValType, ValType};

use super::PreparedFuncVal;
use crate::{
	CallLocation, Result, Val,
	function::FuncVal,
	typed::{FromUntyped, IntoUntyped, Typed},
};

#[derive(Debug, Trace, Clone)]
pub struct NativeFn<D: 'static>(pub(crate) PreparedFuncVal, PhantomData<D>);

impl<D: 'static> NativeFn<D> {
	pub fn func(&self) -> &FuncVal {
		self.0.func()
	}
}
macro_rules! impl_native_desc {
	($i:expr; $($gen:ident)*) => {
		impl<$($gen,)* O> NativeFn<($($gen,)* O,)>
		where
			$($gen: Typed + IntoUntyped,)*
			O: Typed + FromUntyped,
		{
			#[allow(non_snake_case, clippy::too_many_arguments)]
			pub fn call(
				&self,
				$($gen: $gen,)*
			) -> Result<O> {
				let val = self.0.call(
					CallLocation::native(),
					&[$(IntoUntyped::into_lazy_untyped($gen),)*],
					&[],
				)?;
				O::from_untyped(val)
			}
		}
		impl<$($gen,)* O> Typed for NativeFn<($($gen,)* O,)> {
			const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Func);
		}

		impl<$($gen,)* O> FromUntyped for NativeFn<($($gen,)* O,)> {
			fn from_untyped(untyped: Val) -> Result<Self> {
				let func = FuncVal::from_untyped(untyped)?;
				Self::try_from(func)
			}
		}
		impl<$($gen,)* O> TryFrom<FuncVal> for NativeFn<($($gen,)* O,)> {
			type Error = crate::Error;
			fn try_from(v: FuncVal) -> Result<Self> {
				Ok(Self(
					PreparedFuncVal::new(v, $i, &[])?,
					PhantomData,
				))
			}
		}

	};
	($i:expr; $($cur:ident)* @ $c:ident $($rest:ident)*) => {
		impl_native_desc!($i; $($cur)*);
		impl_native_desc!($i + 1; $($cur)* $c @ $($rest)*);
	};
	($i:expr; $($cur:ident)* @) => {
		impl_native_desc!($i; $($cur)*);
	}
}

impl_native_desc! {
	0; @ A B C D E F G H I J K L
}

mod native_macro {
	#[macro_export]
	macro_rules! NativeFn {
		(($($t:ty),* $(,)?) -> $res:ty) => {
			NativeFn<($($t,)* $res)>
		}
	}
}
pub use crate::NativeFn;
