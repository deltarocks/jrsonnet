use std::{fmt::Display, rc::Rc};

pub(crate) mod conversions;
pub use conversions::*;
use jrsonnet_gcmodule::Acyclic;
pub use jrsonnet_types::{ComplexValType, ValType};
use thiserror::Error;

use crate::{
	Val,
	error::{Error, ErrorKind, Result},
	in_description_frame,
};

#[derive(Debug, Error, Clone, Acyclic)]
pub enum TypeError {
	#[error("expected {0}, got {1}")]
	ExpectedGot(ComplexValType, ValType),
	#[error("missing property {0} from {1}")]
	MissingProperty(Rc<str>, ComplexValType),
	#[error("every failed from {0}:\n{1}")]
	UnionFailed(ComplexValType, TypeLocErrorList),
	#[error(
		"number out of bounds: {0} not in {start}..{end}",
		start = .1.map(|v|v.to_string()).unwrap_or_default(),
		end = .2.map(|v|v.to_string()).unwrap_or_default(),
	)]
	BoundsFailed(f64, Option<f64>, Option<f64>),
}
impl From<TypeError> for Error {
	fn from(e: TypeError) -> Self {
		ErrorKind::TypeError(e.into()).into()
	}
}

#[derive(Debug, Clone, Acyclic)]
pub struct TypeLocError(Box<TypeError>, ValuePathStack);
impl From<TypeError> for TypeLocError {
	fn from(e: TypeError) -> Self {
		Self(Box::new(e), ValuePathStack(Vec::new()))
	}
}
impl From<TypeLocError> for Error {
	fn from(e: TypeLocError) -> Self {
		ErrorKind::TypeError(e).into()
	}
}
impl Display for TypeLocError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)?;
		if !(self.1).0.is_empty() {
			write!(f, " at {}", self.1)?;
		}
		Ok(())
	}
}

#[derive(Debug, Clone, Acyclic)]
pub struct TypeLocErrorList(Vec<TypeLocError>);
impl Display for TypeLocErrorList {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		use std::fmt::Write;
		let mut out = String::new();
		for (i, err) in self.0.iter().enumerate() {
			if i != 0 {
				writeln!(f)?;
			}
			out.clear();
			write!(out, "{err}")?;

			for (i, line) in out.lines().enumerate() {
				if line.trim().is_empty() {
					continue;
				}
				if i == 0 {
					write!(f, "  - ")?;
				} else {
					writeln!(f)?;
					write!(f, "    ")?;
				}
				write!(f, "{line}")?;
			}
		}
		Ok(())
	}
}

fn push_type_description(
	error_reason: impl Fn() -> String,
	path: impl Fn() -> ValuePathItem,
	item: impl Fn() -> Result<()>,
) -> Result<()> {
	in_description_frame(error_reason, || match item() {
		Ok(()) => Ok(()),
		Err(mut e) => {
			if let ErrorKind::TypeError(e) = &mut e.error_mut() {
				(e.1).0.push(path());
			}
			Err(e)
		}
	})
}

// TODO: check_fast for fast path of union type checking
pub trait CheckType {
	fn check(&self, value: &Val) -> Result<()>;
}

impl CheckType for ValType {
	fn check(&self, value: &Val) -> Result<()> {
		let got = value.value_type();
		if got != *self {
			let loc_error: TypeLocError = TypeError::ExpectedGot((*self).into(), got).into();
			return Err(loc_error.into());
		}
		Ok(())
	}
}

#[derive(Clone, Debug, Acyclic)]
enum ValuePathItem {
	Field(Rc<str>),
	Index(u64),
}
impl Display for ValuePathItem {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Field(name) => write!(f, ".{name:?}")?,
			Self::Index(idx) => write!(f, "[{idx}]")?,
		}
		Ok(())
	}
}

#[derive(Clone, Debug, Acyclic)]
struct ValuePathStack(Vec<ValuePathItem>);
impl Display for ValuePathStack {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "self")?;
		for elem in self.0.iter().rev() {
			write!(f, "{elem}")?;
		}
		Ok(())
	}
}

