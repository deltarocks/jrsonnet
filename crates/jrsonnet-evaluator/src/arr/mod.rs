use std::{
	any::Any,
	fmt::{self},
	num::NonZeroU32,
	rc::Rc,
};

use jrsonnet_gcmodule::{cc_dyn, Cc};

use crate::{analyze::LExpr, function::NativeFn, typed::IntoUntyped, Context, Result, Thunk, Val};

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

	#[must_use]
	pub fn slice(self, index: Option<i32>, end: Option<i32>, step: Option<NonZeroU32>) -> Self {
		let get_idx = |pos: Option<i32>, len: u32, default| match pos {
			#[expect(
				clippy::cast_sign_loss,
				reason = "abs value is used, len is limited to u31"
			)]
			Some(v) if v < 0 => len.saturating_add_signed(v),
			#[expect(clippy::cast_sign_loss, reason = "abs value is used")]
			Some(v) => (v as u32).min(len),
			None => default,
		};
		let index = get_idx(index, self.len(), 0);
		let end = get_idx(end, self.len(), self.len());
		let step = step.unwrap_or_else(|| NonZeroU32::new(1).expect("1 != 0"));

		if index >= end {
			return Self::empty();
		}

		Self::new(SliceArray {
			inner: self,
			#[expect(clippy::cast_possible_truncation, reason = "len is limited to u31")]
			from: index as u32,
			#[expect(clippy::cast_possible_truncation, reason = "len is limited to u31")]
			to: end as u32,
			step: step.get(),
		})
	}

	/// Array length.
	pub fn len(&self) -> u32 {
		self.0.len()
	}

	/// Is array contains no elements?
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	pub fn is_cheap(&self) -> bool {
		self.0.is_cheap()
	}

	/// Get array element by index, evaluating it, if it is lazy.
	///
	/// Returns `None` on out-of-bounds condition.
	pub fn get(&self, index: u32) -> Result<Option<Val>> {
		self.0.get(index)
	}

	/// Get array element by index, without evaluation.
	///
	/// Returns `None` on out-of-bounds condition.
	pub fn get_lazy(&self, index: u32) -> Option<Thunk<Val>> {
		self.0.get_lazy(index)
	}

	pub fn iter(&self) -> impl ArrayLikeIter<Result<Val>> + '_ {
		(0..self.len()).map(|i| self.get(i).transpose().expect("length checked"))
	}

	/// Iterate over elements, returning lazy values.
	pub fn iter_lazy(&self) -> impl ArrayLikeIter<Thunk<Val>> + '_ {
		(0..self.len()).map(|i| self.get_lazy(i).expect("length checked"))
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
