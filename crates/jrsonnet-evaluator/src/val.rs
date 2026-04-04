use std::{
	cell::RefCell,
	cmp::Ordering,
	fmt::{self, Debug, Display},
	marker::PhantomData,
	mem::replace,
	num::NonZeroU32,
	ops::Deref,
	rc::Rc,
};

use jrsonnet_gcmodule::{Acyclic, Cc, Trace, cc_dyn};
use jrsonnet_interner::IStr;
pub use jrsonnet_macros::Thunk;
use jrsonnet_types::ValType;
use rustc_hash::FxHashMap;
use thiserror::Error;

pub use crate::arr::{ArrValue, ArrayLike};
use crate::{
	ObjValue, Result, SupThis, Unbound, WeakSupThis, bail,
	error::{Error, ErrorKind::*},
	function::FuncVal,
	gc::WithCapacityExt as _,
	manifest::{ManifestFormat, ToStringFormat},
	typed::{BoundedUsize, MAX_SAFE_INTEGER, MIN_SAFE_INTEGER},
};

pub trait ThunkValue: Trace {
	type Output;
	fn get(&self) -> Result<Self::Output>;
}

#[derive(Trace)]
enum MemoizedClusureThunkInner<D: Trace, T: Trace> {
	Computed(T),
	Errored(Error),
	Waiting {
		env: D,
		// Carries no data, as it is not a real closure, all the
		// captured environment is stored in `env` field.
		#[trace(skip)]
		closure: fn(D) -> Result<T>,
	},
	Pending,
}
#[derive(Trace)]
pub struct MemoizedClosureThunk<D: Trace, T: Trace>(RefCell<MemoizedClusureThunkInner<D, T>>);
impl<D: Trace, T: Trace> MemoizedClosureThunk<D, T> {
	pub fn new(env: D, closure: fn(D) -> Result<T>) -> Self {
		Self(RefCell::new(MemoizedClusureThunkInner::Waiting {
			env,
			closure,
		}))
	}
}

impl<D: Trace, T: Trace + Clone> ThunkValue for MemoizedClosureThunk<D, T> {
	type Output = T;

	fn get(&self) -> Result<Self::Output> {
		match &*self.0.borrow() {
			MemoizedClusureThunkInner::Computed(v) => return Ok(v.clone()),
			MemoizedClusureThunkInner::Errored(e) => return Err(e.clone()),
			MemoizedClusureThunkInner::Pending => return Err(InfiniteRecursionDetected.into()),
			MemoizedClusureThunkInner::Waiting { .. } => (),
		}
		let MemoizedClusureThunkInner::Waiting { env, closure } = replace(
			&mut *self.0.borrow_mut(),
			MemoizedClusureThunkInner::Pending,
		) else {
			unreachable!();
		};
		let new_value = match closure(env) {
			Ok(v) => v,
			Err(e) => {
				*self.0.borrow_mut() = MemoizedClusureThunkInner::Errored(e.clone());
				return Err(e);
			}
		};
		*self.0.borrow_mut() = MemoizedClusureThunkInner::Computed(new_value.clone());
		Ok(new_value)
	}
}

cc_dyn!(
	/// Lazily evaluated value
	#[derive(Clone)] Thunk<V: Trace>,
	ThunkValue<Output = V>,
	pub fn new() {...}
);

impl<T: Trace> Thunk<T> {
	pub fn evaluated(val: T) -> Self
	where
		T: Clone,
	{
		#[derive(Trace)]
		struct EvaluatedThunk<T: Trace>(T);
		impl<T> ThunkValue for EvaluatedThunk<T>
		where
			T: Clone + Trace,
		{
			type Output = T;

			fn get(&self) -> Result<Self::Output> {
				Ok(self.0.clone())
			}
		}
		Self::new(EvaluatedThunk(val))
	}
	pub fn errored(e: Error) -> Self {
		#[derive(Trace)]
		struct ErroredThunk<T: Trace>(Error, PhantomData<T>);
		impl<T> ThunkValue for ErroredThunk<T>
		where
			T: Trace,
		{
			type Output = T;

			fn get(&self) -> Result<Self::Output> {
				Err(self.0.clone())
			}
		}
		Self::new(ErroredThunk(e, PhantomData))
	}
	pub fn result(res: Result<T, Error>) -> Self
	where
		T: Clone,
	{
		match res {
			Ok(o) => Self::evaluated(o),
			Err(e) => Self::errored(e),
		}
	}
}

