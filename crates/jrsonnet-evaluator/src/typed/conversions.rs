use std::{any::TypeId, collections::BTreeMap, marker::PhantomData, mem::transmute, ops::Deref};

use jrsonnet_gcmodule::Trace;
use jrsonnet_interner::{IBytes, IStr};
use jrsonnet_types::{ComplexValType, ValType};

use crate::{
	ObjValue, ObjValueBuilder, Result, ResultExt, Thunk, Val,
	arr::{ArrValue, BytesArray},
	bail,
	function::FuncVal,
	typed::CheckType,
	val::{IndexableVal, NumValue, StrValue, ThunkMapper},
};

#[doc(hidden)]
pub mod __typed_macro_prelude {
	pub use ::jrsonnet_evaluator::{
		IStr, ObjValue, ObjValueBuilder, State, Val,
		error::{ErrorKind, Result as JrResult},
		typed::{
			CheckType, ComplexValType, FromUntyped, IntoUntyped, ParseTypedObj, SerializeTypedObj,
			Typed,
		},
	};
}
pub use jrsonnet_macros::{FromUntyped, IntoUntyped, Typed};

#[derive(Trace)]
struct ThunkFromUntyped<K: Trace>(PhantomData<fn() -> K>);
impl<K> ThunkMapper<Val> for ThunkFromUntyped<K>
where
	K: Typed + FromUntyped + Trace,
{
	type Output = K;

	fn map(self, from: Val) -> Result<Self::Output> {
		K::from_untyped(from)
	}
}
impl<K: Trace> Default for ThunkFromUntyped<K> {
	fn default() -> Self {
		Self(PhantomData)
	}
}
#[derive(Trace)]
struct ThunkIntoUntyped<K: Trace>(PhantomData<fn() -> K>);
impl<K> ThunkMapper<K> for ThunkIntoUntyped<K>
where
	K: Typed + Trace + IntoUntyped,
{
	type Output = Val;

	fn map(self, from: K) -> Result<Self::Output> {
		K::into_untyped(from)
	}
}
impl<K: Trace> Default for ThunkIntoUntyped<K> {
	fn default() -> Self {
		Self(PhantomData)
	}
}

#[diagnostic::on_unimplemented(
	note = "don't implement `ParseTypedObj` directly, it is automatically provided by `FromUntyped` derive"
)]
pub trait ParseTypedObj: Typed {
	fn parse(obj: &ObjValue) -> Result<Self>;
}

#[diagnostic::on_unimplemented(
	note = "don't implement `SerializeTypedObj` directly, it is automatically provided by `IntoUntyped` derive"
)]
pub trait SerializeTypedObj: Typed {
	fn serialize(self, out: &mut ObjValueBuilder) -> Result<()>;
	fn into_object(self) -> Result<ObjValue> {
		let mut builder = ObjValueBuilder::new();
		self.serialize(&mut builder)?;
		Ok(builder.build())
	}
}

pub trait Typed: Sized {
	const TYPE: &'static ComplexValType;
}
pub trait IntoUntyped: Typed {
	// Whatever caller should use `into_lazy_untyped` instead of `into_untyped`
	fn provides_lazy() -> bool {
		false
	}
	fn into_untyped(typed: Self) -> Result<Val>;
	fn into_lazy_untyped(typed: Self) -> Thunk<Val> {
		Thunk::from(Self::into_untyped(typed))
	}
}
pub trait IntoUntypedResult: Typed {
	/// Hack to make builtins be able to return non-result values, and make macros able to convert those values to result
	/// This method returns identity in impl Typed for Result, and should not be overriden
	#[doc(hidden)]
	fn into_untyped_result(typed: Self) -> Result<Val>;
}
impl<T> IntoUntypedResult for T
where
	T: IntoUntyped,
{
	fn into_untyped_result(typed: Self) -> Result<Val> {
		T::into_untyped(typed)
	}
}

