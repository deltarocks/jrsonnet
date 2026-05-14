use jrsonnet_gcmodule::Acyclic;
use jrsonnet_ir::{
	ArgsDesc, AssertExpr, AssertStmt, BinaryOp, BinaryOpType, BindSpec, CompSpec, Destruct, Expr,
	ExprParam, ExprParams, FieldMember, FieldName, ForSpecData, IStr, IdentityKind, IfElse,
	IfSpecData, ImportKind, IndexPart, Member, NumValue, ObjBody, ObjComp, ObjMembers, Slice,
	SliceDesc, Source, Span, Spanned, TrivialVal, UnaryOpType, Visibility,
};
use jrsonnet_lexer::{
	Lexeme, Lexer, Span as LexSpan, SyntaxKind, T, collect_lexed_str_block, unescape,
};

pub struct ParserSettings {
	pub source: Source,
}

#[derive(Debug, Clone)]
pub struct ParseError {
	pub message: String,
	pub location: Span,
}

impl std::fmt::Display for ParseError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.message)
	}
}

type Result<T> = std::result::Result<T, ParseError>;

struct Parser<'a> {
	lexemes: Vec<Lexeme<'a>>,
	offset: usize,
	source: Source,
}

impl<'a> Parser<'a> {
	fn new(code: &'a str, source: Source) -> Self {
		Self {
			lexemes: Lexer::new(code)
				.filter(|l| {
					!matches!(
						l.kind,
						SyntaxKind::WHITESPACE
							| SyntaxKind::SINGLE_LINE_SLASH_COMMENT
							| SyntaxKind::SINGLE_LINE_HASH_COMMENT
							| SyntaxKind::MULTI_LINE_COMMENT
					)
				})
				.collect(),
			offset: 0,
			source,
		}
	}

	fn peek(&self) -> SyntaxKind {
		if self.at_eof() {
			SyntaxKind::EOF
		} else {
			self.lexemes[self.offset].kind
		}
	}

	fn text(&self) -> &'a str {
		self.lexemes[self.offset].text
	}

	fn at(&self, kind: SyntaxKind) -> bool {
		!self.at_eof() && self.peek() == kind
	}

	#[allow(dead_code)]
	fn nth(&self, n: usize) -> SyntaxKind {
		self.lexemes
			.get(self.offset + n)
			.map_or(SyntaxKind::EOF, |l| l.kind)
	}

	fn eat_any(&mut self) {
		self.offset += 1;
	}

	fn eat_any_spanned(&mut self) -> Span {
		let start = self.span_start();
		self.eat_any();
		let end = self.span_end();
		Span(self.source.clone(), start, end)
	}

	fn at_eof(&self) -> bool {
		self.offset >= self.lexemes.len()
	}

	fn try_eat(&mut self, t: SyntaxKind) -> bool {
		if self.at(t) {
			self.eat_any();
			return true;
		}
		false
	}

	fn current_desc(&self) -> String {
		if self.at_eof() {
			return "end of file".to_owned();
		}
		let kind = self.peek();
		let text = self.text();
		let name = kind.display_name();
		if matches!(kind, SyntaxKind::IDENT | SyntaxKind::FLOAT) {
			format!("{name} \"{text}\"")
		} else {
			name.to_owned()
		}
	}

	fn eat(&mut self, t: SyntaxKind) -> Result<()> {
		if !self.at(t) {
			return Err(self.error(format!(
				"expected {}, got {}",
				t.display_name(),
				self.current_desc(),
			)));
		}
		self.eat_any();
		Ok(())
	}
	fn eat_spanned(&mut self, t: SyntaxKind) -> Result<Span> {
		let start = self.span_start();
		self.eat(t)?;
		let end = self.span_end();
		Ok(Span(self.source.clone(), start, end))
	}

	fn span_start(&self) -> u32 {
		if self.at_eof() {
			if let Some(last) = self.lexemes.last() {
				return last.range.1;
			}
			return 0;
		}
		self.lexemes[self.offset].range.0
	}

	fn span_end(&self) -> u32 {
		self.lexemes[self.offset - 1].range.1
	}

	fn error(&self, message: String) -> ParseError {
		if self.offset == self.lexemes.len() {
			let pos = self.lexemes.last().map_or(0, |v| v.range.1);
			return ParseError {
				location: Span(self.source.clone(), pos, pos),
				message,
			};
		}
		let LexSpan(start, end) = self.lexemes[self.offset].range;
		ParseError {
			location: Span(self.source.clone(), start, end),
			message,
		}
	}
}