impl<T> Thunk<T>
where
	T: Trace,
{
	pub fn force(&self) -> Result<()> {
		self.evaluate()?;
		Ok(())
	}

	/// Evaluate thunk, or return cached value
	///
	/// # Errors
	///
	/// - Lazy value evaluation returned error
	/// - This method was called during inner value evaluation
	pub fn evaluate(&self) -> Result<T> {
		self.0.get()
	}
}

pub trait ThunkMapper<Input>: Trace {
	type Output;
	fn map(self, from: Input) -> Result<Self::Output>;
}
impl<Input> Thunk<Input>
where
	Input: Trace,
{
	pub fn map<M>(self, mapper: M) -> Thunk<M::Output>
	where
		M: ThunkMapper<Input>,
		M::Output: Trace + Clone,
	{
		let inner = self;
		Thunk!(move || {
			let value = inner.evaluate()?;
			let mapped = mapper.map(value)?;
			Ok(mapped)
		})
	}
}

impl<T: Trace + Clone> From<Result<T>> for Thunk<T> {
	fn from(value: Result<T>) -> Self {
		match value {
			Ok(o) => Self::evaluated(o),
			Err(e) => Self::errored(e),
		}
	}
}
impl<T, V: Trace> From<T> for Thunk<V>
where
	T: ThunkValue<Output = V>,
{
	fn from(value: T) -> Self {
		Self::new(value)
	}
}

impl<T: Trace + Default + Clone> Default for Thunk<T> {
	fn default() -> Self {
		Self::evaluated(T::default())
	}
}

#[derive(Trace, Clone)]
pub struct CachedUnbound<I, T>
where
	I: Unbound<Bound = T>,
	T: Trace,
{
	cache: Cc<RefCell<FxHashMap<WeakSupThis, T>>>,
	value: I,
}
impl<I: Unbound<Bound = T>, T: Trace> CachedUnbound<I, T> {
	pub fn new(value: I) -> Self {
		Self {
			cache: Cc::new(RefCell::new(FxHashMap::new())),
			value,
		}
	}
}
impl<I: Unbound<Bound = T>, T: Clone + Trace> Unbound for CachedUnbound<I, T> {
	type Bound = T;
	fn bind(&self, sup_this: SupThis) -> Result<T> {
		let cache_key = sup_this.clone().downgrade();
		{
			if let Some(t) = self.cache.borrow().get(&cache_key) {
				return Ok(t.clone());
			}
		}
		let bound = self.value.bind(sup_this)?;

		{
			let mut cache = self.cache.borrow_mut();
			cache.insert(cache_key, bound.clone());
		}

		Ok(bound)
	}
}

impl<T: Debug + Trace> Debug for Thunk<T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Lazy")
	}
}
impl<T: Trace> PartialEq for Thunk<T> {
	fn eq(&self, other: &Self) -> bool {
		Cc::ptr_eq(&self.0, &other.0)
	}
}

/// Represents a Jsonnet value, which can be sliced or indexed (string or array).
#[allow(clippy::module_name_repetitions)]
pub enum IndexableVal {
	/// String.
	Str(IStr),
	/// Array.
	Arr(ArrValue),
}
impl IndexableVal {
	pub fn is_empty(&self) -> bool {
		match self {
			Self::Str(s) => s.is_empty(),
			Self::Arr(s) => s.is_empty(),
		}
	}