pub trait FromUntyped: Typed {
	fn from_untyped(untyped: Val) -> Result<Self>;
	fn from_lazy_untyped(lazy: Thunk<Val>) -> Result<Self> {
		Self::from_untyped(lazy.evaluate()?)
	}

	// Whatever caller should use `from_lazy_untyped` instead of `from_untyped` when possible
	fn wants_lazy() -> bool {
		false
	}
}

impl<T> Typed for Thunk<T>
where
	T: Typed + Trace + Clone,
{
	const TYPE: &'static ComplexValType = &ComplexValType::Lazy(T::TYPE);
}

fn try_cast_thunk_val<T: 'static>(typed: Thunk<T>) -> Result<Thunk<Val>, Thunk<T>> {
	if TypeId::of::<T>() == TypeId::of::<Val>() {
		// SAFETY: We know that it is exactly the same type, and we discard the original after that
		// to avoid double-free.
		let transmuted = unsafe { transmute::<Thunk<T>, Thunk<Val>>(typed) };
		Ok(transmuted)
	} else {
		Err(typed)
	}
}
impl<T> IntoUntyped for Thunk<T>
where
	T: IntoUntyped + Trace + Clone,
{
	fn into_untyped(typed: Self) -> Result<Val> {
		T::into_untyped(typed.evaluate()?)
	}
	fn provides_lazy() -> bool {
		true
	}

	fn into_lazy_untyped(inner: Self) -> Thunk<Val> {
		// Avoid lazy mapping
		let inner = match try_cast_thunk_val(inner) {
			Ok(v) => return v,
			Err(e) => e,
		};
		inner.map(<ThunkIntoUntyped<T>>::default())
	}
}

fn try_cast_thunk_t<T: 'static>(typed: Thunk<Val>) -> Result<Thunk<T>, Thunk<Val>> {
	if TypeId::of::<T>() == TypeId::of::<Val>() {
		// SAFETY: We know that it is exactly the same type, and we discard the original after that
		// to avoid double-free.
		let transmuted = unsafe { transmute::<Thunk<Val>, Thunk<T>>(typed) };
		Ok(transmuted)
	} else {
		Err(typed)
	}
}
impl<T> FromUntyped for Thunk<T>
where
	T: Typed + FromUntyped + Trace + Clone,
{
	fn from_untyped(untyped: Val) -> Result<Self> {
		Self::from_lazy_untyped(Thunk::evaluated(untyped))
	}

	fn wants_lazy() -> bool {
		true
	}

	fn from_lazy_untyped(inner: Thunk<Val>) -> Result<Self> {
		// Avoid lazy mapping
		let inner = match try_cast_thunk_t(inner) {
			Ok(v) => return Ok(v),
			Err(e) => e,
		};
		Ok(inner.map(<ThunkFromUntyped<T>>::default()))
	}
}

#[expect(clippy::cast_precision_loss, reason = "checked to not overflow")]
pub const MAX_SAFE_INTEGER: f64 = ((1u64 << (f64::MANTISSA_DIGITS)) - 1) as f64;
#[expect(clippy::cast_precision_loss, reason = "checked to not overflow")]
pub const MIN_SAFE_INTEGER: f64 = (-((1i64 << (f64::MANTISSA_DIGITS)) - 1)) as f64;

macro_rules! impl_int {
	($($ty:ty)*) => {$(
		impl Typed for $ty {
			const TYPE: &'static ComplexValType =
				&ComplexValType::BoundedNumber(Some(Self::MIN as f64), Some(Self::MAX as f64));
		}
		impl FromUntyped for $ty {
			fn from_untyped(value: Val) -> Result<Self> {
				<Self as Typed>::TYPE.check(&value)?;
				match value {
					Val::Num(n) => {
						let n = n.get();
						#[allow(clippy::float_cmp)]
						if n.trunc() != n {
							bail!(
								"cannot convert number with fractional part to {}",
								stringify!($ty)
							)
						}
						#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation, reason = "checked by TYPE")]
						Ok(n as Self)
					}
					_ => unreachable!(),
				}
			}
		}
		impl IntoUntyped for $ty {
			fn into_untyped(value: Self) -> Result<Val> {
				Ok(Val::Num(value.into()))
			}
		}
	)*};
}

