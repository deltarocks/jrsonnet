use std::{cmp::Ordering, convert::Infallible, fmt};

use jrsonnet_gcmodule::{Acyclic, Trace};
use jrsonnet_interner::IStr;
use jrsonnet_ir::{
	BinaryOpType, ConvertNumValueError, Source, SourcePath, Span, Spanned, UnaryOpType,
};
use jrsonnet_types::ValType;
use thiserror::Error;

use crate::{
	ObjValue, ResolvePathOwned,
	analyze::Diagnostic,
	function::{CallLocation, FunctionSignature, ParamName},
	stdlib::format::FormatError,
	typed::TypeLocError,
};

#[derive(Debug, Clone, Acyclic)]
pub struct SyntaxError {
	pub message: String,
	pub location: Span,
}
impl fmt::Display for SyntaxError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.message)
	}
}

pub(crate) fn format_found(list: &[IStr], what: &str) -> String {
	if list.is_empty() {
		return String::new();
	}
	let mut out = String::new();
	out.push_str("\nThere ");
	if list.len() > 1 {
		out.push_str("are ");
	} else {
		out.push_str("is a ");
	}
	out.push_str(what);
	if list.len() > 1 {
		out.push('s');
	}
	out.push_str(" with similar name");
	if list.len() > 1 {
		out.push('s');
	}
	out.push_str(" present: ");
	for (i, v) in list.iter().enumerate() {
		if i != 0 {
			out.push_str(", ");
		}
		out.push_str(v as &str);
	}
	out
}

const fn format_empty_str(str: &str) -> &str {
	if str.is_empty() {
		"\"\" (empty string)"
	} else {
		str
	}
}

pub(crate) fn suggest_names<'a, 'b>(
	name: &'a IStr,
	names: impl IntoIterator<Item = &'b IStr>,
) -> Vec<IStr> {
	let mut heap: Vec<(f64, IStr)> = names
		.into_iter()
		.filter_map(|def| {
			let conf = strsim::jaro_winkler(def.as_str(), name.as_str());
			if conf < 0.8 {
				return None;
			}
			debug_assert!(
				def.as_str() != name.as_str(),
				"string pooling failure: look for DOC(string-pooling) comment in jrsonnet-interner"
			);

			Some((conf, def.clone()))
		})
		.collect();
	heap.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
	heap.into_iter().map(|v| v.1).collect()
}

pub(crate) fn suggest_object_fields(v: &ObjValue, key: IStr) -> Vec<IStr> {
	let fields = v.fields_ex(
		true,
		#[cfg(feature = "exp-preserve-order")]
		false,
	);
	suggest_names(&key, &fields)
}

/// Possible errors
#[allow(missing_docs)]
#[derive(Error, Debug, Clone, Trace)]
#[non_exhaustive]
pub enum ErrorKind {
	#[error("intrinsic not found: {0}")]
	IntrinsicNotFound(IStr),

	#[error("operator {0} does not operate on type {1}")]
	UnaryOperatorDoesNotOperateOnType(UnaryOpType, ValType),
	#[error("binary operation {1} {0} {2} is not implemented")]
	BinaryOperatorDoesNotOperateOnValues(BinaryOpType, ValType, ValType),

	#[error("self/super/$ are only usable inside objects")]
	CantUseSelfSupOutsideOfObject,

	#[error("static analysis errors: {}", .0.iter().map(|d| d.message.as_str()).collect::<Vec<_>>().join("; "))]
	StaticAnalysisError(Vec<Diagnostic>),
	#[error("no super found")]
	NoSuperFound,

	#[error("for loop can only iterate over arrays")]
	InComprehensionCanOnlyIterateOverArray,
	#[error("(should not be visible) eager compspec evaluation failed due to captured context")]
	EagerCompspecCaptured,

	#[error("array out of bounds: {0} is not within [0,{1})")]
	ArrayBoundsError(f64, u32),
	#[error("string out of bounds: {0} is not within [0,{1})")]
	StringBoundsError(f64, u32),

	#[error("assert failed: {}", format_empty_str(.0))]
	AssertionFailed(IStr),

