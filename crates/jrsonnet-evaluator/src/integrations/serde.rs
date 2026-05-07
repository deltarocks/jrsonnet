use std::borrow::Cow;

use jrsonnet_interner::{IBytes, IStr};
use jrsonnet_ir::NumValue;
use serde::{
	Deserialize, Serialize, Serializer,
	de::{self, Visitor},
	ser::{
		Error, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
		SerializeTupleStruct, SerializeTupleVariant,
	},
};

use crate::{
	Error as JrError, ObjValue, ObjValueBuilder, Result, Val, error, in_description_frame,
};

impl<'de> Deserialize<'de> for Val {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct ValVisitor;

		// macro_rules! visit_num {
		// 	($($method:ident => $ty:ty),* $(,)?) => {$(
		// 		fn $method<E>(self, v: $ty) -> Result<Self::Value, E>
		// 		where
		// 			E: serde::de::Error,
		// 		{
		// 			Ok(Val::Num(f64::from(v)))
		// 		}
		// 	)*};
		// }

		impl<'de> Visitor<'de> for ValVisitor {
			type Value = Val;

			fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(Val::Bool(v))
			}
			fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(Val::Num(NumValue::new(v).ok_or_else(|| {
					E::custom("only finite numbers are supported")
				})?))
			}
			fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(Val::string(v))
			}

			// visit_num! {
			// 	visit_i8 => i8,
			// 	visit_i16 => i16,
			// 	visit_i32 => i32,
			// 	visit_u8 => u8,
			// 	visit_u16 => u16,
			// 	visit_u32 => u32,
			// }
			fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				#[expect(
					clippy::cast_precision_loss,
					reason = "this is how it works with stdlib functions"
				)]
				Ok(Val::Num(NumValue::new(v as f64).expect("no overflow")))
			}
			fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				#[expect(
					clippy::cast_precision_loss,
					reason = "this is how it works with stdlib functions"
				)]
				Ok(Val::Num(NumValue::new(v as f64).expect("no overflow")))
			}

			fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(Val::arr(IBytes::from(v)))
			}

			fn visit_none<E>(self) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(Val::Null)
			}
			fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
			where
				D: serde::Deserializer<'de>,
			{
				deserializer.deserialize_any(self)
			}

			fn visit_unit<E>(self) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(Val::Null)
			}

			fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
			where
				D: serde::Deserializer<'de>,
			{
				deserializer.deserialize_any(self)
			}

			fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
			where
				A: de::SeqAccess<'de>,
			{
				let mut out = seq.size_hint().map_or_else(Vec::new, Vec::with_capacity);

				while let Some(val) = seq.next_element::<Val>()? {
					out.push(val);
				}

				Ok(Val::arr(out))
			}

			fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
			where
				A: de::MapAccess<'de>,
			{
				let mut out = map
					.size_hint()
					.map_or_else(ObjValueBuilder::new, ObjValueBuilder::with_capacity);

				while let Some((k, v)) = map.next_entry::<Cow<'de, str>, Val>()? {
					// Jsonnet ignores duplicate keys
					out.field(k).value(v);
				}

				Ok(Val::Obj(out.build()))
			}

			fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
				write!(formatter, "any valid jsonnet value")
			}
		}
		deserializer.deserialize_any(ValVisitor)
	}
}

