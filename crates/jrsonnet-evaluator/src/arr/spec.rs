use std::{
	any::Any,
	cell::RefCell,
	fmt::{self, Debug},
	mem::replace,
	rc::Rc,
};

use jrsonnet_gcmodule::{Cc, Trace};
use jrsonnet_interner::{IBytes, IStr};
use jrsonnet_ir::Expr;

use super::ArrValue;
use crate::{
	Context, Error, ObjValue, Result, Thunk, Val,
	error::ErrorKind::InfiniteRecursionDetected,
	evaluate,
	function::NativeFn,
	typed::{IntoUntyped, Typed},
	val::ThunkValue,
};

pub trait ArrayLike: Any + Trace + Debug {
	fn len(&self) -> usize;
	fn is_empty(&self) -> bool {
		self.len() == 0
	}
	fn get(&self, index: usize) -> Result<Option<Val>>;
	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>>;

	fn is_cheap(&self) -> bool {
		false
	}
}
trait ArrayCheap {
	fn get(&self, index: usize) -> Option<Val>;
	fn len(&self) -> usize;
}
impl<T> ArrayLike for T
where
	T: Any + Trace + Debug + ArrayCheap,
{
	fn len(&self) -> usize {
		<T as ArrayCheap>::len(self)
	}

	fn get(&self, index: usize) -> Result<Option<Val>> {
		Ok(<T as ArrayCheap>::get(self, index))
	}

	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		<T as ArrayCheap>::get(self, index).map(Thunk::evaluated)
	}

	fn is_cheap(&self) -> bool {
		true
	}
}

impl ArrayCheap for () {
	fn len(&self) -> usize {
		0
	}
	fn get(&self, _index: usize) -> Option<Val> {
		None
	}
}

#[derive(Debug, Trace)]
pub struct SliceArray {
	pub(crate) inner: ArrValue,
	pub(crate) from: u32,
	pub(crate) to: u32,
	pub(crate) step: u32,
}

impl SliceArray {
	fn map_idx(&self, index: usize) -> usize {
		self.from as usize + self.step as usize * index
	}
}
impl ArrayLike for SliceArray {
	fn len(&self) -> usize {
		(self.to - self.from).div_ceil(self.step) as usize
	}

	fn get(&self, index: usize) -> Result<Option<Val>> {
		self.inner.get(self.map_idx(index))
	}

	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		self.inner.get_lazy(self.map_idx(index))
	}

	fn is_cheap(&self) -> bool {
		self.inner.is_cheap()
	}
}

impl ArrayCheap for IBytes {
	fn len(&self) -> usize {
		self.as_slice().len()
	}
	fn get(&self, index: usize) -> Option<Val> {
		self.as_slice().get(index).map(|v| Val::Num((*v).into()))
	}
}

#[derive(Debug, Trace, Clone)]
enum ArrayThunk {
	Computed(Val),
	Errored(Error),
	Waiting,
	Pending,
}

#[derive(Debug, Trace, Clone)]
pub struct ExprArray {
	ctx: Context,
	src: Rc<Vec<Expr>>,
	cached: Cc<RefCell<Vec<ArrayThunk>>>,
}
impl ExprArray {
	pub fn new(ctx: Context, src: Rc<Vec<Expr>>) -> Self {
		Self {
			ctx,
			cached: Cc::new(RefCell::new(vec![ArrayThunk::Waiting; src.len()])),
			src,
		}
	}
}
impl ArrayLike for ExprArray {
	fn len(&self) -> usize {
		self.cached.borrow().len()
	}
	fn get(&self, index: usize) -> Result<Option<Val>> {
		if index >= self.len() {
			return Ok(None);
		}
		match &self.cached.borrow()[index] {
			ArrayThunk::Computed(c) => return Ok(Some(c.clone())),
			ArrayThunk::Errored(e) => return Err(e.clone()),
			ArrayThunk::Pending => return Err(InfiniteRecursionDetected.into()),
			ArrayThunk::Waiting => {}
		}

		let ArrayThunk::Waiting =
			replace(&mut self.cached.borrow_mut()[index], ArrayThunk::Pending)
		else {
			unreachable!()
		};

		let new_value = match evaluate(self.ctx.clone(), &self.src[index]) {
			Ok(v) => v,
			Err(e) => {
				self.cached.borrow_mut()[index] = ArrayThunk::Errored(e.clone());
				return Err(e);
			}
		};
		self.cached.borrow_mut()[index] = ArrayThunk::Computed(new_value.clone());
		Ok(Some(new_value))
	}
	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		#[derive(Trace)]
		struct ExprArrThunk {
			expr: ExprArray,
			index: usize,
		}
		impl ThunkValue for ExprArrThunk {
			type Output = Val;

