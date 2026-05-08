use std::{
	any::Any,
	fmt::{self},
	num::NonZeroU32,
	ops::{Bound, RangeBounds},
	rc::Rc,
};

use jrsonnet_gcmodule::{Cc, cc_dyn};

use crate::{Context, Result, Thunk, Val, analyze::LExpr, function::NativeFn, typed::IntoUntyped};

mod spec;
pub use spec::{ArrayLike, *};

cc_dyn!(
	#[doc = "Represents a Jsonnet array value."]
	#[derive(Clone)]
	ArrValue,
	ArrayLike,
	pub fn new() {...}
);
impl fmt::Debug for ArrValue {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.0.fmt(f)
	}
}

pub trait ArrayLikeIter<T>: Iterator<Item = T> + DoubleEndedIterator + ExactSizeIterator {}
impl<I, T> ArrayLikeIter<T> for I where
	I: Iterator<Item = T> + DoubleEndedIterator + ExactSizeIterator
{
}

impl ArrValue {
	pub fn empty() -> Self {
		Self::new(())
	}

	pub fn expr(ctx: Context, exprs: Rc<Vec<LExpr>>) -> Self {
		Self::new(ExprArray::new(ctx, exprs))
	}

	pub fn repeated(data: Self, repeats: u32) -> Option<Self> {
		Some(Self::new(RepeatedArray::new(data, repeats)?))
	}

	pub fn make(len: u32, cb: NativeFn!((u32,)->Val)) -> Self {
		Self::new(MakeArray::new(len, cb))
	}

	#[must_use]
	pub fn map(self, mapper: NativeFn!((Val) -> Val)) -> Self {
		Self::new(<MappedArray>::new(self, ArrayMapper::Plain(mapper)))
	}

	#[must_use]
	pub fn map_with_index(self, mapper: NativeFn!((u32, Val) -> Val)) -> Self {
		Self::new(<MappedArray>::new(self, ArrayMapper::WithIndex(mapper)))
	}

	pub fn filter(self, filter: NativeFn!((Thunk<Val>) -> bool)) -> Result<Self> {
		// TODO: ArrValue::Picked(inner, indexes) for large arrays
		'eager: {
			let mut out = Vec::new();
			for i in self.iter() {
				let Ok(i) = i else {
					break 'eager;
				};
				if filter.call(IntoUntyped::into_lazy_untyped(i.clone()))? {
					out.push(i);
				}
			}
			return Ok(Self::new(out));
		};

		let mut out = Vec::new();
		for i in self.iter_lazy() {
			if filter.call(i.clone())? {
				out.push(i);
			}
		}
		Ok(Self::new(out))
	}

	pub fn extended(a: Self, b: Self) -> Option<Self> {
		Some(if a.is_empty() {
			b
		} else if b.is_empty() {
			a
		} else {
			Self::new(ExtendedArray::new(a, b)?)
		})
	}

	pub fn range_exclusive(a: i32, b: i32) -> Self {
		Self::new(RangeArray::new_exclusive(a, b))
	}
	pub fn range_inclusive(a: i32, b: i32) -> Self {
		Self::new(RangeArray::new_inclusive(a, b))
	}

	#[inline]
	#[must_use]
	pub fn slice(self, range: impl RangeBounds<usize>) -> Self {
		fn map_bound(start: bool, bound: Bound<&usize>) -> Option<i32> {
			match bound {
				Bound::Included(&v) => Some(i32::try_from(v).unwrap_or(i32::MAX)),
				Bound::Excluded(&v) => Some(
					i32::try_from(v)
						.unwrap_or(i32::MAX)
						.saturating_add(if start { 1 } else { -1 }),
				),
				Bound::Unbounded => None,
			}
		}
		self.slice32(
			map_bound(true, range.start_bound()),
			map_bound(false, range.end_bound()),
			None,
		)
	}

	#[must_use]
	pub fn slice32(self, index: Option<i32>, end: Option<i32>, step: Option<NonZeroU32>) -> Self {
		let get_idx = |pos: Option<i32>, len: u32, default| match pos {
			Some(v) if v < 0 => len.saturating_add_signed(v),
			#[expect(clippy::cast_sign_loss, reason = "abs value is used")]
			Some(v) => (v as u32).min(len),
			None => default,
		};
		let index = get_idx(index, self.len32(), 0);
		let end = get_idx(end, self.len32(), self.len32());
		let step = step.unwrap_or_else(|| NonZeroU32::new(1).expect("1 != 0"));

		if index >= end {
			return Self::empty();
		}

		Self::new(SliceArray {
			inner: self,
			from: index,
			to: end,
			step: step.get(),
		})
	}

	/// Array length.
	#[inline]
	pub fn len32(&self) -> u32 {
		self.0.len32()
	}

	pub fn len(&self) -> usize {
		self.len32() as usize
	}

	/// Is array contains no elements?
	#[inline]
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	#[inline]
	pub fn is_cheap(&self) -> bool {
		self.0.is_cheap()
	}

	/// Get array element by index, evaluating it, if it is lazy.
	///
	/// Returns `None` on out-of-bounds condition.
	#[inline]
	pub fn get32(&self, index: u32) -> Result<Option<Val>> {
		self.0.get32(index)
	}

	pub fn get(&self, index: usize) -> Result<Option<Val>> {
		let Ok(i) = u32::try_from(index) else {
			return Ok(None);
		};
		self.get32(i)
	}

	/// Get array element by index, without evaluation.
	///
	/// Returns `None` on out-of-bounds condition.
	#[inline]
	pub fn get_lazy32(&self, index: u32) -> Option<Thunk<Val>> {
		self.0.get_lazy32(index)
	}

	pub fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		u32::try_from(index).ok().and_then(|i| self.get_lazy32(i))
	}

	pub fn iter(&self) -> impl ArrayLikeIter<Result<Val>> + '_ {
		(0..self.len32()).map(|i| self.get32(i).transpose().expect("length checked"))
	}

	/// Iterate over elements, returning lazy values.
	pub fn iter_lazy(&self) -> impl ArrayLikeIter<Thunk<Val>> + '_ {
		(0..self.len32()).map(|i| self.get_lazy32(i).expect("length checked"))
	}

	/// Return a reversed view on current array.
	#[must_use]
	pub fn reversed(self) -> Self {
		Self::new(ReverseArray(self))
	}

	pub fn ptr_eq(a: &Self, b: &Self) -> bool {
		Cc::ptr_eq(&a.0, &b.0)
	}

	pub fn as_any(&self) -> &dyn Any {
		&self.0
	}
}
impl<T> From<T> for ArrValue
where
	T: ArrayLike,
{
	fn from(value: T) -> Self {
		Self::new(value)
	}
}
impl<I> FromIterator<I> for ArrValue
where
	Vec<I>: ArrayLike,
{
	fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
		Self::new(iter.into_iter().collect::<Vec<_>>())
	}
}

/// Checks that the usize does not exceed 4g with debug assertions enabled
/// Should only be used on values that can't reasonably exceed this value
#[inline]
pub(crate) fn arridx(i: usize) -> u32 {
	#[allow(
		clippy::cast_possible_truncation,
		reason = "array indexes never exceed 4g"
	)]
	if cfg!(debug_assertions) {
		u32::try_from(i).expect("4g hard limit")
	} else {
		i as u32
	}
}