	pub fn to_array(self) -> ArrValue {
		match self {
			Self::Str(s) => s.chars().collect(),
			Self::Arr(arr) => arr,
		}
	}
	/// Slice the value.
	///
	/// # Implementation
	///
	/// For strings, will create a copy of specified interval.
	///
	/// For arrays, nothing will be copied on this call, instead [`ArrValue::Slice`] view will be returned.
	pub fn slice(
		self,
		index: Option<i32>,
		end: Option<i32>,
		step: Option<BoundedUsize<1, { i32::MAX as usize }>>,
	) -> Result<Self> {
		match &self {
			Self::Str(s) => {
				let mut computed_len = None;
				let mut get_len = || {
					computed_len.unwrap_or_else(|| {
						let len = s.chars().count();
						let _ = computed_len.insert(len);
						len
					})
				};
				let mut get_idx = |pos: Option<i32>, default| {
					match pos {
						#[expect(clippy::cast_sign_loss, reason = "abs value is used")]
						Some(v) if v < 0 => get_len().saturating_sub((-v as isize) as usize),
						// No need to clamp, as iterator interface is used
						#[expect(clippy::cast_sign_loss, reason = "abs value is used")]
						Some(v) => v as usize,
						None => default,
					}
				};

				let index = get_idx(index, 0);
				let end = get_idx(end, usize::MAX);
				let step = step.as_deref().copied().unwrap_or(1);

				if index >= end {
					return Ok(Self::Str("".into()));
				}

				Ok(Self::Str(
					(s.chars()
						.skip(index)
						.take(end - index)
						.step_by(step)
						.collect::<String>())
					.into(),
				))
			}
			Self::Arr(arr) => Ok(Self::Arr(arr.clone().slice(
				index,
				end,
				#[expect(
					clippy::cast_possible_truncation,
					reason = "overflow will result with skip too large which would be equivalent"
				)]
				step.map(|v| NonZeroU32::new(v.value() as u32).expect("bounded != 0")),
			))),
		}
	}
}

#[derive(Debug, Clone, Acyclic)]
pub enum StrValue {
	Flat(IStr),
	Tree(Rc<(StrValue, StrValue, usize)>),
}
impl StrValue {
	pub fn concat(a: Self, b: Self) -> Self {
		// TODO: benchmark for an optimal value, currently just a arbitrary choice
		const STRING_EXTEND_THRESHOLD: usize = 100;

		if a.is_empty() {
			b
		} else if b.is_empty() {
			a
		} else if a.len() + b.len() < STRING_EXTEND_THRESHOLD {
			Self::Flat(format!("{a}{b}").into())
		} else {
			let len = a.len() + b.len();
			Self::Tree(Rc::new((a, b, len)))
		}
	}
	pub fn into_flat(&self) -> IStr {
		#[cold]
		fn write_buf(s: &StrValue, out: &mut String) {
			match s {
				StrValue::Flat(f) => out.push_str(f),
				StrValue::Tree(t) => {
					write_buf(&t.0, out);
					write_buf(&t.1, out);
				}
			}
		}
		match self {
			Self::Flat(f) => f.clone(),
			Self::Tree(_) => {
				let mut buf = String::with_capacity(self.len());
				write_buf(self, &mut buf);
				buf.into()
			}
		}
	}
	pub fn len(&self) -> usize {
		match self {
			Self::Flat(v) => v.len(),
			Self::Tree(t) => t.2,
		}
	}
	pub fn is_empty(&self) -> bool {
		match self {
			Self::Flat(v) => v.is_empty(),
			// Can't create non-flat empty string
			Self::Tree(_) => false,
		}
	}
}
impl<T> From<T> for StrValue
where
	IStr: From<T>,
{
	fn from(value: T) -> Self {
		Self::Flat(IStr::from(value))
	}
}
impl Display for StrValue {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Flat(v) => write!(f, "{v}"),
			Self::Tree(t) => {
				write!(f, "{}", t.0)?;
				write!(f, "{}", t.1)
			}
		}
	}
}
impl PartialEq for StrValue {
	// False positive, into_flat returns not StrValue, but IStr, thus no infinite recursion here.
	#[allow(clippy::unconditional_recursion)]
	fn eq(&self, other: &Self) -> bool {
		let a = self.clone().into_flat();
		let b = other.clone().into_flat();
		a == b
	}
}
impl Eq for StrValue {}
impl PartialOrd for StrValue {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}
impl Ord for StrValue {
	fn cmp(&self, other: &Self) -> Ordering {
		let a = self.clone().into_flat();
		let b = other.clone().into_flat();
		a.cmp(&b)
	}
}

