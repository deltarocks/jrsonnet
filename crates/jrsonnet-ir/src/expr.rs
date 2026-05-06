use std::{
	fmt::{self, Debug, Display},
	ops::{Deref, RangeInclusive},
};

use jrsonnet_gcmodule::Acyclic;
use jrsonnet_interner::IStr;

use crate::{
	NumValue,
	function::{FunctionSignature, ParamDefault, ParamName, ParamParse},
	source::Source,
};

#[derive(Debug, PartialEq, Acyclic)]
pub enum FieldName {
	/// {fixed: 2}
	Fixed(IStr),
	/// {["dyn"+"amic"]: 3}
	Dyn(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Acyclic)]
#[repr(u8)]
pub enum Visibility {
	/// :
	Normal,
	/// ::
	Hidden,
	/// :::
	Unhide,
}

impl Visibility {
	pub fn is_visible(&self) -> bool {
		matches!(self, Self::Normal | Self::Unhide)
	}
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct AssertStmt {
	pub assertion: Spanned<Expr>,
	pub message: Option<Expr>,
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct FieldMember {
	pub name: Spanned<FieldName>,
	pub plus: bool,
	pub params: Option<ExprParams>,
	pub visibility: Visibility,
	pub value: Expr,
}

#[derive(Debug, PartialEq, Acyclic)]
pub enum Member {
	Field(FieldMember),
	BindStmt(BindSpec),
	AssertStmt(AssertStmt),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Acyclic)]
pub enum UnaryOpType {
	Plus,
	Minus,
	BitNot,
	Not,
}

impl Display for UnaryOpType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		use UnaryOpType::*;
		write!(
			f,
			"{}",
			match self {
				Plus => "+",
				Minus => "-",
				BitNot => "~",
				Not => "!",
			}
		)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Acyclic)]
pub enum BinaryOpType {
	Mul,
	Div,

	/// Implemented as intrinsic, put here for completeness
	Mod,

	Add,
	Sub,

	Lhs,
	Rhs,

	Lt,
	Gt,
	Lte,
	Gte,

	BitAnd,
	BitOr,
	BitXor,

	Eq,
	Neq,

	And,
	Or,
	#[cfg(feature = "exp-null-coaelse")]
	NullCoaelse,

	// Equialent to std.objectHasEx(a, b, true)
	In,
}

impl Display for BinaryOpType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		use BinaryOpType::*;
		write!(
			f,
			"{}",
			match self {
				Mul => "*",
				Div => "/",
				Mod => "%",
				Add => "+",
				Sub => "-",
				Lhs => "<<",
				Rhs => ">>",
				Lt => "<",
				Gt => ">",
				Lte => "<=",
				Gte => ">=",
				BitAnd => "&",
				BitOr => "|",
				BitXor => "^",
				Eq => "==",
				Neq => "!=",
				And => "&&",
				Or => "||",
				In => "in",
				#[cfg(feature = "exp-null-coaelse")]
				NullCoaelse => "??",
			}
		)
	}
}

/// name, default value
#[derive(Debug, PartialEq, Acyclic)]
pub struct ExprParam {
	pub destruct: Destruct,
	pub default: Option<Expr>,
}

/// Defined function parameters
#[derive(Debug, PartialEq, Acyclic)]
pub struct ExprParams {
	pub exprs: Vec<ExprParam>,
	pub signature: FunctionSignature,
	pub(crate) binds_len: usize,
}
impl ExprParams {
	pub fn len(&self) -> usize {
		self.exprs.len()
	}
	pub fn is_empty(&self) -> bool {
		self.exprs.is_empty()
	}