	#[error("type mismatch: expected {expected}, got {2} {0}", expected = .1.iter().map(|e| format!("{e}")).collect::<Vec<_>>().join(", "))]
	TypeMismatch(&'static str, Vec<ValType>, ValType),
	#[error("no such field: {}{}", format_empty_str(.0), format_found(.1, "field"))]
	NoSuchField(IStr, Vec<IStr>),

	#[error("only functions can be called, got {0}")]
	OnlyFunctionsCanBeCalledGot(ValType),
	#[error("parameter {0} is not defined")]
	UnknownFunctionParameter(IStr),
	#[error("argument {0} is already bound")]
	BindingParameterASecondTime(IStr),
	#[error("too many args, function has {0}\nFunction has the following signature: {1}")]
	TooManyArgsFunctionHas(usize, FunctionSignature),
	#[error("function argument is not passed: {0}\nFunction has the following signature: {1}")]
	FunctionParameterNotBoundInCall(ParamName, FunctionSignature),

	#[error("external variable is not defined: {0}")]
	UndefinedExternalVariable(IStr),

	#[error("field name should be string, got {0}")]
	FieldMustBeStringGot(ValType),
	#[error("duplicate field name: {}", format_empty_str(.0))]
	DuplicateFieldName(IStr),

	#[error("attempted to index array with string {}", format_empty_str(.0))]
	AttemptedIndexAnArrayWithString(IStr),
	#[error("{0} index type should be {1}, got {2}")]
	ValueIndexMustBeTypeGot(ValType, ValType, ValType),
	#[error("cant index into {0}")]
	CantIndexInto(ValType),
	#[error("{0} is not indexable")]
	ValueIsNotIndexable(ValType),

	#[error("super can't be used standalone")]
	StandaloneSuper,

	#[error("can't resolve {1} from {0}")]
	ImportFileNotFound(SourcePath, ResolvePathOwned),
	#[error("resolved file not found: {:?}", .0)]
	ResolvedFileNotFound(SourcePath),
	#[error("can't import {0}: is a directory")]
	ImportIsADirectory(SourcePath),
	#[error("imported file is not valid utf-8: {0:?}")]
	ImportBadFileUtf8(SourcePath),
	#[error("import io error: {0}")]
	ImportIo(String),
	#[error("tried to import {1} from {0}, but imports are not supported")]
	ImportNotSupported(SourcePath, ResolvePathOwned),
	#[error("can't import from virtual file")]
	CantImportFromVirtualFile,
	#[error("syntax error: {error}")]
	ImportSyntaxError {
		path: Source,
		error: Box<SyntaxError>,
	},

	#[error("runtime error: {}", format_empty_str(.0))]
	RuntimeError(IStr),
	#[error("stack overflow, try to reduce recursion, or set --max-stack to bigger value")]
	StackOverflow,
	#[error("infinite recursion detected")]
	InfiniteRecursionDetected,
	#[error("tried to index by fractional value")]
	FractionalIndex,
	#[error("attempted to divide by zero")]
	DivisionByZero,

	#[error("string manifest output is not an string")]
	StringManifestOutputIsNotAString,
	#[error("stream manifest output is not an array")]
	StreamManifestOutputIsNotAArray,
	#[error("multi manifest output is not an object")]
	MultiManifestOutputIsNotAObject,

	#[error("cant recurse stream manifest")]
	StreamManifestOutputCannotBeRecursed,
	#[error("stream manifest output cannot consist of raw strings")]
	StreamManifestCannotNestString,

	#[error("{}", format_empty_str(.0))]
	ImportCallbackError(String),
	#[error("invalid unicode codepoint: {0}")]
	InvalidUnicodeCodepointGot(u32),

	#[error("convert num value: {0}")]
	ConvertNumValue(#[from] ConvertNumValueError),

	#[error("format error: {0}")]
	Format(#[from] FormatError),
	#[error("type error: {0}")]
	TypeError(TypeLocError),

	#[cfg(feature = "anyhow-error")]
	#[error(transparent)]
	Other(#[trace(skip)] std::rc::Rc<anyhow::Error>),
}

#[cfg(feature = "anyhow-error")]
impl From<anyhow::Error> for Error {
	fn from(e: anyhow::Error) -> Self {
		Self::new(ErrorKind::Other(std::rc::Rc::new(e)))
	}
}

impl From<ErrorKind> for Error {
	fn from(e: ErrorKind) -> Self {
		Self::new(e)
	}
}
impl From<ConvertNumValueError> for Error {
	fn from(e: ConvertNumValueError) -> Self {
		Self::new(ErrorKind::ConvertNumValue(e))
	}
}

impl From<Infallible> for Error {
	fn from(_value: Infallible) -> Self {
		unreachable!()
	}
}

/// Single stack trace frame
#[derive(Clone, Debug, Trace)]
pub struct StackTraceElement {
	/// Source of this frame
	/// Some frames only act as description, without attached source
	pub location: Option<Span>,
	/// Frame description
	pub desc: String,
}
#[derive(Debug, Clone, Trace)]
pub struct StackTrace(pub Vec<StackTraceElement>);

#[derive(Clone, Trace)]
pub struct Error(Box<(ErrorKind, StackTrace)>);

#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(Error, usize);

impl Error {
	pub fn new(e: ErrorKind) -> Self {
		Self(Box::new((e, StackTrace(vec![]))))
	}

	pub const fn error(&self) -> &ErrorKind {
		&(self.0).0
	}
	pub fn error_mut(&mut self) -> &mut ErrorKind {
		&mut (self.0).0
	}
	pub const fn trace(&self) -> &StackTrace {
		&(self.0).1
	}
	pub fn trace_mut(&mut self) -> &mut StackTrace {
		&mut (self.0).1
	}
}
impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		writeln!(f, "{}", self.0.0)?;
		for el in &self.0.1.0 {
			write!(f, "\t{}", el.desc)?;
			if let Some(loc) = &el.location {
				write!(f, "at {}", loc.0.0.0)?;
				loc.0.map_source_locations(&[loc.1, loc.2]);
			}
			writeln!(f)?;
		}
		Ok(())
	}
}
impl fmt::Debug for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_tuple("LocError").field(&self.0).finish()
	}
}
impl std::error::Error for Error {}