impl_int!(i8 u8 i16 u16 i32 u32);

macro_rules! impl_bounded_int {
	($($name:ident = $ty:ty)*) => {$(
		#[derive(Clone, Copy)]
		#[allow(clippy::cast_possible_truncation, reason = "overflow is api misuse")]
		pub struct $name<const MIN: $ty, const MAX: $ty>($ty);
		impl<const MIN: $ty, const MAX: $ty> $name<MIN, MAX> {
			pub const fn new(value: $ty) -> Option<$name<MIN, MAX>> {
				if value >= MIN && value <= MAX {
					Some(Self(value))
				} else {
					None
				}
			}
			pub const fn value(self) -> $ty {
				self.0
			}
		}
		impl<const MIN: $ty, const MAX: $ty> Deref for $name<MIN, MAX> {
			type Target = $ty;
			fn deref(&self) -> &Self::Target {
				&self.0
			}
		}

		impl<const MIN: $ty, const MAX: $ty> Typed for $name<MIN, MAX> {
			#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, reason = "overflow is api misuse")]
			const TYPE: &'static ComplexValType =
				&ComplexValType::BoundedNumber(
					Some(MIN as f64),
					Some(MAX as f64),
				);
		}

		impl<const MIN: $ty, const MAX: $ty> FromUntyped for $name<MIN, MAX> {
			fn from_untyped(value: Val) -> Result<Self> {
				<Self as Typed>::TYPE.check(&value)?;
				match value {
					Val::Num(n) => {
						let n = n.get();
						#[allow(clippy::float_cmp)]
						if n.trunc() != n {
							bail!(
								"cannot convert number with fractional part to {}",
								stringify!($ty)
							)
						}
						#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "overflow is api misuse, the range is checked by TYPE")]
						Ok(Self(n as $ty))
					}
					_ => unreachable!(),
				}
			}
		}

		impl<const MIN: $ty, const MAX: $ty> IntoUntyped for $name<MIN, MAX> {
			#[allow(clippy::cast_lossless)]
			fn into_untyped(value: Self) -> Result<Val> {
				Ok(Val::try_num(value.0)?)
			}
		}
	)*};
}

impl_bounded_int!(
	BoundedI8 = i8
	BoundedI16 = i16
	BoundedI32 = i32
	BoundedI64 = i64
	BoundedUsize = usize
);

impl Typed for f64 {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Num);
}
impl IntoUntyped for f64 {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::try_num(value)?)
	}
}
impl FromUntyped for f64 {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Num(n) => Ok(n.get()),
			_ => unreachable!(),
		}
	}
}

pub struct PositiveF64(pub f64);
impl Typed for PositiveF64 {
	const TYPE: &'static ComplexValType = &ComplexValType::BoundedNumber(Some(0.0), None);
}
impl IntoUntyped for PositiveF64 {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::try_num(value.0)?)
	}
}
impl FromUntyped for PositiveF64 {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Num(n) => Ok(Self(n.get())),
			_ => unreachable!(),
		}
	}
}
impl Typed for usize {
	const TYPE: &'static ComplexValType =
		&ComplexValType::BoundedNumber(Some(0.0), Some(MAX_SAFE_INTEGER));
}
impl IntoUntyped for usize {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::try_num(value)?)
	}
}
impl FromUntyped for usize {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Num(n) => {
				let n = n.get();
				#[allow(clippy::float_cmp)]
				if n.trunc() != n {
					bail!("cannot convert number with fractional part to usize")
				}
				#[allow(
					clippy::cast_possible_truncation,
					clippy::cast_sign_loss,
					reason = "the range is checked by TYPE"
				)]
				Ok(n as Self)
			}
			_ => unreachable!(),
		}
	}
}