	pub fn binds_len(&self) -> usize {
		self.binds_len
	}
	pub fn new(exprs: Vec<ExprParam>) -> Self {
		Self {
			signature: FunctionSignature::new(
				exprs
					.iter()
					.map(|p| {
						ParamParse::new(
							p.destruct.name(),
							ParamDefault::exists(p.default.is_some()),
						)
					})
					.collect(),
			),
			binds_len: exprs.iter().map(|v| v.destruct.binds_len()).sum(),
			exprs,
		}
	}
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct ArgsDesc {
	pub unnamed: Vec<Expr>,
	pub names: Vec<IStr>,
	pub values: Vec<Expr>,
}
impl ArgsDesc {
	pub fn new(unnamed: Vec<Expr>, names: Vec<IStr>, values: Vec<Expr>) -> Self {
		Self {
			unnamed,
			names,
			values,
		}
	}
}

#[derive(Debug, PartialEq, Eq, Acyclic)]
pub enum DestructRest {
	/// ...rest
	Keep(IStr),
	/// ...
	Drop,
}

#[derive(Debug, PartialEq, Acyclic)]
pub enum Destruct {
	Full(Spanned<IStr>),
	#[cfg(feature = "exp-destruct")]
	Skip,
	#[cfg(feature = "exp-destruct")]
	Array {
		start: Vec<Destruct>,
		rest: Option<DestructRest>,
		end: Vec<Destruct>,
	},
	#[cfg(feature = "exp-destruct")]
	Object {
		#[allow(clippy::type_complexity)]
		fields: Vec<(IStr, Option<Destruct>, Option<Spanned<Expr>>)>,
		rest: Option<DestructRest>,
	},
}
impl Destruct {
	/// Name of destructure, used for function parameter names
	pub fn name(&self) -> ParamName {
		match self {
			Self::Full(name) => ParamName::Named(name.value.clone()),
			#[cfg(feature = "exp-destruct")]
			_ => ParamName::Unnamed,
		}
	}
	pub fn binds_len(&self) -> usize {
		#[cfg(feature = "exp-destruct")]
		fn cap_rest(rest: &Option<DestructRest>) -> usize {
			match rest {
				Some(DestructRest::Keep(_)) => 1,
				Some(DestructRest::Drop) => 0,
				None => 0,
			}
		}
		match self {
			Self::Full(_) => 1,
			#[cfg(feature = "exp-destruct")]
			Self::Skip => 0,
			#[cfg(feature = "exp-destruct")]
			Self::Array { start, rest, end } => {
				start.iter().map(Destruct::binds_len).sum::<usize>()
					+ end.iter().map(Destruct::binds_len).sum::<usize>()
					+ cap_rest(rest)
			}
			#[cfg(feature = "exp-destruct")]
			Self::Object { fields, rest } => {
				let mut out = 0;
				for (_, into, _) in fields {
					match into {
						Some(v) => out += v.binds_len(),
						// Field is destructured to default name
						None => out += 1,
					}
				}
				out + cap_rest(rest)
			}
		}
	}
}

#[derive(Debug, PartialEq, Acyclic)]
pub enum BindSpec {
	Field {
		into: Destruct,
		value: Expr,
	},
	Function {
		name: IStr,
		params: ExprParams,
		value: Expr,
	},
}
impl BindSpec {
	pub fn binds_len(&self) -> usize {
		match self {
			BindSpec::Field { into, .. } => into.binds_len(),
			BindSpec::Function { .. } => 1,
		}
	}
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct IfSpecData {
	pub span: Span,
	pub cond: Expr,
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct ForSpecData {
	pub destruct: Destruct,
	pub over: Expr,
}

#[cfg(feature = "exp-object-iteration")]
#[derive(Debug, PartialEq, Acyclic)]
pub struct ForObjSpecData {
	pub key: IStr,
	pub visibility: Visibility,
	pub value: Destruct,
	pub over: Expr,
}

#[derive(Debug, PartialEq, Acyclic)]
pub enum CompSpec {
	IfSpec(IfSpecData),
	ForSpec(ForSpecData),
	#[cfg(feature = "exp-object-iteration")]
	ForObjSpec(ForObjSpecData),
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct ObjComp {
	pub locals: Vec<BindSpec>,
	pub field: Box<FieldMember>,
	pub compspecs: Vec<CompSpec>,
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct ObjMembers {
	pub locals: Vec<BindSpec>,
	pub asserts: Vec<AssertStmt>,
	pub fields: Vec<FieldMember>,
}

#[derive(Debug, PartialEq, Acyclic)]
pub enum ObjBody {
	MemberList(ObjMembers),
	ObjComp(ObjComp),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Acyclic)]
pub enum LiteralType {
	This,
	Super,
	Dollar,
	Null,
	True,
	False,
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct SliceDesc {
	pub start: Option<Spanned<Expr>>,
	pub end: Option<Spanned<Expr>>,
	pub step: Option<Spanned<Expr>>,
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct AssertExpr {
	pub assert: AssertStmt,
	pub rest: Expr,
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct BinaryOp {
	pub lhs: Expr,
	pub op: BinaryOpType,
	pub rhs: Expr,
}

#[derive(Debug, PartialEq, Acyclic, Clone, Copy)]
pub enum ImportKind {
	Normal,
	Str,
	Bin,
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct IfElse {
	pub cond: IfSpecData,
	pub cond_then: Expr,
	pub cond_else: Option<Expr>,
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct Slice {
	pub value: Expr,
	pub slice: SliceDesc,
}

/// Syntax base
#[derive(Debug, PartialEq, Acyclic)]
pub enum Expr {
	Literal(LiteralType),

	/// String value: "hello"
	Str(IStr),
	/// Number: 1, 2.0, 2e+20
	Num(NumValue),
	/// Variable name: test
	Var(Spanned<IStr>),

	/// Array of expressions: [1, 2, "Hello"]
	Arr(Vec<Expr>),
	/// Array comprehension:
	/// ```jsonnet
	///  ingredients: [
	///    { kind: kind, qty: 4 / 3 }
	///    for kind in [
	///      'Honey Syrup',
	///      'Lemon Juice',
	///      'Farmers Gin',
	///    ]
	///  ],
	/// ```
	ArrComp(Box<Expr>, Vec<CompSpec>),

	/// Object: {a: 2}
	Obj(ObjBody),
	/// Object extension: var1 {b: 2}
	ObjExtend(Box<Expr>, ObjBody),

	/// -2
	UnaryOp(UnaryOpType, Box<Expr>),
	/// 2 - 2
	BinaryOp(Box<BinaryOp>),
	/// assert 2 == 2 : "Math is broken"
	AssertExpr(Box<AssertExpr>),
	/// local a = 2; { b: a }
	LocalExpr(Vec<BindSpec>, Box<Expr>),

	/// import* "hello"
	Import(Spanned<ImportKind>, Box<Expr>),
	/// error "I'm broken"
	ErrorStmt(Span, Box<Expr>),
	/// a(b, c)
	Apply(Box<Expr>, Spanned<ArgsDesc>, bool),
	/// a[b], a.b, a?.b
	Index {
		indexable: Box<Expr>,
		parts: Vec<IndexPart>,
	},
	/// function(x) x
	Function(ExprParams, Box<Expr>),
	/// if true == false then 1 else 2
	IfElse(Box<IfElse>),
	Slice(Box<Slice>),
}

#[derive(Debug, PartialEq, Acyclic)]
pub struct IndexPart {
	pub span: Span,
	pub value: Expr,
	#[cfg(feature = "exp-null-coaelse")]
	pub null_coaelse: bool,
}

/// file, begin offset, end offset
#[derive(Clone, PartialEq, Eq, Acyclic)]
#[repr(C)]
pub struct Span(pub Source, pub u32, pub u32);
impl Span {
	pub fn belongs_to(&self, other: &Span) -> bool {
		other.0 == self.0 && other.1 <= self.1 && other.2 >= self.2
	}
	pub fn range(&self) -> RangeInclusive<usize> {
		let start = self.1;
		let mut end = self.2;
		if end > start {
			// Because it is originally exclusive
			end -= 1;
		}
		start as usize..=end as usize
	}
}

#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(Span, (usize, usize));

impl Debug for Span {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{:?}:{:?}-{:?}", self.0, self.1, self.2)
	}
}

#[derive(Clone, PartialEq, Acyclic)]
pub struct Spanned<T: Acyclic> {
	pub value: T,
	pub span: Span,
}
impl<T: Acyclic> Deref for Spanned<T> {
	type Target = T;
	fn deref(&self) -> &Self::Target {
		&self.value
	}
}
impl<T: Acyclic> Spanned<T> {
	#[inline]
	pub fn new(value: T, span: Span) -> Self {
		Self { value, span }
	}
	pub fn map<U: Acyclic>(self, v: impl FnOnce(T) -> U) -> Spanned<U> {
		Spanned {
			span: self.span,
			value: v(self.value),
		}
	}
	pub fn as_ref<'a>(&'a self) -> Spanned<&'a T>
	where
		&'a T: Acyclic,
	{
		Spanned {
			span: self.span.clone(),
			value: &self.value,
		}
	}
}

impl<T: Debug + Acyclic> Debug for Spanned<T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let expr = &**self;
		if f.alternate() {
			write!(f, "{:#?}", expr)?;
		} else {
			write!(f, "{:?}", expr)?;
		}
		write!(f, " from {:?}", self.span)?;
		Ok(())
	}
}