			fn get(&self) -> Result<Self::Output> {
				self.expr
					.get(self.index)
					.transpose()
					.expect("index checked")
			}
		}

		if index >= self.len() {
			return None;
		}
		match &self.cached.borrow()[index] {
			ArrayThunk::Computed(c) => return Some(Thunk::evaluated(c.clone())),
			ArrayThunk::Errored(e) => return Some(Thunk::errored(e.clone())),
			ArrayThunk::Waiting | ArrayThunk::Pending => {}
		}

		Some(Thunk::new(ExprArrThunk {
			expr: self.clone(),
			index,
		}))
	}
	fn is_cheap(&self) -> bool {
		false
	}
}

#[derive(Trace, Debug)]
pub struct ExtendedArray {
	pub a: ArrValue,
	pub b: ArrValue,
	split: usize,
	len: usize,
}
impl ExtendedArray {
	pub fn new(a: ArrValue, b: ArrValue) -> Self {
		let a_len = a.len();
		let b_len = b.len();
		Self {
			a,
			b,
			split: a_len,
			len: a_len.checked_add(b_len).expect("too large array value"),
		}
	}
}

struct WithExactSize<I>(I, usize);
impl<I, T> Iterator for WithExactSize<I>
where
	I: Iterator<Item = T>,
{
	type Item = T;

	fn next(&mut self) -> Option<Self::Item> {
		self.0.next()
	}
	fn nth(&mut self, n: usize) -> Option<Self::Item> {
		self.0.nth(n)
	}
	fn size_hint(&self) -> (usize, Option<usize>) {
		(self.1, Some(self.1))
	}
}
impl<I> DoubleEndedIterator for WithExactSize<I>
where
	I: DoubleEndedIterator,
{
	fn next_back(&mut self) -> Option<Self::Item> {
		self.0.next_back()
	}
	fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
		self.0.nth_back(n)
	}
}
impl<I> ExactSizeIterator for WithExactSize<I>
where
	I: Iterator,
{
	fn len(&self) -> usize {
		self.1
	}
}
impl ArrayLike for ExtendedArray {
	fn get(&self, index: usize) -> Result<Option<Val>> {
		if self.split > index {
			self.a.get(index)
		} else {
			self.b.get(index - self.split)
		}
	}
	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		if self.split > index {
			self.a.get_lazy(index)
		} else {
			self.b.get_lazy(index - self.split)
		}
	}

	fn len(&self) -> usize {
		self.len
	}

	fn is_cheap(&self) -> bool {
		self.a.is_cheap() && self.b.is_cheap()
	}
}

impl<T> ArrayLike for Vec<T>
where
	T: IntoUntyped + Trace + fmt::Debug,
	for<'a> &'a T: IntoUntyped,
{
	fn len(&self) -> usize {
		self.as_slice().len()
	}

	fn get(&self, index: usize) -> Result<Option<Val>> {
		let Some(elem) = self.as_slice().get(index) else {
			return Ok(None);
		};
		IntoUntyped::into_untyped(elem).map(Some)
	}

	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		let elem = self.as_slice().get(index)?;
		Some(IntoUntyped::into_lazy_untyped(elem))
	}

	fn is_cheap(&self) -> bool {
		!T::provides_lazy()
	}
}