impl Serialize for Val {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		match self {
			Self::Bool(v) => serializer.serialize_bool(*v),
			Self::Null => serializer.serialize_none(),
			Self::Str(s) => serializer.serialize_str(&s.clone().into_flat()),
			Self::Num(n) => {
				let n = n.get();
				if n.fract() == 0.0 {
					#[expect(
						clippy::cast_possible_truncation,
						reason = "no correct implementation is possible here; expected"
					)]
					let n = n as i64;
					serializer.serialize_i64(n)
				} else {
					serializer.serialize_f64(n)
				}
			}
			#[cfg(feature = "exp-bigint")]
			Self::BigInt(b) => b.serialize(serializer),
			Self::Arr(arr) => {
				let mut seq = serializer.serialize_seq(Some(arr.len()))?;
				for (i, element) in arr.iter().enumerate() {
					let mut serde_error = None;
					in_description_frame(
						|| format!("array index [{i}]"),
						|| {
							let e = element?;
							if let Err(e) = seq.serialize_element(&e) {
								serde_error = Some(e);
							}
							Ok(())
						},
					)
					.map_err(|e| S::Error::custom(e.to_string()))?;
					if let Some(e) = serde_error {
						return Err(e);
					}
				}
				seq.end()
			}
			Self::Obj(obj) => {
				let mut map = serializer.serialize_map(Some(obj.len32() as usize))?;
				for (field, value) in obj.iter(
					#[cfg(feature = "exp-preserve-order")]
					true,
				) {
					let mut serde_error = None;
					// TODO: rewrite using try{} after stabilization
					in_description_frame(
						|| format!("object field {field:?}"),
						|| {
							let v = value?;
							if let Err(e) = map.serialize_entry(field.as_str(), &v) {
								serde_error = Some(e);
							}
							Ok(())
						},
					)
					.map_err(|e| S::Error::custom(e.to_string()))?;
					if let Some(e) = serde_error {
						return Err(e);
					}
				}
				map.end()
			}
			Self::Func(_) => Err(S::Error::custom("tried to manifest function")),
		}
	}
}

struct IntoVecValSerializer {
	variant: Option<IStr>,
	data: Vec<Val>,
}
impl IntoVecValSerializer {
	fn new() -> Self {
		Self {
			variant: None,
			data: Vec::new(),
		}
	}
	fn with_capacity(capacity: usize) -> Self {
		Self {
			variant: None,
			data: Vec::with_capacity(capacity),
		}
	}
	fn variant_with_capacity(variant: impl Into<IStr>, capacity: usize) -> Self {
		Self {
			variant: Some(variant.into()),
			data: Vec::with_capacity(capacity),
		}
	}
}
impl SerializeSeq for IntoVecValSerializer {
	type Ok = Val;
	type Error = JrError;

	fn serialize_element<T>(&mut self, value: &T) -> Result<()>
	where
		T: ?Sized + Serialize,
	{
		let value = value.serialize(IntoValSerializer)?;
		self.data.push(value);
		Ok(())
	}

	fn end(self) -> Result<Val> {
		let inner = Val::arr(self.data);
		if let Some(variant) = self.variant {
			let mut out = ObjValue::builder_with_capacity(1);
			out.field(variant).value(inner);
			Ok(Val::Obj(out.build()))
		} else {
			Ok(inner)
		}
	}
}
impl SerializeTuple for IntoVecValSerializer {
	type Ok = Val;
	type Error = JrError;

	fn serialize_element<T>(&mut self, value: &T) -> Result<()>
	where
		T: ?Sized + Serialize,
	{
		SerializeSeq::serialize_element(self, value)
	}

	fn end(self) -> Result<Val> {
		SerializeSeq::end(self)
	}
}
impl SerializeTupleVariant for IntoVecValSerializer {
	type Ok = Val;
	type Error = JrError;

	fn serialize_field<T>(&mut self, value: &T) -> Result<()>
	where
		T: ?Sized + Serialize,
	{
		SerializeSeq::serialize_element(self, value)
	}

	fn end(self) -> Result<Val> {
		SerializeSeq::end(self)
	}
}
impl SerializeTupleStruct for IntoVecValSerializer {
	type Ok = Val;
	type Error = JrError;

	fn serialize_field<T>(&mut self, value: &T) -> Result<()>
	where
		T: ?Sized + Serialize,
	{
		SerializeSeq::serialize_element(self, value)
	}

	fn end(self) -> Result<Val> {
		SerializeSeq::end(self)
	}
}