/// Represents jsonnet number
/// Jsonnet numbers are finite f64, with NaNs disallowed
#[derive(Trace, Clone, Copy)]
#[repr(transparent)]
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
	pub(crate) fn truncate_for_bitwise(self) -> Result<i64> {
		if self.0 < MIN_SAFE_INTEGER || self.0 > MAX_SAFE_INTEGER {
			bail!("numberic value outside of safe integer range for bitwise operation");
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
impl Debug for NumValue {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		Debug::fmt(&self.0, f)
	}
}
impl Display for NumValue {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		Display::fmt(&self.0, f)
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

#[derive(Clone, Copy, Debug, Error, Trace)]
pub enum ConvertNumValueError {
	#[error("overflow")]
	Overflow,
	#[error("underflow")]
	Underflow,
	#[error("non-finite")]
	NonFinite,
}
impl From<ConvertNumValueError> for Error {
	fn from(e: ConvertNumValueError) -> Self {
		Self::new(e.into())
	}
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

/// Represents any valid Jsonnet value.
#[derive(Debug, Clone, Trace, Default)]
pub enum Val {
	/// Represents a Jsonnet boolean.
	Bool(bool),
	/// Represents a Jsonnet null value.
	#[default]
	Null,
	/// Represents a Jsonnet string.
	Str(StrValue),
	/// Represents a Jsonnet number.
	/// Should be finite, and not NaN
	/// This restriction isn't enforced by enum, as enum field can't be marked as private
	Num(NumValue),
	/// Experimental bigint
	#[cfg(feature = "exp-bigint")]
	BigInt(#[trace(skip)] Box<num_bigint::BigInt>),
	/// Represents a Jsonnet array.
	Arr(ArrValue),
	/// Represents a Jsonnet object.
	Obj(ObjValue),
	/// Represents a Jsonnet function.
	Func(FuncVal),
}

#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(Val, [u8; 24]);

impl From<IndexableVal> for Val {
	fn from(v: IndexableVal) -> Self {
		match v {
			IndexableVal::Str(s) => Self::string(s),
			IndexableVal::Arr(a) => Self::Arr(a),
		}
	}
}

impl Val {
	pub const fn as_bool(&self) -> Option<bool> {
		match self {
			Self::Bool(v) => Some(*v),
			_ => None,
		}
	}
	pub const fn as_null(&self) -> Option<()> {
		match self {
			Self::Null => Some(()),
			_ => None,
		}
	}
	pub fn as_str(&self) -> Option<IStr> {
		match self {
			Self::Str(s) => Some(s.clone().into_flat()),
			_ => None,
		}
	}
	pub const fn as_num(&self) -> Option<f64> {
		match self {
			Self::Num(n) => Some(n.get()),
			_ => None,
		}
	}
	#[cfg(feature = "exp-bigint")]
	pub fn as_bigint(&self) -> Option<num_bigint::BigInt> {
		match self {
			Self::BigInt(n) => Some(*n.clone()),
			_ => None,
		}
	}
	pub fn as_arr(&self) -> Option<ArrValue> {
		match self {
			Self::Arr(a) => Some(a.clone()),
			_ => None,
		}
	}
	pub fn as_obj(&self) -> Option<ObjValue> {
		match self {
			Self::Obj(o) => Some(o.clone()),
			_ => None,
		}
	}
	pub fn as_func(&self) -> Option<FuncVal> {
		match self {
			Self::Func(f) => Some(f.clone()),
			_ => None,
		}
	}

	pub const fn value_type(&self) -> ValType {
		match self {
			Self::Str(..) => ValType::Str,
			Self::Num(..) => ValType::Num,
			#[cfg(feature = "exp-bigint")]
			Self::BigInt(..) => ValType::BigInt,
			Self::Arr(..) => ValType::Arr,
			Self::Obj(..) => ValType::Obj,
			Self::Bool(_) => ValType::Bool,
			Self::Null => ValType::Null,
			Self::Func(..) => ValType::Func,
		}
	}

	pub fn manifest(&self, format: impl ManifestFormat) -> Result<String> {
		fn manifest_dyn(val: &Val, manifest: &dyn ManifestFormat) -> Result<String> {
			manifest.manifest(val.clone())
		}
		manifest_dyn(self, &format)
	}

	pub fn to_string(&self) -> Result<IStr> {
		Ok(match self {
			Self::Bool(true) => "true".into(),
			Self::Bool(false) => "false".into(),
			Self::Null => "null".into(),
			Self::Str(s) => s.clone().into_flat(),
			_ => self.manifest(ToStringFormat).map(IStr::from)?,
		})
	}

	pub fn into_indexable(self) -> Result<IndexableVal> {
		Ok(match self {
			Self::Str(s) => IndexableVal::Str(s.into_flat()),
			Self::Arr(arr) => IndexableVal::Arr(arr),
			_ => bail!(ValueIsNotIndexable(self.value_type())),
		})
	}

	pub fn function(function: impl Into<FuncVal>) -> Self {
		Self::Func(function.into())
	}
	pub fn string(string: impl Into<StrValue>) -> Self {
		Self::Str(string.into())
	}
	pub fn num(num: impl Into<NumValue>) -> Self {
		Self::Num(num.into())
	}
	pub fn try_num<V, E>(num: V) -> Result<Self, E>
	where
		NumValue: TryFrom<V, Error = E>,
	{
		Ok(Self::Num(num.try_into()?))
	}
	pub fn arr(a: impl ArrayLike) -> Self {
		Self::Arr(ArrValue::new(a))
	}
}

impl From<IStr> for Val {
	fn from(value: IStr) -> Self {
		Self::string(value)
	}
}
impl From<String> for Val {
	fn from(value: String) -> Self {
		Self::string(value)
	}
}
impl From<&str> for Val {
	fn from(value: &str) -> Self {
		Self::string(value)
	}
}
impl From<ObjValue> for Val {
	fn from(value: ObjValue) -> Self {
		Self::Obj(value)
	}
}

const fn is_function_like(val: &Val) -> bool {
	matches!(val, Val::Func(_))
}

/// Native implementation of `std.primitiveEquals`
pub fn primitive_equals(val_a: &Val, val_b: &Val) -> Result<bool> {
	Ok(match (val_a, val_b) {
		(Val::Bool(a), Val::Bool(b)) => a == b,
		(Val::Null, Val::Null) => true,
		(Val::Str(a), Val::Str(b)) => a == b,
		(Val::Num(a), Val::Num(b)) => (a.get() - b.get()).abs() <= f64::EPSILON,
		#[cfg(feature = "exp-bigint")]
		(Val::BigInt(a), Val::BigInt(b)) => a == b,
		(Val::Arr(_), Val::Arr(_)) => {
			bail!("primitiveEquals operates on primitive types, got array")
		}
		(Val::Obj(_), Val::Obj(_)) => {
			bail!("primitiveEquals operates on primitive types, got object")
		}
		(a, b) if is_function_like(a) && is_function_like(b) => {
			bail!("cannot test equality of functions")
		}
		(_, _) => false,
	})
}

/// Native implementation of `std.equals`
pub fn equals(val_a: &Val, val_b: &Val) -> Result<bool> {
	if val_a.value_type() != val_b.value_type() {
		return Ok(false);
	}
	match (val_a, val_b) {
		(Val::Arr(a), Val::Arr(b)) => {
			if ArrValue::ptr_eq(a, b) {
				return Ok(true);
			}
			if a.len() != b.len() {
				return Ok(false);
			}
			for (a, b) in a.iter().zip(b.iter()) {
				if !equals(&a?, &b?)? {
					return Ok(false);
				}
			}
			Ok(true)
		}
		(Val::Obj(a), Val::Obj(b)) => {
			if ObjValue::ptr_eq(a, b) {
				return Ok(true);
			}
			let fields = a.fields(
				#[cfg(feature = "exp-preserve-order")]
				false,
			);
			if fields
				!= b.fields(
					#[cfg(feature = "exp-preserve-order")]
					false,
				) {
				return Ok(false);
			}
			for field in fields {
				if !equals(
					&a.get(field.clone())?.expect("field exists"),
					&b.get(field)?.expect("field exists"),
				)? {
					return Ok(false);
				}
			}
			Ok(true)
		}
		(a, b) => Ok(primitive_equals(a, b)?),
	}
}