/// Inclusive range type
#[derive(Debug, Trace, PartialEq, Eq)]
pub struct RangeArray {
	start: i32,
	end: i32,
}
impl RangeArray {
	pub fn empty() -> Self {
		Self::new_exclusive(0, 0)
	}
	pub fn new_exclusive(start: i32, end: i32) -> Self {
		end.checked_sub(1)
			.map_or_else(Self::empty, |end| Self { start, end })
	}
	pub fn new_inclusive(start: i32, end: i32) -> Self {
		Self { start, end }
	}
	#[expect(
		clippy::cast_sign_loss,
		reason = "the math is valid with wrapping, sign loss works as intended"
	)]
	fn size(&self) -> usize {
		(self.end as usize)
			.wrapping_sub(self.start as usize)
			.wrapping_add(1)
	}
	fn range(&self) -> impl ExactSizeIterator<Item = i32> + DoubleEndedIterator {
		WithExactSize(self.start..=self.end, self.size())
	}
}
impl ArrayCheap for RangeArray {
	fn get(&self, index: usize) -> Option<Val> {
		self.range().nth(index).map(|i| Val::Num(i.into()))
	}
	fn len(&self) -> usize {
		self.size()
	}
}

#[derive(Debug, Trace)]
pub struct ReverseArray(pub ArrValue);
impl ArrayLike for ReverseArray {
	fn len(&self) -> usize {
		self.0.len()
	}

	fn get(&self, index: usize) -> Result<Option<Val>> {
		self.0.get(self.0.len() - index - 1)
	}

	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		self.0.get_lazy(self.0.len() - index - 1)
	}

	fn is_cheap(&self) -> bool {
		self.0.is_cheap()
	}
}

#[derive(Trace, Clone, Debug)]
pub enum ArrayMapper {
	Plain(NativeFn!((Val) -> Val)),
	WithIndex(NativeFn!((u32, Val) -> Val)),
}

#[derive(Trace, Debug, Clone)]
pub struct MappedArray {
	inner: ArrValue,
	cached: Cc<RefCell<Vec<ArrayThunk>>>,
	mapper: ArrayMapper,
}
impl MappedArray {
	pub fn new(inner: ArrValue, mapper: ArrayMapper) -> Self {
		let len = inner.len();
		Self {
			inner,
			cached: Cc::new(RefCell::new(vec![ArrayThunk::Waiting; len])),
			mapper,
		}
	}
	fn evaluate(&self, index: usize, value: Val) -> Result<Val> {
		match &self.mapper {
			ArrayMapper::Plain(f) => f.call(value),
			#[expect(
				clippy::cast_possible_truncation,
				reason = "array len is limited to u31"
			)]
			ArrayMapper::WithIndex(f) => f.call(index as u32, value),
		}
	}
}
impl ArrayLike for MappedArray {
	fn len(&self) -> usize {
		self.cached.borrow().len()
	}

	fn get(&self, index: usize) -> Result<Option<Val>> {
		if index >= self.len() {
			return Ok(None);
		}
		match &self.cached.borrow()[index] {
			ArrayThunk::Computed(c) => return Ok(Some(c.clone())),
			ArrayThunk::Errored(e) => return Err(e.clone()),
			ArrayThunk::Pending => return Err(InfiniteRecursionDetected.into()),
			ArrayThunk::Waiting => {}
		}

		let ArrayThunk::Waiting =
			replace(&mut self.cached.borrow_mut()[index], ArrayThunk::Pending)
		else {
			unreachable!()
		};

		let val = self
			.inner
			.get(index)
			.transpose()
			.expect("index checked")
			.and_then(|r| self.evaluate(index, r));

		let new_value = match val {
			Ok(v) => v,
			Err(e) => {
				self.cached.borrow_mut()[index] = ArrayThunk::Errored(e.clone());
				return Err(e);
			}
		};
		self.cached.borrow_mut()[index] = ArrayThunk::Computed(new_value.clone());
		Ok(Some(new_value))
	}
	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		#[derive(Trace)]
		struct MappedArrayThunk {
			arr: MappedArray,
			index: usize,
		}
		impl ThunkValue for MappedArrayThunk {
			type Output = Val;