fn spanned<T: Acyclic>(
	p: &mut Parser<'_>,
	cb: impl FnOnce(&mut Parser<'_>) -> Result<T>,
) -> Result<Spanned<T>> {
	let start = p.span_start();
	let v = cb(p)?;
	let end = p.span_end();
	Ok(Spanned::new(v, Span(p.source.clone(), start, end)))
}

fn parse_string_content(p: &mut Parser<'_>) -> Result<IStr> {
	let kind = p.peek();
	let text = p.text();
	let s = match kind {
		SyntaxKind::STRING_DOUBLE => {
			let inner = &text[1..text.len() - 1];
			unescape(inner).ok_or_else(|| p.error("invalid string escape".into()))?
		}
		SyntaxKind::STRING_SINGLE => {
			let inner = &text[1..text.len() - 1];
			unescape(inner).ok_or_else(|| p.error("invalid string escape".into()))?
		}
		SyntaxKind::STRING_DOUBLE_VERBATIM => {
			let inner = &text[2..text.len() - 1];
			inner.replace("\"\"", "\"")
		}
		SyntaxKind::STRING_SINGLE_VERBATIM => {
			let inner = &text[2..text.len() - 1];
			inner.replace("''", "'")
		}
		SyntaxKind::STRING_BLOCK => {
			let inner = &text[3..];
			let collected = collect_lexed_str_block(inner)
				.map_err(|_| p.error("invalid string block".into()))?;
			let mut result = String::new();
			for (i, line) in collected.lines.iter().enumerate() {
				if i > 0 {
					result.push('\n');
				}
				result.push_str(line);
			}
			if !collected.truncate {
				result.push('\n');
			}
			result
		}
		_ => return Err(p.error(format!("expected string, got {}", p.current_desc()))),
	};
	p.eat_any();
	Ok(s.into())
}

fn is_string_token(kind: SyntaxKind) -> bool {
	matches!(
		kind,
		SyntaxKind::STRING_DOUBLE
			| SyntaxKind::STRING_SINGLE
			| SyntaxKind::STRING_DOUBLE_VERBATIM
			| SyntaxKind::STRING_SINGLE_VERBATIM
			| SyntaxKind::STRING_BLOCK
	)
}

fn parse_number(p: &mut Parser<'_>) -> Result<NumValue> {
	let text = p.text();
	let n: f64 = text
		.replace('_', "")
		.parse()
		.map_err(|_| p.error(format!("invalid number literal: {text}")))?;

	let v = match NumValue::try_from(n) {
		Ok(v) => v,
		Err(e) => return Err(p.error(format!("invalid number value: {e}"))),
	};

	p.eat_any();

	Ok(v)
}

fn ident(p: &mut Parser<'_>) -> Result<IStr> {
	if !p.at(SyntaxKind::IDENT) {
		return Err(p.error(format!("expected identifier, got {}", p.current_desc())));
	}
	let text = p.text();
	p.eat_any();
	Ok(IStr::from(text))
}

fn assert_stmt(p: &mut Parser<'_>) -> Result<AssertStmt> {
	p.eat(T![assert])?;
	let assertion = spanned(p, expr)?;
	let message = if p.try_eat(T![:]) {
		Some(expr(p)?)
	} else {
		None
	};
	Ok(AssertStmt { assertion, message })
}

fn if_spec_data(p: &mut Parser<'_>) -> Result<IfSpecData> {
	let v = spanned(p, |p| p.eat(T![if]))?;
	let cond = expr(p)?;
	Ok(IfSpecData { span: v.span, cond })
}

fn if_else(p: &mut Parser<'_>) -> Result<IfElse> {
	let cond = if_spec_data(p)?;
	p.eat(T![then])?;
	let cond_then = expr(p)?;
	let cond_else = if p.try_eat(T![else]) {
		Some(expr(p)?)
	} else {
		None
	};
	Ok(IfElse {
		cond,
		cond_then,
		cond_else,
	})
}

fn slice_desc(p: &mut Parser<'_>, start: Option<Spanned<Expr>>) -> Result<SliceDesc> {
	p.eat(T![:])?;
	let end = if !p.at(T![:]) && !p.at(T![']']) {
		Some(spanned(p, expr)?)
	} else {
		None
	};
	let step = if p.try_eat(T![:]) {
		if p.at(T![']']) {
			None
		} else {
			Some(spanned(p, expr)?)
		}
	} else {
		None
	};
	Ok(SliceDesc { start, end, step })
}

fn destruct(p: &mut Parser<'_>) -> Result<Destruct> {
	if p.at(SyntaxKind::IDENT) {
		return Ok(Destruct::Full(spanned(p, ident)?));
	}
	#[cfg(not(feature = "exp-destruct"))]
	return Err(p.error(format!("expected identifier, got {}", p.current_desc())));
	#[cfg(feature = "exp-destruct")]
	{
		if p.try_eat(T![?]) {
			return Ok(Destruct::Skip);
		}
		if p.at(T!['[']) {
			return destruct_array(p);
		}
		if p.at(T!['{']) {
			return destruct_object(p);
		}
		Err(p.error(format!(
			"expected destructure pattern, got {}",
			p.current_desc()
		)))
	}
}

#[cfg(feature = "exp-destruct")]
fn destruct_rest(p: &mut Parser<'_>) -> Result<jrsonnet_ir::DestructRest> {
	p.eat(T![...])?;
	if p.at(SyntaxKind::IDENT) {
		Ok(jrsonnet_ir::DestructRest::Keep(ident(p)?))
	} else {
		Ok(jrsonnet_ir::DestructRest::Drop)
	}
}

#[cfg(feature = "exp-destruct")]
fn destruct_array(p: &mut Parser<'_>) -> Result<Destruct> {
	p.eat(T!['['])?;
	let mut start = Vec::new();
	let mut rest = None;
	let mut end = Vec::new();
	if !p.at(T![']']) {
		loop {
			if p.at(T![...]) {
				rest = Some(destruct_rest(p)?);
				if p.try_eat(T![,]) {
					if !p.at(T![']']) {
						loop {
							end.push(destruct(p)?);
							if !p.try_eat(T![,]) {
								break;
							}
							if p.at(T![']']) {
								break;
							}
						}
					}
				}
				break;
			}
			start.push(destruct(p)?);
			if !p.try_eat(T![,]) {
				break;
			}
			if p.at(T![']']) {
				break;
			}
		}
	}
	p.eat(T![']'])?;
	Ok(Destruct::Array { start, rest, end })
}

#[cfg(feature = "exp-destruct")]
fn destruct_object(p: &mut Parser<'_>) -> Result<Destruct> {
	p.eat(T!['{'])?;
	let mut fields = Vec::new();
	let mut rest = None;
	if !p.at(T!['}']) {
		loop {
			if p.at(T![...]) {
				rest = Some(destruct_rest(p)?);
				p.try_eat(T![,]);
				break;
			}
			let name = ident(p)?;
			let into = if p.try_eat(T![:]) {
				Some(destruct(p)?)
			} else {
				None
			};
			let default = if p.try_eat(T![=]) {
				Some(spanned(p, expr)?)
			} else {
				None
			};
			fields.push((name, into, default));
			if !p.try_eat(T![,]) {
				break;
			}
			if p.at(T!['}']) {
				break;
			}
		}
	}
	p.eat(T!['}'])?;
	Ok(Destruct::Object { fields, rest })
}

fn params(p: &mut Parser<'_>) -> Result<ExprParams> {
	if p.at(T![')']) {
		return Ok(ExprParams::new(Vec::new()));
	}
	let mut result = Vec::new();
	loop {
		let d = destruct(p)?;
		let default = if p.try_eat(T![=]) {
			Some(expr(p)?)
		} else {
			None
		};
		result.push(ExprParam {
			destruct: d,
			default,
		});
		if !p.try_eat(T![,]) {
			break;
		}
		if p.at(T![')']) {
			break;
		}
	}
	Ok(ExprParams::new(result))
}

fn args(p: &mut Parser<'_>) -> Result<ArgsDesc> {
	if p.at(T![')']) {
		return Ok(ArgsDesc::new(Vec::new(), Vec::new(), Vec::new()));
	}
	let mut unnamed = Vec::new();
	let mut names = Vec::new();
	let mut values = Vec::new();
	let mut named_started = false;
	loop {
		let is_named = p.at(SyntaxKind::IDENT) && {
			let next_offset = p.offset + 1;
			next_offset < p.lexemes.len() && p.lexemes[next_offset].kind == T![=]
		};
		if is_named {
			let name: IStr = ident(p)?;
			p.eat(T![=])?;

			names.push(name);
			values.push(expr(p)?);
			named_started = true;
		} else {
			if named_started {
				return Err(p.error("positional argument after named argument".into()));
			}
			unnamed.push(expr(p)?);
		}
		if !p.try_eat(T![,]) {
			break;
		}
		if p.at(T![')']) {
			break;
		}
	}
	Ok(ArgsDesc::new(unnamed, names, values))
}

fn bind(p: &mut Parser<'_>) -> Result<BindSpec> {
	#[cfg(feature = "exp-destruct")]
	{
		if !p.at(SyntaxKind::IDENT) {
			let d = destruct(p)?;
			p.eat(T![=])?;
			return Ok(BindSpec::Field {
				into: d,
				value: expr(p)?,
			});
		}
	}
	let name = spanned(p, ident)?;
	if p.try_eat(T!['(']) {
		let ps = params(p)?;
		p.eat(T![')'])?;
		p.eat(T![=])?;
		Ok(BindSpec::Function {
			name,
			params: ps,
			value: expr(p)?,
		})
	} else {
		p.eat(T![=])?;
		Ok(BindSpec::Field {
			into: Destruct::Full(name),
			value: expr(p)?,
		})
	}
}

fn visibility(p: &mut Parser<'_>) -> Result<Visibility> {
	p.eat(T![:])?;
	if p.try_eat(T![:]) {
		if p.try_eat(T![:]) {
			Ok(Visibility::Unhide)
		} else {
			Ok(Visibility::Hidden)
		}
	} else {
		Ok(Visibility::Normal)
	}
}

fn field_name(p: &mut Parser<'_>) -> Result<FieldName> {
	if p.at(SyntaxKind::IDENT) {
		Ok(FieldName::Fixed(ident(p)?))
	} else if is_string_token(p.peek()) {
		Ok(FieldName::Fixed(parse_string_content(p)?))
	} else if p.at(T!['[']) {
		p.eat(T!['['])?;
		let e = expr(p)?;
		p.eat(T![']'])?;
		Ok(FieldName::Dyn(e))
	} else {
		Err(p.error(format!("expected field name, got {}", p.current_desc())))
	}
}

fn field(p: &mut Parser<'_>) -> Result<FieldMember> {
	let name = spanned(p, field_name)?;

	if p.at(T!['(']) {
		p.eat(T!['('])?;
		let ps = params(p)?;
		p.eat(T![')'])?;
		let vis = visibility(p)?;
		Ok(FieldMember {
			name,
			plus: false,
			params: Some(ps),
			visibility: vis,
			value: expr(p)?,
		})
	} else {
		let plus = p.try_eat(T![+]);
		let vis = visibility(p)?;
		Ok(FieldMember {
			name,
			plus,
			params: None,
			visibility: vis,
			value: expr(p)?,
		})
	}
}

fn member(p: &mut Parser<'_>) -> Result<Member> {
	if p.at(T![local]) {
		p.eat(T![local])?;
		Ok(Member::BindStmt(bind(p)?))
	} else if p.at(T![assert]) {
		Ok(Member::AssertStmt(assert_stmt(p)?))
	} else {
		Ok(Member::Field(field(p)?))
	}
}

fn for_spec(p: &mut Parser<'_>) -> Result<CompSpec> {
	p.eat(T![for])?;
	#[cfg(feature = "exp-object-iteration")]
	if p.at(T!['[']) && p.nth(1) == SyntaxKind::IDENT && p.nth(2) == T![']'] && p.nth(3) == T![:] {
		p.eat(T!['['])?;
		let key = ident(p)?;
		p.eat(T![']'])?;
		let visibility = visibility(p)?;
		let value = destruct(p)?;
		p.eat(T![in])?;
		let over = expr(p)?;
		return Ok(CompSpec::ForObjSpec(jrsonnet_ir::ForObjSpecData {
			key,
			visibility,
			value,
			over,
		}));
	}
	let d = destruct(p)?;
	p.eat(T![in])?;
	let over = expr(p)?;
	Ok(CompSpec::ForSpec(ForSpecData { destruct: d, over }))
}

fn compspecs(p: &mut Parser<'_>) -> Result<Vec<CompSpec>> {
	let mut specs = Vec::new();
	specs.push(for_spec(p)?);
	loop {
		if p.at(T![for]) {
			specs.push(for_spec(p)?);
		} else if p.at(T![if]) {
			let isd = if_spec_data(p)?;
			specs.push(CompSpec::IfSpec(isd));
		} else {
			break;
		}
	}
	Ok(specs)
}

fn objinside(p: &mut Parser<'_>) -> Result<ObjBody> {
	if p.at(T!['}']) {
		return Ok(ObjBody::MemberList(ObjMembers {
			locals: Vec::new(),
			asserts: Vec::new(),
			fields: Vec::new(),
		}));
	}

	let mut members = Vec::new();
	loop {
		members.push(member(p)?);
		if !p.try_eat(T![,]) {
			break;
		}
		if p.at(T!['}']) || p.at(T![for]) {
			break;
		}
	}

	if p.at(T![for]) {
		let specs = compspecs(p)?;
		let mut locals = Vec::new();
		let mut field_member = None;
		for m in members {
			match m {
				Member::Field(f) => {
					if field_member.is_some() {
						return Err(
							p.error("object comprehension can only contain one field".into())
						);
					}
					field_member = Some(f);
				}
				Member::BindStmt(b) => locals.push(b),
				Member::AssertStmt(_) => {
					return Err(p.error("asserts are unsupported in object comprehension".into()));
				}
			}
		}
		Ok(ObjBody::ObjComp(ObjComp {
			locals,
			field: Box::new(
				field_member.ok_or_else(|| p.error("missing object comprehension field".into()))?,
			),
			compspecs: specs,
		}))
	} else {
		let mut locals = Vec::new();
		let mut asserts = Vec::new();
		let mut fields = Vec::new();
		for m in members {
			match m {
				Member::Field(f) => fields.push(f),
				Member::BindStmt(b) => locals.push(b),
				Member::AssertStmt(a) => asserts.push(a),
			}
		}
		Ok(ObjBody::MemberList(ObjMembers {
			locals,
			asserts,
			fields,
		}))
	}
}

#[allow(clippy::too_many_lines)]
fn expr_basic(p: &mut Parser<'_>) -> Result<Expr> {
	match p.peek() {
		T![self] => Ok(Expr::Identity(p.eat_any_spanned(), IdentityKind::This)),
		T![super] => Ok(Expr::Identity(p.eat_any_spanned(), IdentityKind::Super)),
		T!['$'] => Ok(Expr::Identity(p.eat_any_spanned(), IdentityKind::Dollar)),
		T![null] => {
			p.eat_any();
			Ok(Expr::Trivial(TrivialVal::Null))
		}
		T![true] => {
			p.eat_any();
			Ok(Expr::Trivial(TrivialVal::Bool(true)))
		}
		T![false] => {
			p.eat_any();
			Ok(Expr::Trivial(TrivialVal::Bool(false)))
		}

		SyntaxKind::STRING_DOUBLE
		| SyntaxKind::STRING_SINGLE
		| SyntaxKind::STRING_DOUBLE_VERBATIM
		| SyntaxKind::STRING_SINGLE_VERBATIM
		| SyntaxKind::STRING_BLOCK => Ok(Expr::Trivial(TrivialVal::Str(parse_string_content(p)?))),

		SyntaxKind::FLOAT => Ok(Expr::Trivial(TrivialVal::Num(parse_number(p)?))),

		T!['('] => {
			p.eat(T!['('])?;
			let e = expr(p)?;
			p.eat(T![')'])?;
			Ok(e)
		}

		T!['['] => {
			p.eat(T!['['])?;
			if p.at(T![']']) {
				p.eat(T![']'])?;
				return Ok(Expr::Arr(Vec::new()));
			}
			let first = expr(p)?;
			if p.at(T![for]) {
				let specs = compspecs(p)?;
				p.eat(T![']'])?;
				Ok(Expr::ArrComp(Box::new(first), specs))
			} else if p.at(T![,]) && {
				let next = p.offset + 1;
				next < p.lexemes.len() && p.lexemes[next].kind == T![for]
			} {
				p.eat(T![,])?;
				let specs = compspecs(p)?;
				p.eat(T![']'])?;
				Ok(Expr::ArrComp(Box::new(first), specs))
			} else {
				let mut elems = vec![first];
				while p.try_eat(T![,]) {
					if p.at(T![']']) {
						break;
					}
					elems.push(expr(p)?);
				}
				p.eat(T![']'])?;
				Ok(Expr::Arr(elems))
			}
		}

		T!['{'] => {
			p.eat(T!['{'])?;
			let body = objinside(p)?;
			p.eat(T!['}'])?;
			Ok(Expr::Obj(body))
		}

		T![local] => {
			p.eat(T![local])?;
			let mut binds = Vec::new();
			loop {
				if p.at(T![;]) {
					break;
				}
				binds.push(bind(p)?);
				if !p.try_eat(T![,]) {
					break;
				}
			}
			p.eat(T![;])?;
			let body = expr(p)?;
			Ok(Expr::LocalExpr(binds, Box::new(body)))
		}

		T![if] => Ok(Expr::IfElse(Box::new(if_else(p)?))),

		T![function] => {
			let span = p.eat_spanned(T![function])?;
			p.eat(T!['('])?;
			let ps = params(p)?;
			p.eat(T![')'])?;
			let body = expr(p)?;
			Ok(Expr::Function(span, ps, Box::new(body)))
		}

		T![assert] => {
			let a = assert_stmt(p)?;
			p.eat(T![;])?;
			let rest = expr(p)?;
			Ok(Expr::AssertExpr(Box::new(AssertExpr { assert: a, rest })))
		}

		T![error] => {
			let span = spanned(p, |p| p.eat(T![error]))?;
			let e = expr(p)?;
			Ok(Expr::ErrorStmt(span.span, Box::new(e)))
		}

		T![importstr] => {
			let kind = spanned(p, |p| {
				p.eat(T![importstr])?;
				Ok(ImportKind::Str)
			})?;
			let path = expr(p)?;
			Ok(Expr::Import(kind, Box::new(path)))
		}

		T![importbin] => {
			let kind = spanned(p, |p| {
				p.eat(T![importbin])?;
				Ok(ImportKind::Bin)
			})?;
			let path = expr(p)?;
			Ok(Expr::Import(kind, Box::new(path)))
		}

		T![import] => {
			let kind = spanned(p, |p| {
				p.eat(T![import])?;
				Ok(ImportKind::Normal)
			})?;
			let path = expr(p)?;
			Ok(Expr::Import(kind, Box::new(path)))
		}

		SyntaxKind::IDENT => {
			let n = spanned(p, |p| {
				let s: IStr = p.text().into();
				p.eat_any();
				Ok(s)
			})?;
			Ok(Expr::Var(n))
		}

		_ => Err(p.error(format!("unexpected {}", p.current_desc()))),
	}
}

fn flush_index_parts(e: &mut Expr, parts: &mut Vec<IndexPart>) {
	if parts.is_empty() {
		return;
	}
	let old = std::mem::replace(e, Expr::Trivial(TrivialVal::Null));
	*e = Expr::Index {
		indexable: Box::new(old),
		parts: std::mem::take(parts),
	};
}

fn expr_suffix(p: &mut Parser<'_>) -> Result<Expr> {
	let mut e = expr_basic(p)?;
	// Accumulate consecutive index parts (.field, [expr], ?.field, ?.[expr])
	// into a single Expr::Index. This is critical for null-coalesce semantics:
	// a?.b.c needs all parts in one Index so the evaluator can skip .c when .b is null.
	let mut parts: Vec<IndexPart> = Vec::new();

	loop {
		#[cfg(feature = "exp-null-coaelse")]
		if p.at(T![?]) {
			p.eat_any();
			if p.try_eat(T![.]) {
				if p.at(T!['[']) {
					// ?.[expr]
					p.eat(T!['['])?;
					let idx = spanned(p, expr)?;
					p.eat(T![']'])?;
					parts.push(IndexPart {
						span: idx.span,
						value: idx.value,
						null_coaelse: true,
					});
				} else {
					// ?.field
					let id_spanned = spanned(p, |p| Ok(Expr::Trivial(TrivialVal::Str(ident(p)?))))?;
					parts.push(IndexPart {
						span: id_spanned.span,
						value: id_spanned.value,
						null_coaelse: true,
					});
				}
			} else {
				return Err(p.error("expected '.' after '?'".into()));
			}
			continue;
		}

		if p.at(T![.]) {
			p.eat(T![.])?;
			let id_spanned = spanned(p, |p| Ok(Expr::Trivial(TrivialVal::Str(ident(p)?))))?;
			parts.push(IndexPart {
				span: id_spanned.span,
				value: id_spanned.value,
				#[cfg(feature = "exp-null-coaelse")]
				null_coaelse: false,
			});
		} else if p.at(T!['[']) {
			p.eat(T!['['])?;

			if p.at(T![:]) {
				// Slice: flush index parts first, then handle slice
				flush_index_parts(&mut e, &mut parts);
				let slice = slice_desc(p, None)?;
				p.eat(T![']'])?;
				e = Expr::Slice(Box::new(Slice { value: e, slice }));
			} else {
				let idx = spanned(p, expr)?;
				if p.at(T![:]) {
					// Slice with start: flush index parts first
					flush_index_parts(&mut e, &mut parts);
					let slice = slice_desc(p, Some(idx))?;
					p.eat(T![']'])?;
					e = Expr::Slice(Box::new(Slice { value: e, slice }));
				} else {
					// Bracket index: add to parts
					p.eat(T![']'])?;
					parts.push(IndexPart {
						span: idx.span,
						value: idx.value,
						#[cfg(feature = "exp-null-coaelse")]
						null_coaelse: false,
					});
				}
			}
		} else if p.at(T!['(']) {
			flush_index_parts(&mut e, &mut parts);
			let args_spanned = spanned(p, |p| {
				p.eat(T!['('])?;
				let a = args(p)?;
				p.eat(T![')'])?;
				Ok(a)
			})?;
			let tailstrict = p.try_eat(T![tailstrict]);
			e = Expr::Apply(Box::new(e), args_spanned, tailstrict);
		} else if p.at(T!['{']) {
			flush_index_parts(&mut e, &mut parts);
			p.eat(T!['{'])?;
			let body = objinside(p)?;
			p.eat(T!['}'])?;
			e = Expr::ObjExtend(Box::new(e), body);
		} else {
			break;
		}
	}

	flush_index_parts(&mut e, &mut parts);
	Ok(e)
}

fn prefix_binding_power(op: UnaryOpType) -> u8 {
	match op {
		UnaryOpType::Plus | UnaryOpType::Minus | UnaryOpType::Not | UnaryOpType::BitNot => 20,
	}
}

fn infix_binding_power(op: BinaryOpType) -> (u8, u8) {
	match op {
		BinaryOpType::Or => (2, 3),
		#[cfg(feature = "exp-null-coaelse")]
		BinaryOpType::NullCoaelse => (2, 3),
		BinaryOpType::And => (4, 5),
		BinaryOpType::BitOr => (6, 7),
		BinaryOpType::BitXor => (8, 9),
		BinaryOpType::BitAnd => (10, 11),
		BinaryOpType::Eq | BinaryOpType::Neq => (12, 13),
		BinaryOpType::Lt
		| BinaryOpType::Gt
		| BinaryOpType::Lte
		| BinaryOpType::Gte
		| BinaryOpType::In => (14, 15),
		BinaryOpType::Lhs | BinaryOpType::Rhs => (16, 17),
		BinaryOpType::Add | BinaryOpType::Sub => (18, 19),
		BinaryOpType::Mul | BinaryOpType::Div | BinaryOpType::Mod => (20, 21),
	}
}

fn unary_op(kind: SyntaxKind) -> Option<UnaryOpType> {
	match kind {
		T![+] => Some(UnaryOpType::Plus),
		T![-] => Some(UnaryOpType::Minus),
		T![!] => Some(UnaryOpType::Not),
		T![~] => Some(UnaryOpType::BitNot),
		_ => None,
	}
}

fn binary_op(p: &Parser<'_>) -> Option<BinaryOpType> {
	match p.peek() {
		T![||] => Some(BinaryOpType::Or),
		T![&&] => Some(BinaryOpType::And),
		T![|] => Some(BinaryOpType::BitOr),
		T![^] => Some(BinaryOpType::BitXor),
		T![&] => Some(BinaryOpType::BitAnd),
		T![==] => Some(BinaryOpType::Eq),
		T![!=] => Some(BinaryOpType::Neq),
		T![<] => Some(BinaryOpType::Lt),
		T![>] => Some(BinaryOpType::Gt),
		T![<=] => Some(BinaryOpType::Lte),
		T![>=] => Some(BinaryOpType::Gte),
		T![<<] => Some(BinaryOpType::Lhs),
		T![>>] => Some(BinaryOpType::Rhs),
		T![+] => Some(BinaryOpType::Add),
		T![-] => Some(BinaryOpType::Sub),
		T![*] => Some(BinaryOpType::Mul),
		T![/] => Some(BinaryOpType::Div),
		T![%] => Some(BinaryOpType::Mod),
		T![in] => Some(BinaryOpType::In),
		#[cfg(feature = "exp-null-coaelse")]
		T![??] => Some(BinaryOpType::NullCoaelse),
		_ => None,
	}
}

fn expr_bp(p: &mut Parser<'_>, min_bp: u8) -> Result<Expr> {
	let mut lhs = if let Some(op) = unary_op(p.peek()) {
		p.eat_any();
		let rbp = prefix_binding_power(op);
		let rhs = expr_bp(p, rbp)?;
		Expr::UnaryOp(op, Box::new(rhs))
	} else {
		expr_suffix(p)?
	};

	loop {
		if p.at_eof() {
			break;
		}

		let Some(op) = binary_op(p) else {
			break;
		};

		let (lbp, rbp) = infix_binding_power(op);
		if lbp < min_bp {
			break;
		}

		p.eat_any();
		let rhs = expr_bp(p, rbp)?;
		lhs = Expr::BinaryOp(Box::new(BinaryOp { lhs, op, rhs }));
	}

	Ok(lhs)
}

fn expr(p: &mut Parser<'_>) -> Result<Expr> {
	expr_bp(p, 0)
}

pub fn parse(str: &str, settings: &ParserSettings) -> Result<Expr> {
	let mut p = Parser::new(str, settings.source.clone());
	for lexeme in &p.lexemes {
		if let Some(desc) = lexeme.kind.error_description() {
			return Err(ParseError {
				message: desc.to_owned(),
				location: Span(p.source.clone(), lexeme.range.0, lexeme.range.1),
			});
		}
	}
	let e = expr(&mut p)?;
	if !p.at_eof() {
		return Err(p.error(format!("expected end of file, got {}", p.current_desc())));
	}
	Ok(e)
}

pub fn string_to_expr(s: IStr, settings: &ParserSettings) -> Spanned<Expr> {
	let len = u32::try_from(s.len()).expect("code size is limited by 4gb");

	Spanned::new(
		Expr::Trivial(TrivialVal::Str(s)),
		Span(settings.source.clone(), 0, len),
	)
}

#[cfg(test)]
mod tests {
	use insta::assert_snapshot;

	use super::*;

	fn parse_str(input: &str) -> Expr {
		let source = Source::new_virtual("<test>".into(), input.into());
		let settings = ParserSettings { source };
		parse(input, &settings).unwrap()
	}

	#[test]
	#[cfg(not(feature = "exp-null-coaelse"))]
	fn basic_test() {
		let v = parse_str("assert true[false] : false ; true");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn literals() {
		let v = parse_str("[null, true, false, self, super, $]");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn basic_math() {
		let v = parse_str("2+2*2");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn underscore_numbers() {
		let v = parse_str("[1_000, 1_000.000_1, 1_0e1_0]");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn strings() {
		let v = parse_str(r#"["hello", 'world', @"raw""str", @'raw''str']"#);
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn object() {
		let v = parse_str("{a: 1, b:: 2, c::: 3}");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn function_and_call() {
		let v = parse_str("local f(x, y=1) = x + y; f(2, y=3)");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn if_then_else() {
		let v = parse_str("if true then 1 else 2");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn imports() {
		let v = parse_str(r#"[import "a", importstr "b", importbin "c"]"#);
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn array_comp() {
		let v = parse_str("[x for x in arr]");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	#[cfg(not(feature = "exp-null-coaelse"))]
	fn index_and_suffix() {
		let v = parse_str("std.test(2).field[0]");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn obj_extend() {
		let v = parse_str("{} { x: 1 }");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn unary_ops() {
		let v = parse_str("!a && !b");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn error_expr() {
		let v = parse_str("error \"bad\"");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	fn slice() {
		let v = parse_str("[a[1:], a[1::], a[:1:], a[::1]]");
		assert_snapshot!(format!("{v:#?}"));
	}

	#[test]
	#[cfg(not(feature = "exp-null-coaelse"))]
	fn peg_snapshots() {
		use std::fs;

		use insta::glob;
		use jrsonnet_ir::{IStr, Source};

		glob!("../../jrsonnet-peg-parser/src", "tests/*.jsonnet", |path| {
			let input = fs::read_to_string(path).expect("read test file");
			let source = Source::new_virtual("<test>".into(), IStr::empty());
			let settings = ParserSettings { source };
			let v = parse(&input, &settings).unwrap();
			let v = format!("{v:#?}");
			assert_snapshot!(v);
		});
	}
}