struct IntoObjValueSerializer {
	variant: Option<IStr>,
	data: ObjValueBuilder,
	key: Option<IStr>,
}
impl IntoObjValueSerializer {
	fn new() -> Self {
		Self {
			variant: None,
			data: ObjValue::builder(),
			key: None,
		}
	}
	fn with_capacity(capacity: usize) -> Self {
		Self {
			variant: None,
			data: ObjValue::builder_with_capacity(capacity),
			key: None,
		}
	}
	fn variant_with_capacity(variant: impl Into<IStr>, capacity: usize) -> Self {
		Self {
			variant: Some(variant.into()),
			data: ObjValue::builder_with_capacity(capacity),
			key: None,
		}
	}
}
impl SerializeMap for IntoObjValueSerializer {
	type Ok = Val;
	type Error = JrError;

	fn serialize_key<T>(&mut self, key: &T) -> Result<()>
	where
		T: ?Sized + Serialize,
	{
		let key = key.serialize(IntoValSerializer)?;
		let key = key.to_string()?;
		self.key = Some(key);
		Ok(())
	}

	fn serialize_value<T>(&mut self, value: &T) -> Result<()>
	where
		T: ?Sized + Serialize,
	{
		let key = self.key.take().expect("no serialize_key called");
		let value = value.serialize(IntoValSerializer)?;
		self.data.field(key).try_value(value)?;
		Ok(())
	}

	// TODO: serialize_key/serialize_value
	fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<()>
	where
		K: ?Sized + Serialize,
		V: ?Sized + Serialize,
	{
		let key = key.serialize(IntoValSerializer)?;
		let key = key.to_string()?;
		let value = value.serialize(IntoValSerializer)?;
		self.data.field(key).try_value(value)?;
		Ok(())
	}

	fn end(self) -> Result<Val> {
		let inner = Val::Obj(self.data.build());
		if let Some(variant) = self.variant {
			let mut out = ObjValue::builder_with_capacity(1);
			out.field(variant).value(inner);
			Ok(Val::Obj(out.build()))
		} else {
			Ok(inner)
		}
	}
}
impl SerializeStruct for IntoObjValueSerializer {
	type Ok = Val;
	type Error = JrError;

	fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
	where
		T: ?Sized + Serialize,
	{
		SerializeMap::serialize_entry(self, key, value)?;
		Ok(())
	}

	fn end(self) -> Result<Val> {
		SerializeMap::end(self)
	}
}
impl SerializeStructVariant for IntoObjValueSerializer {
	type Ok = Val;

	type Error = JrError;

	fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<()>
	where
		T: ?Sized + Serialize,
	{
		SerializeMap::serialize_entry(self, key, value)?;
		Ok(())
	}

	fn end(self) -> Result<Val> {
		SerializeMap::end(self)
	}
}

struct IntoValSerializer;
impl Serializer for IntoValSerializer {
	type Ok = Val;

	type Error = JrError;

	type SerializeSeq = IntoVecValSerializer;

	type SerializeTuple = IntoVecValSerializer;

	type SerializeTupleStruct = IntoVecValSerializer;

	type SerializeTupleVariant = IntoVecValSerializer;

	type SerializeMap = IntoObjValueSerializer;

	type SerializeStruct = IntoObjValueSerializer;

	type SerializeStructVariant = IntoObjValueSerializer;

	fn serialize_bool(self, v: bool) -> Result<Val> {
		Ok(Val::Bool(v))
	}

	fn serialize_i8(self, v: i8) -> Result<Val> {
		Ok(Val::Num(v.into()))
	}

	fn serialize_i16(self, v: i16) -> Result<Val> {
		Ok(Val::Num(v.into()))
	}

	fn serialize_i32(self, v: i32) -> Result<Val> {
		Ok(Val::Num(v.into()))
	}

	fn serialize_i64(self, v: i64) -> Result<Val> {
		Ok(Val::Str(v.to_string().into()))
	}

	fn serialize_u8(self, v: u8) -> Result<Val> {
		Ok(Val::Num(v.into()))
	}