			fn get(&self) -> Result<Self::Output> {
				self.arr.get(self.index).transpose().expect("index checked")
			}
		}

		if index >= self.len() {
			return None;
		}
		match &self.cached.borrow()[index] {
			ArrayThunk::Computed(c) => return Some(Thunk::evaluated(c.clone())),
			ArrayThunk::Errored(e) => return Some(Thunk::errored(e.clone())),
			ArrayThunk::Waiting | ArrayThunk::Pending => {}
		}

		Some(Thunk::new(MappedArrayThunk {
			arr: self.clone(),
			index,
		}))
	}
}

#[derive(Trace, Debug)]
pub struct RepeatedArray {
	data: ArrValue,
	repeats: usize,
	total_len: usize,
}
impl RepeatedArray {
	pub fn new(data: ArrValue, repeats: usize) -> Option<Self> {
		let total_len = data.len().checked_mul(repeats)?;
		Some(Self {
			data,
			repeats,
			total_len,
		})
	}
	fn map_idx(&self, index: usize) -> Option<usize> {
		if index > self.total_len {
			return None;
		}
		Some(index % self.data.len())
	}
}

impl ArrayLike for RepeatedArray {
	fn len(&self) -> usize {
		self.total_len
	}

	fn get(&self, index: usize) -> Result<Option<Val>> {
		let Some(idx) = self.map_idx(index) else {
			return Ok(None);
		};
		self.data.get(idx)
	}

	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		let idx = self.map_idx(index)?;
		self.data.get_lazy(idx)
	}

	fn is_cheap(&self) -> bool {
		self.data.is_cheap()
	}
}

#[derive(Trace, Debug)]
pub struct PickObjectValues {
	obj: ObjValue,
	keys: Vec<IStr>,
}

impl PickObjectValues {
	pub fn new(obj: ObjValue, keys: Vec<IStr>) -> Self {
		Self { obj, keys }
	}
}

impl ArrayLike for PickObjectValues {
	fn len(&self) -> usize {
		self.keys.len()
	}

	fn get(&self, index: usize) -> Result<Option<Val>> {
		let Some(key) = self.keys.as_slice().get(index) else {
			return Ok(None);
		};
		Ok(Some(self.obj.get_or_bail(key.clone())?))
	}

	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		let key = self.keys.as_slice().get(index)?;
		Some(self.obj.get_lazy_or_bail(key.clone()))
	}

	fn is_cheap(&self) -> bool {
		false
	}
}

#[derive(Trace, Debug)]
pub struct PickObjectKeyValues {
	obj: ObjValue,
	keys: Vec<IStr>,
}

impl PickObjectKeyValues {
	pub fn new(obj: ObjValue, keys: Vec<IStr>) -> Self {
		Self { obj, keys }
	}
}

#[derive(Typed, IntoUntyped)]
pub struct KeyValue {
	key: IStr,
	value: Thunk<Val>,
}

impl ArrayLike for PickObjectKeyValues {
	fn len(&self) -> usize {
		self.keys.len()
	}

	fn get(&self, index: usize) -> Result<Option<Val>> {
		let Some(key) = self.keys.as_slice().get(index) else {
			return Ok(None);
		};
		Ok(Some(
			KeyValue::into_untyped(KeyValue {
				key: key.clone(),
				value: Thunk::evaluated(self.obj.get_or_bail(key.clone())?),
			})
			.expect("convertible"),
		))
	}

	fn get_lazy(&self, index: usize) -> Option<Thunk<Val>> {
		let key = self.keys.as_slice().get(index)?;
		// Nothing can fail in the key part, yet value is still
		// lazy-evaluated
		Some(Thunk::evaluated(
			KeyValue::into_untyped(KeyValue {
				key: key.clone(),
				value: self.obj.get_lazy_or_bail(key.clone()),
			})
			.expect("convertible"),
		))
	}

	fn is_cheap(&self) -> bool {
		false
	}
}
