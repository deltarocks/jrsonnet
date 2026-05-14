#![allow(clippy::redundant_closure_call, clippy::derive_partial_eq_without_eq)]

mod expr;
use std::{cmp::Ordering, fmt, ops::Deref};

pub use expr::*;
use jrsonnet_gcmodule::Acyclic;
pub use jrsonnet_interner::IStr;
pub mod function;
mod location;
mod source;
pub mod visit;

pub use location::CodeLocation;
pub use source::{
	Source, SourceDefaultIgnoreJpath, SourceDirectory, SourceFifo, SourceFile, SourcePath,
	SourcePathT, SourceUrl, SourceVirtual,
};

// It seels to be a wrong place for this kind of stuff, but as it would also be used for static analysis and
// is already wanted for NumValue, I don't know a better place.
#[expect(clippy::cast_precision_loss, reason = "checked to not overflow")]
pub const MAX_SAFE_INTEGER: f64 = ((1u64 << (f64::MANTISSA_DIGITS)) - 1) as f64;
#[expect(clippy::cast_precision_loss, reason = "checked to not overflow")]
pub const MIN_SAFE_INTEGER: f64 = (-((1i64 << (f64::MANTISSA_DIGITS)) - 1)) as f64;

/// Represents jsonnet number
/// Jsonnet numbers are finite f64, with NaNs disallowed
#[derive(Acyclic, Clone, Copy)]
pub struct NumValue(f64);
impl NumValue {
	/// Creates a [`NumValue`], if value is finite and not NaN
	pub fn new(v: f64) -> Option<Self> {
		if !v.is_finite() {
			return None;
		}
		Some(Self(v))
	}
	#[inline]
	pub const fn get(&self) -> f64 {
		self.0
	}
	pub fn truncate_for_bitwise(self) -> Result<i64, ConvertNumValueError> {
		if self.0 < MIN_SAFE_INTEGER || self.0 > MAX_SAFE_INTEGER {
			return Err(ConvertNumValueError::BitwiseSafeRange);
		}
		#[expect(clippy::cast_possible_truncation, reason = "intended")]
		Ok(self.0 as i64)
	}
}
impl PartialEq for NumValue {
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}
impl Eq for NumValue {}
impl Ord for NumValue {
	#[inline]
	fn cmp(&self, other: &Self) -> Ordering {
		// Can't use `total_cmp`: its behavior for `-0` and `0`
		// is not following wanted.
		unsafe { self.0.partial_cmp(&other.0).unwrap_unchecked() }
	}
}
impl PartialOrd for NumValue {
	#[inline]
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}
impl fmt::Debug for NumValue {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(&self.0, f)
	}
}
impl fmt::Display for NumValue {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Display::fmt(&self.0, f)
	}
}
impl Deref for NumValue {
	type Target = f64;

	#[inline]
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
macro_rules! impl_num {
	($($ty:ty),+) => {$(
		impl From<$ty> for NumValue {
			#[inline]
			fn from(value: $ty) -> Self {
				Self(value.into())
			}
		}
	)+};
}
impl_num!(i8, u8, i16, u16, i32, u32);

#[derive(Clone, Copy, Debug, thiserror::Error, Acyclic)]
pub enum ConvertNumValueError {
	#[error("overflow")]
	Overflow,
	#[error("underflow")]
	Underflow,
	#[error("non-finite")]
	NonFinite,
	#[error("float out of safe int range")]
	BitwiseSafeRange,
}

macro_rules! impl_try_num {
	($($ty:ty),+) => {$(
		impl TryFrom<$ty> for NumValue {
			type Error = ConvertNumValueError;
			#[inline]
			fn try_from(value: $ty) -> Result<Self, ConvertNumValueError> {
				#[expect(clippy::cast_precision_loss, reason = "precision loss is explicitly handled")]
				let value = value as f64;
				if value < MIN_SAFE_INTEGER {
					return Err(ConvertNumValueError::Underflow)
				} else if value > MAX_SAFE_INTEGER {
					return Err(ConvertNumValueError::Overflow)
				}
				// Number is finite.
				Ok(Self(value))
			}
		}
	)+};
}
impl_try_num!(usize, isize, i64, u64);

impl TryFrom<f64> for NumValue {
	type Error = ConvertNumValueError;

	#[inline]
	fn try_from(value: f64) -> Result<Self, Self::Error> {
		Self::new(value).ok_or(ConvertNumValueError::NonFinite)
	}
}
impl TryFrom<f32> for NumValue {
	type Error = ConvertNumValueError;

	#[inline]
	fn try_from(value: f32) -> Result<Self, Self::Error> {
		Self::new(f64::from(value)).ok_or(ConvertNumValueError::NonFinite)
	}
}
