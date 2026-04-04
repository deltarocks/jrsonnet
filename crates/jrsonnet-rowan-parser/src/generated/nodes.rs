//! This is a generated file, please do not edit manually. Changes can be
//! made in codegeneration that lives in `xtask` top-level dir.

#![allow(non_snake_case, clippy::match_like_matches_macro)]
use crate::{
	SyntaxKind::{self, *},
	SyntaxNode, SyntaxToken, T,
	ast::{AstChildren, AstNode, AstToken, support},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile {
	pub(crate) syntax: SyntaxNode,
}
impl SourceFile {
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Expr {
	pub(crate) syntax: SyntaxNode,
}
impl Expr {
	pub fn stmts(&self) -> AstChildren<Stmt> {
		support::children(&self.syntax)
	}
	pub fn expr_base(&self) -> Option<ExprBase> {
		support::children(&self.syntax).next()
	}
	pub fn suffixs(&self) -> AstChildren<Suffix> {
		support::children(&self.syntax)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuffixIndex {
	pub(crate) syntax: SyntaxNode,
}
impl SuffixIndex {
	pub fn question_mark_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![?])
	}
	pub fn dot_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![.])
	}
	pub fn index(&self) -> Option<Name> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name {
	pub(crate) syntax: SyntaxNode,
}
impl Name {
	pub fn ident_lit(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, IDENT)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuffixIndexExpr {
	pub(crate) syntax: SyntaxNode,
}
impl SuffixIndexExpr {
	pub fn question_mark_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![?])
	}
	pub fn dot_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![.])
	}
	pub fn l_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['['])
	}
	pub fn index(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn r_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![']'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuffixSlice {
	pub(crate) syntax: SyntaxNode,
}
impl SuffixSlice {
	pub fn slice_desc(&self) -> Option<SliceDesc> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SliceDesc {
	pub(crate) syntax: SyntaxNode,
}
impl SliceDesc {
	pub fn l_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['['])
	}
	pub fn from(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn colon_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![:])
	}
	pub fn end(&self) -> Option<SliceDescEnd> {
		support::children(&self.syntax).next()
	}
	pub fn step(&self) -> Option<SliceDescStep> {
		support::children(&self.syntax).next()
	}
	pub fn r_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![']'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuffixApply {
	pub(crate) syntax: SyntaxNode,
}
impl SuffixApply {
	pub fn args_desc(&self) -> Option<ArgsDesc> {
		support::children(&self.syntax).next()
	}
	pub fn tailstrict_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![tailstrict])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArgsDesc {
	pub(crate) syntax: SyntaxNode,
}
impl ArgsDesc {
	pub fn l_paren_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['('])
	}
	pub fn args(&self) -> AstChildren<Arg> {
		support::children(&self.syntax)
	}
	pub fn r_paren_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![')'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StmtLocal {
	pub(crate) syntax: SyntaxNode,
}
impl StmtLocal {
	pub fn local_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![local])
	}
	pub fn binds(&self) -> AstChildren<Bind> {
		support::children(&self.syntax)
	}
	pub fn semi_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![;])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StmtAssert {
	pub(crate) syntax: SyntaxNode,
}
impl StmtAssert {
	pub fn assertion(&self) -> Option<Assertion> {
		support::children(&self.syntax).next()
	}
	pub fn semi_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![;])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Assertion {
	pub(crate) syntax: SyntaxNode,
}
impl Assertion {
	pub fn assert_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![assert])
	}
	pub fn condition(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn colon_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![:])
	}
	pub fn message(&self) -> Option<Expr> {
		support::children(&self.syntax).nth(1usize)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprBinary {
	pub(crate) syntax: SyntaxNode,
}
impl ExprBinary {
	pub fn lhs(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn binary_operator(&self) -> Option<BinaryOperator> {
		support::token_child(&self.syntax)
	}
	pub fn rhs(&self) -> Option<Expr> {
		support::children(&self.syntax).nth(1usize)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprUnary {
	pub(crate) syntax: SyntaxNode,
}
impl ExprUnary {
	pub fn unary_operator(&self) -> Option<UnaryOperator> {
		support::token_child(&self.syntax)
	}
	pub fn rhs(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprObjExtend {
	pub(crate) syntax: SyntaxNode,
}
impl ExprObjExtend {
	pub fn lhs(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn rhs(&self) -> Option<Expr> {
		support::children(&self.syntax).nth(1usize)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprParened {
	pub(crate) syntax: SyntaxNode,
}
impl ExprParened {
	pub fn l_paren_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['('])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn r_paren_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![')'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprLiteral {
	pub(crate) syntax: SyntaxNode,
}
impl ExprLiteral {
	pub fn literal(&self) -> Option<Literal> {
		support::token_child(&self.syntax)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprString {
	pub(crate) syntax: SyntaxNode,
}
impl ExprString {
	pub fn text(&self) -> Option<Text> {
		support::token_child(&self.syntax)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprNumber {
	pub(crate) syntax: SyntaxNode,
}
impl ExprNumber {
	pub fn number(&self) -> Option<Number> {
		support::token_child(&self.syntax)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprArray {
	pub(crate) syntax: SyntaxNode,
}
impl ExprArray {
	pub fn l_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['['])
	}
	pub fn exprs(&self) -> AstChildren<Expr> {
		support::children(&self.syntax)
	}
	pub fn r_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![']'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprObject {
	pub(crate) syntax: SyntaxNode,
}
impl ExprObject {
	pub fn obj_body(&self) -> Option<ObjBody> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprArrayComp {
	pub(crate) syntax: SyntaxNode,
}
impl ExprArrayComp {
	pub fn l_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['['])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn comma_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![,])
	}
	pub fn comp_specs(&self) -> AstChildren<CompSpec> {
		support::children(&self.syntax)
	}
	pub fn r_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![']'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprImport {
	pub(crate) syntax: SyntaxNode,
}
impl ExprImport {
	pub fn import_kind(&self) -> Option<ImportKind> {
		support::token_child(&self.syntax)
	}
	pub fn text(&self) -> Option<Text> {
		support::token_child(&self.syntax)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprVar {
	pub(crate) syntax: SyntaxNode,
}
impl ExprVar {
	pub fn name(&self) -> Option<Name> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprIfThenElse {
	pub(crate) syntax: SyntaxNode,
}
impl ExprIfThenElse {
	pub fn if_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![if])
	}
	pub fn cond(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn then_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![then])
	}
	pub fn then(&self) -> Option<TrueExpr> {
		support::children(&self.syntax).next()
	}
	pub fn else_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![else])
	}
	pub fn else_(&self) -> Option<FalseExpr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrueExpr {
	pub(crate) syntax: SyntaxNode,
}
impl TrueExpr {
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FalseExpr {
	pub(crate) syntax: SyntaxNode,
}
impl FalseExpr {
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprFunction {
	pub(crate) syntax: SyntaxNode,
}
impl ExprFunction {
	pub fn function_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![function])
	}
	pub fn l_paren_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['('])
	}
	pub fn params_desc(&self) -> Option<ParamsDesc> {
		support::children(&self.syntax).next()
	}
	pub fn r_paren_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![')'])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamsDesc {
	pub(crate) syntax: SyntaxNode,
}
impl ParamsDesc {
	pub fn l_paren_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['('])
	}
	pub fn params(&self) -> AstChildren<Param> {
		support::children(&self.syntax)
	}
	pub fn r_paren_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![')'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprError {
	pub(crate) syntax: SyntaxNode,
}
impl ExprError {
	pub fn error_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![error])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SliceDescEnd {
	pub(crate) syntax: SyntaxNode,
}
impl SliceDescEnd {
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SliceDescStep {
	pub(crate) syntax: SyntaxNode,
}
impl SliceDescStep {
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Arg {
	pub(crate) syntax: SyntaxNode,
}
impl Arg {
	pub fn name(&self) -> Option<Name> {
		support::children(&self.syntax).next()
	}
	pub fn assign_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![=])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjBodyComp {
	pub(crate) syntax: SyntaxNode,
}
impl ObjBodyComp {
	pub fn l_brace_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['{'])
	}
	pub fn member_comps(&self) -> AstChildren<MemberComp> {
		support::children(&self.syntax)
	}
	pub fn comp_specs(&self) -> AstChildren<CompSpec> {
		support::children(&self.syntax)
	}
	pub fn r_brace_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['}'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjBodyMemberList {
	pub(crate) syntax: SyntaxNode,
}
impl ObjBodyMemberList {
	pub fn l_brace_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['{'])
	}
	pub fn members(&self) -> AstChildren<Member> {
		support::children(&self.syntax)
	}
	pub fn r_brace_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['}'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberBindStmt {
	pub(crate) syntax: SyntaxNode,
}
impl MemberBindStmt {
	pub fn obj_local(&self) -> Option<ObjLocal> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjLocal {
	pub(crate) syntax: SyntaxNode,
}
impl ObjLocal {
	pub fn local_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![local])
	}
	pub fn bind(&self) -> Option<Bind> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberAssertStmt {
	pub(crate) syntax: SyntaxNode,
}
impl MemberAssertStmt {
	pub fn assertion(&self) -> Option<Assertion> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberFieldNormal {
	pub(crate) syntax: SyntaxNode,
}
impl MemberFieldNormal {
	pub fn field_name(&self) -> Option<FieldName> {
		support::children(&self.syntax).next()
	}
	pub fn plus_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![+])
	}
	pub fn visibility(&self) -> Option<Visibility> {
		support::children(&self.syntax).next()
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Visibility {
	pub(crate) syntax: SyntaxNode,
}
impl Visibility {
	pub fn colon_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![:])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemberFieldMethod {
	pub(crate) syntax: SyntaxNode,
}
impl MemberFieldMethod {
	pub fn field_name(&self) -> Option<FieldName> {
		support::children(&self.syntax).next()
	}
	pub fn params_desc(&self) -> Option<ParamsDesc> {
		support::children(&self.syntax).next()
	}
	pub fn visibility(&self) -> Option<Visibility> {
		support::children(&self.syntax).next()
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldNameFixed {
	pub(crate) syntax: SyntaxNode,
}
impl FieldNameFixed {
	pub fn id(&self) -> Option<Name> {
		support::children(&self.syntax).next()
	}
	pub fn text(&self) -> Option<Text> {
		support::token_child(&self.syntax)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldNameDynamic {
	pub(crate) syntax: SyntaxNode,
}
impl FieldNameDynamic {
	pub fn l_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['['])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
	pub fn r_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![']'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForSpec {
	pub(crate) syntax: SyntaxNode,
}
impl ForSpec {
	pub fn for_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![for])
	}
	pub fn bind(&self) -> Option<Destruct> {
		support::children(&self.syntax).next()
	}
	pub fn in_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![in])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IfSpec {
	pub(crate) syntax: SyntaxNode,
}
impl IfSpec {
	pub fn if_kw_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![if])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindDestruct {
	pub(crate) syntax: SyntaxNode,
}
impl BindDestruct {
	pub fn into(&self) -> Option<Destruct> {
		support::children(&self.syntax).next()
	}
	pub fn assign_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![=])
	}
	pub fn value(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindFunction {
	pub(crate) syntax: SyntaxNode,
}
impl BindFunction {
	pub fn name(&self) -> Option<Name> {
		support::children(&self.syntax).next()
	}
	pub fn params(&self) -> Option<ParamsDesc> {
		support::children(&self.syntax).next()
	}
	pub fn assign_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![=])
	}
	pub fn value(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Param {
	pub(crate) syntax: SyntaxNode,
}
impl Param {
	pub fn destruct(&self) -> Option<Destruct> {
		support::children(&self.syntax).next()
	}
	pub fn assign_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![=])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestructFull {
	pub(crate) syntax: SyntaxNode,
}
impl DestructFull {
	pub fn name(&self) -> Option<Name> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestructSkip {
	pub(crate) syntax: SyntaxNode,
}
impl DestructSkip {
	pub fn question_mark_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![?])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestructArray {
	pub(crate) syntax: SyntaxNode,
}
impl DestructArray {
	pub fn l_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['['])
	}
	pub fn destruct_array_parts(&self) -> AstChildren<DestructArrayPart> {
		support::children(&self.syntax)
	}
	pub fn r_brack_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![']'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestructObject {
	pub(crate) syntax: SyntaxNode,
}
impl DestructObject {
	pub fn l_brace_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['{'])
	}
	pub fn destruct_object_fields(&self) -> AstChildren<DestructObjectField> {
		support::children(&self.syntax)
	}
	pub fn destruct_rest(&self) -> Option<DestructRest> {
		support::children(&self.syntax).next()
	}
	pub fn comma_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![,])
	}
	pub fn r_brace_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T!['}'])
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestructObjectField {
	pub(crate) syntax: SyntaxNode,
}
impl DestructObjectField {
	pub fn field(&self) -> Option<Name> {
		support::children(&self.syntax).next()
	}
	pub fn colon_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![:])
	}
	pub fn destruct(&self) -> Option<Destruct> {
		support::children(&self.syntax).next()
	}
	pub fn assign_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![=])
	}
	pub fn expr(&self) -> Option<Expr> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestructRest {
	pub(crate) syntax: SyntaxNode,
}
impl DestructRest {
	pub fn dotdotdot_token(&self) -> Option<SyntaxToken> {
		support::token(&self.syntax, T![...])
	}
	pub fn into(&self) -> Option<Name> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DestructArrayElement {
	pub(crate) syntax: SyntaxNode,
}
impl DestructArrayElement {
	pub fn destruct(&self) -> Option<Destruct> {
		support::children(&self.syntax).next()
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Suffix {
	SuffixIndex(SuffixIndex),
	SuffixIndexExpr(SuffixIndexExpr),
	SuffixSlice(SuffixSlice),
	SuffixApply(SuffixApply),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Bind {
	BindDestruct(BindDestruct),
	BindFunction(BindFunction),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stmt {
	StmtLocal(StmtLocal),
	StmtAssert(StmtAssert),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ObjBody {
	ObjBodyComp(ObjBodyComp),
	ObjBodyMemberList(ObjBodyMemberList),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CompSpec {
	ForSpec(ForSpec),
	IfSpec(IfSpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExprBase {
	ExprBinary(ExprBinary),
	ExprUnary(ExprUnary),
	ExprObjExtend(ExprObjExtend),
	ExprParened(ExprParened),
	ExprString(ExprString),
	ExprNumber(ExprNumber),
	ExprLiteral(ExprLiteral),
	ExprArray(ExprArray),
	ExprObject(ExprObject),
	ExprArrayComp(ExprArrayComp),
	ExprImport(ExprImport),
	ExprVar(ExprVar),
	ExprIfThenElse(ExprIfThenElse),
	ExprFunction(ExprFunction),
	ExprError(ExprError),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MemberComp {
	MemberBindStmt(MemberBindStmt),
	MemberFieldNormal(MemberFieldNormal),
	MemberFieldMethod(MemberFieldMethod),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Member {
	MemberBindStmt(MemberBindStmt),
	MemberAssertStmt(MemberAssertStmt),
	MemberFieldNormal(MemberFieldNormal),
	MemberFieldMethod(MemberFieldMethod),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldName {
	FieldNameFixed(FieldNameFixed),
	FieldNameDynamic(FieldNameDynamic),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Destruct {
	DestructFull(DestructFull),
	DestructSkip(DestructSkip),
	DestructArray(DestructArray),
	DestructObject(DestructObject),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DestructArrayPart {
	DestructArrayElement(DestructArrayElement),
	DestructRest(DestructRest),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinaryOperator {
	syntax: SyntaxToken,
	kind: BinaryOperatorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOperatorKind {
	Or,
	NullCoaelse,
	And,
	BitOr,
	BitXor,
	BitAnd,
	Eq,
	Ne,
	Lt,
	Gt,
	Le,
	Ge,
	InKw,
	Lhs,
	Rhs,
	Plus,
	Minus,
	Mul,
	Div,
	Modulo,
	MetaObjectApply,
	ErrorNoOperator,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnaryOperator {
	syntax: SyntaxToken,
	kind: UnaryOperatorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOperatorKind {
	Minus,
	Not,
	BitNot,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Literal {
	syntax: SyntaxToken,
	kind: LiteralKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiteralKind {
	NullKw,
	TrueKw,
	FalseKw,
	SelfKw,
	Dollar,
	SuperKw,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Text {
	syntax: SyntaxToken,
	kind: TextKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextKind {
	StringDouble,
	ErrorStringDoubleUnterminated,
	StringSingle,
	ErrorStringSingleUnterminated,
	StringDoubleVerbatim,
	ErrorStringDoubleVerbatimUnterminated,
	StringSingleVerbatim,
	ErrorStringSingleVerbatimUnterminated,
	ErrorStringVerbatimMissingQuotes,
	StringBlock,
	ErrorStringBlockUnexpectedEnd,
	ErrorStringBlockMissingNewLine,
	ErrorStringBlockMissingTermination,
	ErrorStringBlockMissingIndent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Number {
	syntax: SyntaxToken,
	kind: NumberKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumberKind {
	Float,
	ErrorFloatJunkAfterPoint,
	ErrorFloatJunkAfterExponent,
	ErrorFloatJunkAfterExponentSign,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportKind {
	syntax: SyntaxToken,
	kind: ImportKindKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportKindKind {
	ImportstrKw,
	ImportbinKw,
	ImportKw,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Trivia {
	syntax: SyntaxToken,
	kind: TriviaKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TriviaKind {
	Whitespace,
	MultiLineComment,
	ErrorCommentTooShort,
	ErrorCommentUnterminated,
	SingleLineHashComment,
	SingleLineSlashComment,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CustomError {
	syntax: SyntaxToken,
	kind: CustomErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CustomErrorKind {
	ErrorMissingToken,
	ErrorUnexpectedToken,
	ErrorCustom,
}
impl AstNode for SourceFile {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == SOURCE_FILE
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for Expr {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for SuffixIndex {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == SUFFIX_INDEX
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for Name {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == NAME
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for SuffixIndexExpr {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == SUFFIX_INDEX_EXPR
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for SuffixSlice {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == SUFFIX_SLICE
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for SliceDesc {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == SLICE_DESC
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for SuffixApply {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == SUFFIX_APPLY
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ArgsDesc {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == ARGS_DESC
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for StmtLocal {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == STMT_LOCAL
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for StmtAssert {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == STMT_ASSERT
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for Assertion {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == ASSERTION
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprBinary {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_BINARY
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprUnary {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_UNARY
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprObjExtend {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_OBJ_EXTEND
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprParened {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_PARENED
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprLiteral {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_LITERAL
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprString {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_STRING
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprNumber {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_NUMBER
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprArray {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_ARRAY
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprObject {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_OBJECT
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprArrayComp {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_ARRAY_COMP
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprImport {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_IMPORT
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprVar {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_VAR
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprIfThenElse {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_IF_THEN_ELSE
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for TrueExpr {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == TRUE_EXPR
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for FalseExpr {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == FALSE_EXPR
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprFunction {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_FUNCTION
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ParamsDesc {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == PARAMS_DESC
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ExprError {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == EXPR_ERROR
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for SliceDescEnd {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == SLICE_DESC_END
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for SliceDescStep {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == SLICE_DESC_STEP
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for Arg {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == ARG
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ObjBodyComp {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == OBJ_BODY_COMP
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ObjBodyMemberList {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == OBJ_BODY_MEMBER_LIST
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for MemberBindStmt {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == MEMBER_BIND_STMT
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ObjLocal {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == OBJ_LOCAL
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for MemberAssertStmt {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == MEMBER_ASSERT_STMT
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for MemberFieldNormal {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == MEMBER_FIELD_NORMAL
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for Visibility {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == VISIBILITY
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for MemberFieldMethod {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == MEMBER_FIELD_METHOD
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for FieldNameFixed {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == FIELD_NAME_FIXED
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for FieldNameDynamic {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == FIELD_NAME_DYNAMIC
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for ForSpec {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == FOR_SPEC
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for IfSpec {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == IF_SPEC
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for BindDestruct {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == BIND_DESTRUCT
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for BindFunction {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == BIND_FUNCTION
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for Param {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == PARAM
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for DestructFull {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == DESTRUCT_FULL
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for DestructSkip {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == DESTRUCT_SKIP
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for DestructArray {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == DESTRUCT_ARRAY
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for DestructObject {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == DESTRUCT_OBJECT
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for DestructObjectField {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == DESTRUCT_OBJECT_FIELD
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for DestructRest {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == DESTRUCT_REST
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl AstNode for DestructArrayElement {
	fn can_cast(kind: SyntaxKind) -> bool {
		kind == DESTRUCT_ARRAY_ELEMENT
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		if Self::can_cast(syntax.kind()) {
			Some(Self { syntax })
		} else {
			None
		}
	}
	fn syntax(&self) -> &SyntaxNode {
		&self.syntax
	}
}
impl From<SuffixIndex> for Suffix {
	fn from(node: SuffixIndex) -> Suffix {
		Suffix::SuffixIndex(node)
	}
}
impl From<SuffixIndexExpr> for Suffix {
	fn from(node: SuffixIndexExpr) -> Suffix {
		Suffix::SuffixIndexExpr(node)
	}
}
impl From<SuffixSlice> for Suffix {
	fn from(node: SuffixSlice) -> Suffix {
		Suffix::SuffixSlice(node)
	}
}
impl From<SuffixApply> for Suffix {
	fn from(node: SuffixApply) -> Suffix {
		Suffix::SuffixApply(node)
	}
}
impl AstNode for Suffix {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			SUFFIX_INDEX | SUFFIX_INDEX_EXPR | SUFFIX_SLICE | SUFFIX_APPLY => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			SUFFIX_INDEX => Suffix::SuffixIndex(SuffixIndex { syntax }),
			SUFFIX_INDEX_EXPR => Suffix::SuffixIndexExpr(SuffixIndexExpr { syntax }),
			SUFFIX_SLICE => Suffix::SuffixSlice(SuffixSlice { syntax }),
			SUFFIX_APPLY => Suffix::SuffixApply(SuffixApply { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			Suffix::SuffixIndex(it) => &it.syntax,
			Suffix::SuffixIndexExpr(it) => &it.syntax,
			Suffix::SuffixSlice(it) => &it.syntax,
			Suffix::SuffixApply(it) => &it.syntax,
		}
	}
}
impl From<BindDestruct> for Bind {
	fn from(node: BindDestruct) -> Bind {
		Bind::BindDestruct(node)
	}
}
impl From<BindFunction> for Bind {
	fn from(node: BindFunction) -> Bind {
		Bind::BindFunction(node)
	}
}
impl AstNode for Bind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			BIND_DESTRUCT | BIND_FUNCTION => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			BIND_DESTRUCT => Bind::BindDestruct(BindDestruct { syntax }),
			BIND_FUNCTION => Bind::BindFunction(BindFunction { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			Bind::BindDestruct(it) => &it.syntax,
			Bind::BindFunction(it) => &it.syntax,
		}
	}
}
impl From<StmtLocal> for Stmt {
	fn from(node: StmtLocal) -> Stmt {
		Stmt::StmtLocal(node)
	}
}
impl From<StmtAssert> for Stmt {
	fn from(node: StmtAssert) -> Stmt {
		Stmt::StmtAssert(node)
	}
}
impl AstNode for Stmt {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			STMT_LOCAL | STMT_ASSERT => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			STMT_LOCAL => Stmt::StmtLocal(StmtLocal { syntax }),
			STMT_ASSERT => Stmt::StmtAssert(StmtAssert { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			Stmt::StmtLocal(it) => &it.syntax,
			Stmt::StmtAssert(it) => &it.syntax,
		}
	}
}
impl From<ObjBodyComp> for ObjBody {
	fn from(node: ObjBodyComp) -> ObjBody {
		ObjBody::ObjBodyComp(node)
	}
}
impl From<ObjBodyMemberList> for ObjBody {
	fn from(node: ObjBodyMemberList) -> ObjBody {
		ObjBody::ObjBodyMemberList(node)
	}
}
impl AstNode for ObjBody {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			OBJ_BODY_COMP | OBJ_BODY_MEMBER_LIST => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			OBJ_BODY_COMP => ObjBody::ObjBodyComp(ObjBodyComp { syntax }),
			OBJ_BODY_MEMBER_LIST => ObjBody::ObjBodyMemberList(ObjBodyMemberList { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			ObjBody::ObjBodyComp(it) => &it.syntax,
			ObjBody::ObjBodyMemberList(it) => &it.syntax,
		}
	}
}
impl From<ForSpec> for CompSpec {
	fn from(node: ForSpec) -> CompSpec {
		CompSpec::ForSpec(node)
	}
}
impl From<IfSpec> for CompSpec {
	fn from(node: IfSpec) -> CompSpec {
		CompSpec::IfSpec(node)
	}
}
impl AstNode for CompSpec {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			FOR_SPEC | IF_SPEC => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			FOR_SPEC => CompSpec::ForSpec(ForSpec { syntax }),
			IF_SPEC => CompSpec::IfSpec(IfSpec { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			CompSpec::ForSpec(it) => &it.syntax,
			CompSpec::IfSpec(it) => &it.syntax,
		}
	}
}
impl From<ExprBinary> for ExprBase {
	fn from(node: ExprBinary) -> ExprBase {
		ExprBase::ExprBinary(node)
	}
}
impl From<ExprUnary> for ExprBase {
	fn from(node: ExprUnary) -> ExprBase {
		ExprBase::ExprUnary(node)
	}
}
impl From<ExprObjExtend> for ExprBase {
	fn from(node: ExprObjExtend) -> ExprBase {
		ExprBase::ExprObjExtend(node)
	}
}
impl From<ExprParened> for ExprBase {
	fn from(node: ExprParened) -> ExprBase {
		ExprBase::ExprParened(node)
	}
}
impl From<ExprString> for ExprBase {
	fn from(node: ExprString) -> ExprBase {
		ExprBase::ExprString(node)
	}
}
impl From<ExprNumber> for ExprBase {
	fn from(node: ExprNumber) -> ExprBase {
		ExprBase::ExprNumber(node)
	}
}
impl From<ExprLiteral> for ExprBase {
	fn from(node: ExprLiteral) -> ExprBase {
		ExprBase::ExprLiteral(node)
	}
}
impl From<ExprArray> for ExprBase {
	fn from(node: ExprArray) -> ExprBase {
		ExprBase::ExprArray(node)
	}
}
impl From<ExprObject> for ExprBase {
	fn from(node: ExprObject) -> ExprBase {
		ExprBase::ExprObject(node)
	}
}
impl From<ExprArrayComp> for ExprBase {
	fn from(node: ExprArrayComp) -> ExprBase {
		ExprBase::ExprArrayComp(node)
	}
}
impl From<ExprImport> for ExprBase {
	fn from(node: ExprImport) -> ExprBase {
		ExprBase::ExprImport(node)
	}
}
impl From<ExprVar> for ExprBase {
	fn from(node: ExprVar) -> ExprBase {
		ExprBase::ExprVar(node)
	}
}
impl From<ExprIfThenElse> for ExprBase {
	fn from(node: ExprIfThenElse) -> ExprBase {
		ExprBase::ExprIfThenElse(node)
	}
}
impl From<ExprFunction> for ExprBase {
	fn from(node: ExprFunction) -> ExprBase {
		ExprBase::ExprFunction(node)
	}
}
impl From<ExprError> for ExprBase {
	fn from(node: ExprError) -> ExprBase {
		ExprBase::ExprError(node)
	}
}
impl AstNode for ExprBase {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			EXPR_BINARY | EXPR_UNARY | EXPR_OBJ_EXTEND | EXPR_PARENED | EXPR_STRING
			| EXPR_NUMBER | EXPR_LITERAL | EXPR_ARRAY | EXPR_OBJECT | EXPR_ARRAY_COMP
			| EXPR_IMPORT | EXPR_VAR | EXPR_IF_THEN_ELSE | EXPR_FUNCTION | EXPR_ERROR => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			EXPR_BINARY => ExprBase::ExprBinary(ExprBinary { syntax }),
			EXPR_UNARY => ExprBase::ExprUnary(ExprUnary { syntax }),
			EXPR_OBJ_EXTEND => ExprBase::ExprObjExtend(ExprObjExtend { syntax }),
			EXPR_PARENED => ExprBase::ExprParened(ExprParened { syntax }),
			EXPR_STRING => ExprBase::ExprString(ExprString { syntax }),
			EXPR_NUMBER => ExprBase::ExprNumber(ExprNumber { syntax }),
			EXPR_LITERAL => ExprBase::ExprLiteral(ExprLiteral { syntax }),
			EXPR_ARRAY => ExprBase::ExprArray(ExprArray { syntax }),
			EXPR_OBJECT => ExprBase::ExprObject(ExprObject { syntax }),
			EXPR_ARRAY_COMP => ExprBase::ExprArrayComp(ExprArrayComp { syntax }),
			EXPR_IMPORT => ExprBase::ExprImport(ExprImport { syntax }),
			EXPR_VAR => ExprBase::ExprVar(ExprVar { syntax }),
			EXPR_IF_THEN_ELSE => ExprBase::ExprIfThenElse(ExprIfThenElse { syntax }),
			EXPR_FUNCTION => ExprBase::ExprFunction(ExprFunction { syntax }),
			EXPR_ERROR => ExprBase::ExprError(ExprError { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			ExprBase::ExprBinary(it) => &it.syntax,
			ExprBase::ExprUnary(it) => &it.syntax,
			ExprBase::ExprObjExtend(it) => &it.syntax,
			ExprBase::ExprParened(it) => &it.syntax,
			ExprBase::ExprString(it) => &it.syntax,
			ExprBase::ExprNumber(it) => &it.syntax,
			ExprBase::ExprLiteral(it) => &it.syntax,
			ExprBase::ExprArray(it) => &it.syntax,
			ExprBase::ExprObject(it) => &it.syntax,
			ExprBase::ExprArrayComp(it) => &it.syntax,
			ExprBase::ExprImport(it) => &it.syntax,
			ExprBase::ExprVar(it) => &it.syntax,
			ExprBase::ExprIfThenElse(it) => &it.syntax,
			ExprBase::ExprFunction(it) => &it.syntax,
			ExprBase::ExprError(it) => &it.syntax,
		}
	}
}
impl From<MemberBindStmt> for MemberComp {
	fn from(node: MemberBindStmt) -> MemberComp {
		MemberComp::MemberBindStmt(node)
	}
}
impl From<MemberFieldNormal> for MemberComp {
	fn from(node: MemberFieldNormal) -> MemberComp {
		MemberComp::MemberFieldNormal(node)
	}
}
impl From<MemberFieldMethod> for MemberComp {
	fn from(node: MemberFieldMethod) -> MemberComp {
		MemberComp::MemberFieldMethod(node)
	}
}
impl AstNode for MemberComp {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			MEMBER_BIND_STMT | MEMBER_FIELD_NORMAL | MEMBER_FIELD_METHOD => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			MEMBER_BIND_STMT => MemberComp::MemberBindStmt(MemberBindStmt { syntax }),
			MEMBER_FIELD_NORMAL => MemberComp::MemberFieldNormal(MemberFieldNormal { syntax }),
			MEMBER_FIELD_METHOD => MemberComp::MemberFieldMethod(MemberFieldMethod { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			MemberComp::MemberBindStmt(it) => &it.syntax,
			MemberComp::MemberFieldNormal(it) => &it.syntax,
			MemberComp::MemberFieldMethod(it) => &it.syntax,
		}
	}
}
impl From<MemberBindStmt> for Member {
	fn from(node: MemberBindStmt) -> Member {
		Member::MemberBindStmt(node)
	}
}
impl From<MemberAssertStmt> for Member {
	fn from(node: MemberAssertStmt) -> Member {
		Member::MemberAssertStmt(node)
	}
}
impl From<MemberFieldNormal> for Member {
	fn from(node: MemberFieldNormal) -> Member {
		Member::MemberFieldNormal(node)
	}
}
impl From<MemberFieldMethod> for Member {
	fn from(node: MemberFieldMethod) -> Member {
		Member::MemberFieldMethod(node)
	}
}
impl AstNode for Member {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			MEMBER_BIND_STMT | MEMBER_ASSERT_STMT | MEMBER_FIELD_NORMAL | MEMBER_FIELD_METHOD => {
				true
			}
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			MEMBER_BIND_STMT => Member::MemberBindStmt(MemberBindStmt { syntax }),
			MEMBER_ASSERT_STMT => Member::MemberAssertStmt(MemberAssertStmt { syntax }),
			MEMBER_FIELD_NORMAL => Member::MemberFieldNormal(MemberFieldNormal { syntax }),
			MEMBER_FIELD_METHOD => Member::MemberFieldMethod(MemberFieldMethod { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			Member::MemberBindStmt(it) => &it.syntax,
			Member::MemberAssertStmt(it) => &it.syntax,
			Member::MemberFieldNormal(it) => &it.syntax,
			Member::MemberFieldMethod(it) => &it.syntax,
		}
	}
}
impl From<FieldNameFixed> for FieldName {
	fn from(node: FieldNameFixed) -> FieldName {
		FieldName::FieldNameFixed(node)
	}
}
impl From<FieldNameDynamic> for FieldName {
	fn from(node: FieldNameDynamic) -> FieldName {
		FieldName::FieldNameDynamic(node)
	}
}
impl AstNode for FieldName {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			FIELD_NAME_FIXED | FIELD_NAME_DYNAMIC => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			FIELD_NAME_FIXED => FieldName::FieldNameFixed(FieldNameFixed { syntax }),
			FIELD_NAME_DYNAMIC => FieldName::FieldNameDynamic(FieldNameDynamic { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			FieldName::FieldNameFixed(it) => &it.syntax,
			FieldName::FieldNameDynamic(it) => &it.syntax,
		}
	}
}
impl From<DestructFull> for Destruct {
	fn from(node: DestructFull) -> Destruct {
		Destruct::DestructFull(node)
	}
}
impl From<DestructSkip> for Destruct {
	fn from(node: DestructSkip) -> Destruct {
		Destruct::DestructSkip(node)
	}
}
impl From<DestructArray> for Destruct {
	fn from(node: DestructArray) -> Destruct {
		Destruct::DestructArray(node)
	}
}
impl From<DestructObject> for Destruct {
	fn from(node: DestructObject) -> Destruct {
		Destruct::DestructObject(node)
	}
}
impl AstNode for Destruct {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			DESTRUCT_FULL | DESTRUCT_SKIP | DESTRUCT_ARRAY | DESTRUCT_OBJECT => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			DESTRUCT_FULL => Destruct::DestructFull(DestructFull { syntax }),
			DESTRUCT_SKIP => Destruct::DestructSkip(DestructSkip { syntax }),
			DESTRUCT_ARRAY => Destruct::DestructArray(DestructArray { syntax }),
			DESTRUCT_OBJECT => Destruct::DestructObject(DestructObject { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			Destruct::DestructFull(it) => &it.syntax,
			Destruct::DestructSkip(it) => &it.syntax,
			Destruct::DestructArray(it) => &it.syntax,
			Destruct::DestructObject(it) => &it.syntax,
		}
	}
}
impl From<DestructArrayElement> for DestructArrayPart {
	fn from(node: DestructArrayElement) -> DestructArrayPart {
		DestructArrayPart::DestructArrayElement(node)
	}
}
impl From<DestructRest> for DestructArrayPart {
	fn from(node: DestructRest) -> DestructArrayPart {
		DestructArrayPart::DestructRest(node)
	}
}
impl AstNode for DestructArrayPart {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			DESTRUCT_ARRAY_ELEMENT | DESTRUCT_REST => true,
			_ => false,
		}
	}
	fn cast(syntax: SyntaxNode) -> Option<Self> {
		let res = match syntax.kind() {
			DESTRUCT_ARRAY_ELEMENT => {
				DestructArrayPart::DestructArrayElement(DestructArrayElement { syntax })
			}
			DESTRUCT_REST => DestructArrayPart::DestructRest(DestructRest { syntax }),
			_ => return None,
		};
		Some(res)
	}
	fn syntax(&self) -> &SyntaxNode {
		match self {
			DestructArrayPart::DestructArrayElement(it) => &it.syntax,
			DestructArrayPart::DestructRest(it) => &it.syntax,
		}
	}
}
impl AstToken for BinaryOperator {
	fn can_cast(kind: SyntaxKind) -> bool {
		BinaryOperatorKind::can_cast(kind)
	}
	fn cast(syntax: SyntaxToken) -> Option<Self> {
		let kind = BinaryOperatorKind::cast(syntax.kind())?;
		Some(BinaryOperator { syntax, kind })
	}
	fn syntax(&self) -> &SyntaxToken {
		&self.syntax
	}
}
impl BinaryOperatorKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			OR | NULL_COAELSE | AND | BIT_OR | BIT_XOR | BIT_AND | EQ | NE | LT | GT | LE | GE
			| IN_KW | LHS | RHS | PLUS | MINUS | MUL | DIV | MODULO | META_OBJECT_APPLY
			| ERROR_NO_OPERATOR => true,
			_ => false,
		}
	}
	pub fn cast(kind: SyntaxKind) -> Option<Self> {
		let res = match kind {
			OR => Self::Or,
			NULL_COAELSE => Self::NullCoaelse,
			AND => Self::And,
			BIT_OR => Self::BitOr,
			BIT_XOR => Self::BitXor,
			BIT_AND => Self::BitAnd,
			EQ => Self::Eq,
			NE => Self::Ne,
			LT => Self::Lt,
			GT => Self::Gt,
			LE => Self::Le,
			GE => Self::Ge,
			IN_KW => Self::InKw,
			LHS => Self::Lhs,
			RHS => Self::Rhs,
			PLUS => Self::Plus,
			MINUS => Self::Minus,
			MUL => Self::Mul,
			DIV => Self::Div,
			MODULO => Self::Modulo,
			META_OBJECT_APPLY => Self::MetaObjectApply,
			ERROR_NO_OPERATOR => Self::ErrorNoOperator,
			_ => return None,
		};
		Some(res)
	}
}
impl BinaryOperator {
	pub fn kind(&self) -> BinaryOperatorKind {
		self.kind
	}
}
impl std::fmt::Display for BinaryOperator {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl AstToken for UnaryOperator {
	fn can_cast(kind: SyntaxKind) -> bool {
		UnaryOperatorKind::can_cast(kind)
	}
	fn cast(syntax: SyntaxToken) -> Option<Self> {
		let kind = UnaryOperatorKind::cast(syntax.kind())?;
		Some(UnaryOperator { syntax, kind })
	}
	fn syntax(&self) -> &SyntaxToken {
		&self.syntax
	}
}
impl UnaryOperatorKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			MINUS | NOT | BIT_NOT => true,
			_ => false,
		}
	}
	pub fn cast(kind: SyntaxKind) -> Option<Self> {
		let res = match kind {
			MINUS => Self::Minus,
			NOT => Self::Not,
			BIT_NOT => Self::BitNot,
			_ => return None,
		};
		Some(res)
	}
}
impl UnaryOperator {
	pub fn kind(&self) -> UnaryOperatorKind {
		self.kind
	}
}
impl std::fmt::Display for UnaryOperator {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl AstToken for Literal {
	fn can_cast(kind: SyntaxKind) -> bool {
		LiteralKind::can_cast(kind)
	}
	fn cast(syntax: SyntaxToken) -> Option<Self> {
		let kind = LiteralKind::cast(syntax.kind())?;
		Some(Literal { syntax, kind })
	}
	fn syntax(&self) -> &SyntaxToken {
		&self.syntax
	}
}
impl LiteralKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			NULL_KW | TRUE_KW | FALSE_KW | SELF_KW | DOLLAR | SUPER_KW => true,
			_ => false,
		}
	}
	pub fn cast(kind: SyntaxKind) -> Option<Self> {
		let res = match kind {
			NULL_KW => Self::NullKw,
			TRUE_KW => Self::TrueKw,
			FALSE_KW => Self::FalseKw,
			SELF_KW => Self::SelfKw,
			DOLLAR => Self::Dollar,
			SUPER_KW => Self::SuperKw,
			_ => return None,
		};
		Some(res)
	}
}
impl Literal {
	pub fn kind(&self) -> LiteralKind {
		self.kind
	}
}
impl std::fmt::Display for Literal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl AstToken for Text {
	fn can_cast(kind: SyntaxKind) -> bool {
		TextKind::can_cast(kind)
	}
	fn cast(syntax: SyntaxToken) -> Option<Self> {
		let kind = TextKind::cast(syntax.kind())?;
		Some(Text { syntax, kind })
	}
	fn syntax(&self) -> &SyntaxToken {
		&self.syntax
	}
}
impl TextKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			STRING_DOUBLE
			| ERROR_STRING_DOUBLE_UNTERMINATED
			| STRING_SINGLE
			| ERROR_STRING_SINGLE_UNTERMINATED
			| STRING_DOUBLE_VERBATIM
			| ERROR_STRING_DOUBLE_VERBATIM_UNTERMINATED
			| STRING_SINGLE_VERBATIM
			| ERROR_STRING_SINGLE_VERBATIM_UNTERMINATED
			| ERROR_STRING_VERBATIM_MISSING_QUOTES
			| STRING_BLOCK
			| ERROR_STRING_BLOCK_UNEXPECTED_END
			| ERROR_STRING_BLOCK_MISSING_NEW_LINE
			| ERROR_STRING_BLOCK_MISSING_TERMINATION
			| ERROR_STRING_BLOCK_MISSING_INDENT => true,
			_ => false,
		}
	}
	pub fn cast(kind: SyntaxKind) -> Option<Self> {
		let res = match kind {
			STRING_DOUBLE => Self::StringDouble,
			ERROR_STRING_DOUBLE_UNTERMINATED => Self::ErrorStringDoubleUnterminated,
			STRING_SINGLE => Self::StringSingle,
			ERROR_STRING_SINGLE_UNTERMINATED => Self::ErrorStringSingleUnterminated,
			STRING_DOUBLE_VERBATIM => Self::StringDoubleVerbatim,
			ERROR_STRING_DOUBLE_VERBATIM_UNTERMINATED => {
				Self::ErrorStringDoubleVerbatimUnterminated
			}
			STRING_SINGLE_VERBATIM => Self::StringSingleVerbatim,
			ERROR_STRING_SINGLE_VERBATIM_UNTERMINATED => {
				Self::ErrorStringSingleVerbatimUnterminated
			}
			ERROR_STRING_VERBATIM_MISSING_QUOTES => Self::ErrorStringVerbatimMissingQuotes,
			STRING_BLOCK => Self::StringBlock,
			ERROR_STRING_BLOCK_UNEXPECTED_END => Self::ErrorStringBlockUnexpectedEnd,
			ERROR_STRING_BLOCK_MISSING_NEW_LINE => Self::ErrorStringBlockMissingNewLine,
			ERROR_STRING_BLOCK_MISSING_TERMINATION => Self::ErrorStringBlockMissingTermination,
			ERROR_STRING_BLOCK_MISSING_INDENT => Self::ErrorStringBlockMissingIndent,
			_ => return None,
		};
		Some(res)
	}
}
impl Text {
	pub fn kind(&self) -> TextKind {
		self.kind
	}
}
impl std::fmt::Display for Text {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl AstToken for Number {
	fn can_cast(kind: SyntaxKind) -> bool {
		NumberKind::can_cast(kind)
	}
	fn cast(syntax: SyntaxToken) -> Option<Self> {
		let kind = NumberKind::cast(syntax.kind())?;
		Some(Number { syntax, kind })
	}
	fn syntax(&self) -> &SyntaxToken {
		&self.syntax
	}
}
impl NumberKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			FLOAT
			| ERROR_FLOAT_JUNK_AFTER_POINT
			| ERROR_FLOAT_JUNK_AFTER_EXPONENT
			| ERROR_FLOAT_JUNK_AFTER_EXPONENT_SIGN => true,
			_ => false,
		}
	}
	pub fn cast(kind: SyntaxKind) -> Option<Self> {
		let res = match kind {
			FLOAT => Self::Float,
			ERROR_FLOAT_JUNK_AFTER_POINT => Self::ErrorFloatJunkAfterPoint,
			ERROR_FLOAT_JUNK_AFTER_EXPONENT => Self::ErrorFloatJunkAfterExponent,
			ERROR_FLOAT_JUNK_AFTER_EXPONENT_SIGN => Self::ErrorFloatJunkAfterExponentSign,
			_ => return None,
		};
		Some(res)
	}
}
impl Number {
	pub fn kind(&self) -> NumberKind {
		self.kind
	}
}
impl std::fmt::Display for Number {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl AstToken for ImportKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		ImportKindKind::can_cast(kind)
	}
	fn cast(syntax: SyntaxToken) -> Option<Self> {
		let kind = ImportKindKind::cast(syntax.kind())?;
		Some(ImportKind { syntax, kind })
	}
	fn syntax(&self) -> &SyntaxToken {
		&self.syntax
	}
}
impl ImportKindKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			IMPORTSTR_KW | IMPORTBIN_KW | IMPORT_KW => true,
			_ => false,
		}
	}
	pub fn cast(kind: SyntaxKind) -> Option<Self> {
		let res = match kind {
			IMPORTSTR_KW => Self::ImportstrKw,
			IMPORTBIN_KW => Self::ImportbinKw,
			IMPORT_KW => Self::ImportKw,
			_ => return None,
		};
		Some(res)
	}
}
impl ImportKind {
	pub fn kind(&self) -> ImportKindKind {
		self.kind
	}
}
impl std::fmt::Display for ImportKind {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl AstToken for Trivia {
	fn can_cast(kind: SyntaxKind) -> bool {
		TriviaKind::can_cast(kind)
	}
	fn cast(syntax: SyntaxToken) -> Option<Self> {
		let kind = TriviaKind::cast(syntax.kind())?;
		Some(Trivia { syntax, kind })
	}
	fn syntax(&self) -> &SyntaxToken {
		&self.syntax
	}
}
impl TriviaKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			WHITESPACE
			| MULTI_LINE_COMMENT
			| ERROR_COMMENT_TOO_SHORT
			| ERROR_COMMENT_UNTERMINATED
			| SINGLE_LINE_HASH_COMMENT
			| SINGLE_LINE_SLASH_COMMENT => true,
			_ => false,
		}
	}
	pub fn cast(kind: SyntaxKind) -> Option<Self> {
		let res = match kind {
			WHITESPACE => Self::Whitespace,
			MULTI_LINE_COMMENT => Self::MultiLineComment,
			ERROR_COMMENT_TOO_SHORT => Self::ErrorCommentTooShort,
			ERROR_COMMENT_UNTERMINATED => Self::ErrorCommentUnterminated,
			SINGLE_LINE_HASH_COMMENT => Self::SingleLineHashComment,
			SINGLE_LINE_SLASH_COMMENT => Self::SingleLineSlashComment,
			_ => return None,
		};
		Some(res)
	}
}
impl Trivia {
	pub fn kind(&self) -> TriviaKind {
		self.kind
	}
}
impl std::fmt::Display for Trivia {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl AstToken for CustomError {
	fn can_cast(kind: SyntaxKind) -> bool {
		CustomErrorKind::can_cast(kind)
	}
	fn cast(syntax: SyntaxToken) -> Option<Self> {
		let kind = CustomErrorKind::cast(syntax.kind())?;
		Some(CustomError { syntax, kind })
	}
	fn syntax(&self) -> &SyntaxToken {
		&self.syntax
	}
}
impl CustomErrorKind {
	fn can_cast(kind: SyntaxKind) -> bool {
		match kind {
			ERROR_MISSING_TOKEN | ERROR_UNEXPECTED_TOKEN | ERROR_CUSTOM => true,
			_ => false,
		}
	}
	pub fn cast(kind: SyntaxKind) -> Option<Self> {
		let res = match kind {
			ERROR_MISSING_TOKEN => Self::ErrorMissingToken,
			ERROR_UNEXPECTED_TOKEN => Self::ErrorUnexpectedToken,
			ERROR_CUSTOM => Self::ErrorCustom,
			_ => return None,
		};
		Some(res)
	}
}
impl CustomError {
	pub fn kind(&self) -> CustomErrorKind {
		self.kind
	}
}
impl std::fmt::Display for CustomError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Suffix {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Bind {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Stmt {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ObjBody {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for CompSpec {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprBase {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for MemberComp {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Member {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for FieldName {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Destruct {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for DestructArrayPart {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for SourceFile {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Expr {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for SuffixIndex {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Name {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for SuffixIndexExpr {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for SuffixSlice {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for SliceDesc {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for SuffixApply {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ArgsDesc {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for StmtLocal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for StmtAssert {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Assertion {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprBinary {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprUnary {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprObjExtend {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprParened {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprLiteral {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprString {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprNumber {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprArray {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprObject {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprArrayComp {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprImport {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprVar {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprIfThenElse {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for TrueExpr {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for FalseExpr {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprFunction {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ParamsDesc {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ExprError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for SliceDescEnd {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for SliceDescStep {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Arg {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ObjBodyComp {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ObjBodyMemberList {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for MemberBindStmt {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ObjLocal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for MemberAssertStmt {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for MemberFieldNormal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Visibility {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for MemberFieldMethod {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for FieldNameFixed {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for FieldNameDynamic {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for ForSpec {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for IfSpec {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for BindDestruct {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for BindFunction {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for Param {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for DestructFull {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for DestructSkip {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for DestructArray {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for DestructObject {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for DestructObjectField {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for DestructRest {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
impl std::fmt::Display for DestructArrayElement {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self.syntax(), f)
	}
}