impl Typed for IStr {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Str);
}
impl IntoUntyped for IStr {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::string(value))
	}
}
impl FromUntyped for IStr {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Str(s) => Ok(s.into_flat()),
			_ => unreachable!(),
		}
	}
}

impl Typed for String {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Str);
}
impl IntoUntyped for String {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::string(value))
	}
}
impl FromUntyped for String {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Str(s) => Ok(s.to_string()),
			_ => unreachable!(),
		}
	}
}

impl Typed for StrValue {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Str);
}
impl IntoUntyped for StrValue {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::Str(value))
	}
}
impl FromUntyped for StrValue {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Str(s) => Ok(s),
			_ => unreachable!(),
		}
	}
}

impl Typed for char {
	const TYPE: &'static ComplexValType = &ComplexValType::Char;
}
impl IntoUntyped for char {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::string(value))
	}
}
impl FromUntyped for char {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Str(s) => Ok(s.into_flat().chars().next().unwrap()),
			_ => unreachable!(),
		}
	}
}

// TODO: View into vec using ArrayLike?
impl<T> Typed for Vec<T>
where
	T: Typed,
{
	const TYPE: &'static ComplexValType = &ComplexValType::ArrayRef(T::TYPE);
}
impl<T: Typed + IntoUntyped> IntoUntyped for Vec<T> {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::Arr(
			value
				.into_iter()
				.map(T::into_untyped)
				.collect::<Result<ArrValue>>()?,
		))
	}
}
impl<T: Typed + FromUntyped> FromUntyped for Vec<T> {
	fn from_untyped(value: Val) -> Result<Self> {
		let Val::Arr(a) = value else {
			<Self as Typed>::TYPE.check(&value)?;
			unreachable!("typecheck should fail")
		};
		a.iter()
			.enumerate()
			.map(|(i, r)| {
				r.and_then(|t| {
					T::from_untyped(t).with_description(|| format!("parsing elem <{i}>"))
				})
			})
			.collect::<Result<Self>>()
	}
}

// TODO: View into BTreeMap using ObjectCore?
impl<K, V> Typed for BTreeMap<K, V>
where
	K: Typed + Ord,
	V: Typed,
{
	const TYPE: &'static ComplexValType = &ComplexValType::AttrsOf(V::TYPE);
}
impl<K, V> IntoUntyped for BTreeMap<K, V>
where
	K: Typed + Ord + IntoUntyped,
	V: Typed + IntoUntyped,
{
	fn into_untyped(typed: Self) -> Result<Val> {
		let mut out = ObjValueBuilder::with_capacity(typed.len());
		for (k, v) in typed {
			let Some(key) = K::into_untyped(k)?.as_str() else {
				bail!("map key should serialize to string");
			};
			let value = V::into_untyped(v)?;
			out.field(key).value(value);
		}
		Ok(Val::Obj(out.build()))
	}
}
impl<K, V> FromUntyped for BTreeMap<K, V>
where
	K: FromUntyped + Ord,
	V: FromUntyped,
{
	fn from_untyped(value: Val) -> Result<Self> {
		Self::TYPE.check(&value)?;
		let obj = value.as_obj().expect("typecheck should fail");

		let mut out = Self::new();
		if V::wants_lazy() {
			for key in obj.fields_ex(
				false,
				#[cfg(feature = "exp-preserve-order")]
				false,
			) {
				let value = obj.get_lazy(key.clone()).expect("field exists");
				let value = V::from_lazy_untyped(value)?;
				let key = K::from_untyped(Val::Str(key.into()))?;
				let _ = out.insert(key, value);
			}
		} else {
			for (key, value) in obj.iter(
				#[cfg(feature = "exp-preserve-order")]
				false,
			) {
				let key = K::from_untyped(Val::Str(key.into()))?;
				let value = V::from_untyped(value?)?;
				let _ = out.insert(key, value);
			}
		}
		Ok(out)
	}
}