	fn serialize_u16(self, v: u16) -> Result<Val> {
		Ok(Val::Num(v.into()))
	}

	fn serialize_u32(self, v: u32) -> Result<Val> {
		Ok(Val::Num(v.into()))
	}

	fn serialize_u64(self, v: u64) -> Result<Val> {
		Ok(Val::Str(v.to_string().into()))
	}

	fn serialize_f32(self, v: f32) -> Result<Val> {
		Ok(Val::try_num(f64::from(v))?)
	}

	fn serialize_f64(self, v: f64) -> Result<Val> {
		Ok(Val::try_num(v)?)
	}

	fn serialize_char(self, v: char) -> Result<Val> {
		Ok(Val::Str(v.to_string().into()))
	}

	fn serialize_str(self, v: &str) -> Result<Val> {
		Ok(Val::Str(v.into()))
	}

	fn serialize_bytes(self, v: &[u8]) -> Result<Val> {
		Ok(Val::arr(IBytes::from(v)))
	}

	fn serialize_none(self) -> Result<Val> {
		Ok(Val::Null)
	}

	fn serialize_some<T>(self, value: &T) -> Result<Val>
	where
		T: ?Sized + Serialize,
	{
		value.serialize(self)
	}

	fn serialize_unit(self) -> Result<Val> {
		Ok(Val::Null)
	}

	fn serialize_unit_struct(self, _name: &'static str) -> Result<Val> {
		Ok(Val::Null)
	}

	fn serialize_unit_variant(
		self,
		_name: &'static str,
		_variant_index: u32,
		variant: &'static str,
	) -> Result<Val> {
		Ok(Val::Str(variant.into()))
	}

	fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<Val>
	where
		T: ?Sized + Serialize,
	{
		value.serialize(self)
	}

	fn serialize_newtype_variant<T>(
		self,
		_name: &'static str,
		_variant_index: u32,
		variant: &'static str,
		value: &T,
	) -> Result<Val>
	where
		T: ?Sized + Serialize,
	{
		let mut out = ObjValue::builder_with_capacity(1);
		let value = value.serialize(self)?;
		out.field(variant).value(value);
		Ok(Val::Obj(out.build()))
	}

	fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
		Ok(len.map_or_else(
			IntoVecValSerializer::new,
			IntoVecValSerializer::with_capacity,
		))
	}

	fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
		Ok(IntoVecValSerializer::with_capacity(len))
	}

	fn serialize_tuple_struct(
		self,
		_name: &'static str,
		len: usize,
	) -> Result<Self::SerializeTupleStruct, Self::Error> {
		Ok(IntoVecValSerializer::with_capacity(len))
	}

	fn serialize_tuple_variant(
		self,
		_name: &'static str,
		_variant_index: u32,
		variant: &'static str,
		len: usize,
	) -> Result<Self::SerializeTupleVariant, Self::Error> {
		Ok(IntoVecValSerializer::variant_with_capacity(variant, len))
	}

	fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
		Ok(len.map_or_else(
			IntoObjValueSerializer::new,
			IntoObjValueSerializer::with_capacity,
		))
	}

	fn serialize_struct(
		self,
		_name: &'static str,
		len: usize,
	) -> Result<Self::SerializeStruct, Self::Error> {
		Ok(IntoObjValueSerializer::with_capacity(len))
	}

	fn serialize_struct_variant(
		self,
		_name: &'static str,
		_variant_index: u32,
		variant: &'static str,
		len: usize,
	) -> Result<Self::SerializeStructVariant, Self::Error> {
		Ok(IntoObjValueSerializer::variant_with_capacity(variant, len))
	}
}

impl Val {
	pub fn from_serde(v: impl Serialize) -> Result<Self, JrError> {
		v.serialize(IntoValSerializer)
	}
}

impl serde::ser::Error for JrError {
	fn custom<T>(msg: T) -> Self
	where
		T: std::fmt::Display,
	{
		error!("serde: {msg}")
	}
}