pub trait ErrorSource {
	fn to_location(self) -> Option<Span>;
}
impl<T: Acyclic> ErrorSource for &Spanned<T> {
	fn to_location(self) -> Option<Span> {
		Some(self.span.clone())
	}
}
impl ErrorSource for &Span {
	fn to_location(self) -> Option<Span> {
		Some(self.clone())
	}
}
impl ErrorSource for CallLocation<'_> {
	fn to_location(self) -> Option<Span> {
		self.0.cloned()
	}
}

pub type Result<V, E = Error> = std::result::Result<V, E>;
pub trait ResultExt: Sized {
	#[must_use]
	fn with_description<O: Into<String>>(self, msg: impl FnOnce() -> O) -> Self;
	#[must_use]
	fn description(self, msg: &str) -> Self {
		self.with_description(|| msg)
	}

	#[must_use]
	fn with_description_src<O: Into<String>>(
		self,
		src: impl ErrorSource,
		msg: impl FnOnce() -> O,
	) -> Self;
	#[must_use]
	fn description_src(self, src: impl ErrorSource, msg: &str) -> Self {
		self.with_description_src(src, || msg)
	}
}
impl<T> ResultExt for Result<T, Error> {
	fn with_description<O: Into<String>>(mut self, msg: impl FnOnce() -> O) -> Self {
		if let Err(e) = &mut self {
			let trace = e.trace_mut();
			trace.0.push(StackTraceElement {
				location: None,
				desc: msg().into(),
			});
		}
		self
	}

	fn with_description_src<O: Into<String>>(
		mut self,
		src: impl ErrorSource,
		msg: impl FnOnce() -> O,
	) -> Self {
		if let Err(e) = &mut self {
			let trace = e.trace_mut();
			trace.0.push(StackTraceElement {
				location: src.to_location(),
				desc: msg().into(),
			});
		}
		self
	}
}

#[macro_export]
macro_rules! bail {
	($w:ident$(::$i:ident)*$(($($tt:tt)*))?) => {
		return Err($w$(::$i)*$(($($tt)*))?.into())
	};
	($w:ident$(::$i:ident)*$({$($tt:tt)*})?) => {
		return Err($w$(::$i)*$({$($tt)*})?.into())
	};
	($l:literal$(, $($tt:tt)*)?) => {
		return Err($crate::error::ErrorKind::RuntimeError($crate::jrsonnet_macros::format_istr!($l$(, $($tt)*)?)).into())
	};
}
#[macro_export]
macro_rules! error {
	($w:ident$(::$i:ident)*$(($($tt:tt)*))?) => {
		$crate::error::Error::from($w$(::$i)*$(($($tt)*))?)
	};
	($w:ident$(::$i:ident)*$({$($tt:tt)*})?) => {
		$crate::error::Error::from($w$(::$i)*$({$($tt)*})?)
	};
	($l:literal$(, $($tt:tt)*)?) => {
		<$crate::error::Error as From<$crate::error::ErrorKind>>::from($crate::error::ErrorKind::RuntimeError($crate::jrsonnet_macros::format_istr!($l$(, $($tt)*)?)).into())
	};
}