impl Typed for Val {
	const TYPE: &'static ComplexValType = &ComplexValType::Any;
}
impl IntoUntyped for Val {
	fn into_untyped(typed: Self) -> Result<Val> {
		Ok(typed)
	}
}
impl FromUntyped for Val {
	fn from_untyped(untyped: Val) -> Result<Self> {
		Ok(untyped)
	}
}

#[doc(hidden)]
impl<T> Typed for Result<T>
where
	T: Typed,
{
	const TYPE: &'static ComplexValType = &ComplexValType::Any;
}
impl<T: IntoUntyped> IntoUntypedResult for Result<T> {
	fn into_untyped_result(typed: Self) -> Result<Val> {
		typed.map(T::into_untyped)?
	}
}

/// Specialization
impl Typed for IBytes {
	const TYPE: &'static ComplexValType =
		&ComplexValType::ArrayRef(&ComplexValType::BoundedNumber(Some(0.0), Some(255.0)));
}
impl IntoUntyped for IBytes {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::Arr(ArrValue::bytes(value)))
	}
}
impl FromUntyped for IBytes {
	fn from_untyped(value: Val) -> Result<Self> {
		let Val::Arr(a) = &value else {
			<Self as Typed>::TYPE.check(&value)?;
			unreachable!()
		};
		if let Some(bytes) = a.as_any().downcast_ref::<BytesArray>() {
			return Ok(bytes.0.as_slice().into());
		}
		<Self as Typed>::TYPE.check(&value)?;
		// Any::downcast_ref::<ByteArray>(&a);
		let mut out = Vec::with_capacity(a.len());
		for e in a.iter() {
			let r = e?;
			out.push(u8::from_untyped(r)?);
		}
		Ok(out.as_slice().into())
	}
}

pub struct M1;
impl Typed for M1 {
	const TYPE: &'static ComplexValType = &ComplexValType::BoundedNumber(Some(-1.0), Some(-1.0));
}
impl IntoUntyped for M1 {
	fn into_untyped(_: Self) -> Result<Val> {
		Ok(Val::Num(NumValue::new(-1.0).expect("finite")))
	}
}
impl FromUntyped for M1 {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		Ok(Self)
	}
}

macro_rules! decl_either {
	($($name: ident, $($id: ident)*);*) => {$(
		#[derive(Clone)]
		pub enum $name<$($id),*> {
			$($id($id)),*
		}
		impl<$($id),*> Typed for $name<$($id),*>
		where
			$($id: Typed,)*
		{
			const TYPE: &'static ComplexValType = &ComplexValType::UnionRef(&[$($id::TYPE),*]);
		}
		impl<$($id),*> IntoUntyped for $name<$($id),*>
		where
			$($id: Typed + IntoUntyped,)*
		{
			fn into_untyped(value: Self) -> Result<Val> {
				match value {$(
					$name::$id(v) => $id::into_untyped(v)
				),*}
			}
		}

		impl<$($id),*> FromUntyped for $name<$($id),*>
		where
			$($id: Typed + FromUntyped,)*
		{
			fn from_untyped(value: Val) -> Result<Self> {
				$(
					if $id::TYPE.check(&value).is_ok() {
						$id::from_untyped(value).map(Self::$id)
					} else
				)* {
					<Self as Typed>::TYPE.check(&value)?;
					unreachable!()
				}
			}
		}
	)*}
}
decl_either!(
	Either1, A;
	Either2, A B;
	Either3, A B C;
	Either4, A B C D;
	Either5, A B C D E;
	Either6, A B C D E F;
	Either7, A B C D E F G
);
#[macro_export]
macro_rules! Either {
	($a:ty) => {$crate::typed::Either1<$a>};
	($a:ty, $b:ty) => {$crate::typed::Either2<$a, $b>};
	($a:ty, $b:ty, $c:ty) => {$crate::typed::Either3<$a, $b, $c>};
	($a:ty, $b:ty, $c:ty, $d:ty) => {$crate::typed::Either4<$a, $b, $c, $d>};
	($a:ty, $b:ty, $c:ty, $d:ty, $e:ty) => {$crate::typed::Either5<$a, $b, $c, $d, $e>};
	($a:ty, $b:ty, $c:ty, $d:ty, $e:ty, $f:ty) => {$crate::typed::Either6<$a, $b, $c, $d, $e, $f>};
	($a:ty, $b:ty, $c:ty, $d:ty, $e:ty, $f:ty, $g:ty) => {$crate::typed::Either7<$a, $b, $c, $d, $e, $f, $g>};
}
pub use Either;