impl CheckType for ComplexValType {
	#[allow(clippy::too_many_lines)]
	fn check(&self, value: &Val) -> Result<()> {
		match self {
			Self::Any => Ok(()),
			Self::Simple(t) => t.check(value),
			Self::Char => match value {
				Val::Str(s) if s.len() == 1 || s.clone().into_flat().chars().count() == 1 => Ok(()),
				v => Err(TypeError::ExpectedGot(self.clone(), v.value_type()).into()),
			},
			Self::BoundedNumber(from, to) => {
				if let Val::Num(n) = value {
					let n = n.get();
					if from.is_some_and(|from| from > n) || to.is_some_and(|to| to < n) {
						return Err(TypeError::BoundsFailed(n, *from, *to).into());
					}
					Ok(())
				} else {
					Err(TypeError::ExpectedGot(self.clone(), value.value_type()).into())
				}
			}
			Self::Array(elem_type) => match value {
				Val::Arr(a) => {
					for (i, item) in a.iter().enumerate() {
						push_type_description(
							|| format!("array index {i}"),
							|| ValuePathItem::Index(i as u64),
							|| elem_type.check(&item.clone()?),
						)?;
					}
					Ok(())
				}
				v => Err(TypeError::ExpectedGot(self.clone(), v.value_type()).into()),
			},
			Self::ArrayRef(elem_type) => match value {
				Val::Arr(a) => {
					for (i, item) in a.iter().enumerate() {
						push_type_description(
							|| format!("array index {i}"),
							|| ValuePathItem::Index(i as u64),
							|| elem_type.check(&item.clone()?),
						)?;
					}
					Ok(())
				}
				v => Err(TypeError::ExpectedGot(self.clone(), v.value_type()).into()),
			},
			Self::AttrsOf(a) => match value {
				Val::Obj(o) => {
					for (_key, value) in o.iter(
						#[cfg(feature = "exp-preserve-order")]
						false,
					) {
						let value = value?;
						a.check(&value)?;
					}
					Ok(())
				}
				v => Err(TypeError::ExpectedGot(self.clone(), v.value_type()).into()),
			},
			Self::ObjectRef(elems) => match value {
				Val::Obj(obj) => {
					for (k, v) in *elems {
						if let Some(got_v) = obj.get((*k).into())? {
							push_type_description(
								|| format!("property {k}"),
								|| ValuePathItem::Field((*k).into()),
								|| v.check(&got_v),
							)?;
						} else {
							return Err(
								TypeError::MissingProperty((*k).into(), self.clone()).into()
							);
						}
					}
					Ok(())
				}
				v => Err(TypeError::ExpectedGot(self.clone(), v.value_type()).into()),
			},
			Self::Union(types) => {
				let mut errors = Vec::new();
				for ty in types {
					match ty.check(value) {
						Ok(()) => {
							return Ok(());
						}
						Err(e) => match e.error() {
							ErrorKind::TypeError(e) => errors.push(e.clone()),
							_ => return Err(e),
						},
					}
				}
				Err(TypeError::UnionFailed(self.clone(), TypeLocErrorList(errors)).into())
			}
			Self::UnionRef(types) => {
				let mut errors = Vec::new();
				for ty in *types {
					match ty.check(value) {
						Ok(()) => {
							return Ok(());
						}
						Err(e) => match e.error() {
							ErrorKind::TypeError(e) => errors.push(e.clone()),
							_ => return Err(e),
						},
					}
				}
				Err(TypeError::UnionFailed(self.clone(), TypeLocErrorList(errors)).into())
			}
			Self::Sum(types) => {
				for ty in types {
					ty.check(value)?;
				}
				Ok(())
			}
			Self::SumRef(types) => {
				for ty in *types {
					ty.check(value)?;
				}
				Ok(())
			}
			Self::Lazy(_lazy) => Ok(()),
		}
	}
}