pub type MyType = Either![u32, f64, String];

impl Typed for ArrValue {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Arr);
}
impl IntoUntyped for ArrValue {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::Arr(value))
	}
}
impl FromUntyped for ArrValue {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Arr(a) => Ok(a),
			_ => unreachable!(),
		}
	}
}

impl Typed for FuncVal {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Func);
}
impl IntoUntyped for FuncVal {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::Func(value))
	}
}
impl FromUntyped for FuncVal {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Func(a) => Ok(a),
			_ => unreachable!(),
		}
	}
}

impl Typed for ObjValue {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Obj);
}
impl IntoUntyped for ObjValue {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::Obj(value))
	}
}
impl FromUntyped for ObjValue {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Obj(a) => Ok(a),
			_ => unreachable!(),
		}
	}
}

impl Typed for bool {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Bool);
}
impl IntoUntyped for bool {
	fn into_untyped(value: Self) -> Result<Val> {
		Ok(Val::Bool(value))
	}
}
impl FromUntyped for bool {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		match value {
			Val::Bool(a) => Ok(a),
			_ => unreachable!(),
		}
	}
}

impl Typed for IndexableVal {
	const TYPE: &'static ComplexValType = &ComplexValType::UnionRef(&[
		&ComplexValType::Simple(ValType::Arr),
		&ComplexValType::Simple(ValType::Str),
	]);
}
impl IntoUntyped for IndexableVal {
	fn into_untyped(value: Self) -> Result<Val> {
		match value {
			Self::Str(s) => Ok(Val::string(s)),
			Self::Arr(a) => Ok(Val::Arr(a)),
		}
	}
}
impl FromUntyped for IndexableVal {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		value.into_indexable()
	}
}

pub struct Null;
impl Typed for Null {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Null);
}
impl IntoUntyped for Null {
	fn into_untyped(_: Self) -> Result<Val> {
		Ok(Val::Null)
	}
}
impl FromUntyped for Null {
	fn from_untyped(value: Val) -> Result<Self> {
		<Self as Typed>::TYPE.check(&value)?;
		Ok(Self)
	}
}

impl<T> Typed for Option<T>
where
	T: Typed,
{
	const TYPE: &'static ComplexValType =
		&ComplexValType::UnionRef(&[&ComplexValType::Simple(ValType::Null), T::TYPE]);
}
impl<T> IntoUntyped for Option<T>
where
	T: Typed + IntoUntyped,
{
	fn into_untyped(typed: Self) -> Result<Val> {
		typed.map_or_else(|| Ok(Val::Null), |v| T::into_untyped(v))
	}
}
impl<T> FromUntyped for Option<T>
where
	T: Typed + FromUntyped,
{
	fn from_untyped(untyped: Val) -> Result<Self> {
		if matches!(untyped, Val::Null) {
			Ok(None)
		} else {
			T::from_untyped(untyped).map(Some)
		}
	}
}

impl Typed for NumValue {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Num);
}
impl IntoUntyped for NumValue {
	fn into_untyped(typed: Self) -> Result<Val> {
		Ok(Val::Num(typed))
	}
}
impl FromUntyped for NumValue {
	fn from_untyped(untyped: Val) -> Result<Self> {
		Self::TYPE.check(&untyped)?;
		match untyped {
			Val::Num(v) => Ok(v),
			_ => unreachable!(),
		}
	}
}
