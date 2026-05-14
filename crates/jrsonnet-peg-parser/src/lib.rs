use jrsonnet_gcmodule::Acyclic;
use jrsonnet_ir::{
	ArgsDesc, AssertExpr, AssertStmt, BinaryOp, BindSpec, CompSpec, Destruct, DestructRest, Expr,
	ExprParam, ExprParams, FieldMember, FieldName, ForSpecData, IStr, IdentityKind, IfElse,
	IfSpecData, ImportKind, IndexPart, Member, NumValue, ObjBody, ObjComp, ObjMembers, Slice,
	SliceDesc, Source, Span, Spanned, TrivialVal, Visibility,
};
use jrsonnet_lexer::unescape;
use peg::parser;

pub struct ParserSettings {
	pub source: Source,
}

macro_rules! expr_bin {
	($a:ident $op:ident $b:ident) => {
		Expr::BinaryOp(Box::new(BinaryOp {
			lhs: $a,
			op: $op,
			rhs: $b,
		}))
	};
}
macro_rules! expr_un {
	($op:ident $a:ident) => {
		Expr::UnaryOp($op, Box::new($a))
	};
}

parser! {
	pub grammar jsonnet_parser() for str {
		use peg::ParseLiteral;

		rule eof() = quiet!{![_]} / expected!("<eof>")
		rule eol() = "\n" / eof()

		/// Standard C-like comments
		rule comment()
			= "//" (!eol()[_])* eol()
			/ "/*" (!("*/")[_])* "*/"
			/ "#" (!eol()[_])* eol()

		rule single_whitespace() = quiet!{([' ' | '\r' | '\n' | '\t'] / comment())} / expected!("<whitespace>")
		rule _() = quiet!{([' ' | '\r' | '\n' | '\t']+) / comment()}* / expected!("<whitespace>")

		/// For comma-delimited elements
		rule comma() = quiet!{_ "," _} / expected!("<comma>")
		rule alpha() -> char = c:$(['_' | 'a'..='z' | 'A'..='Z']) {c.chars().next().unwrap()}
		rule digit() -> char = d:$(['0'..='9']) {d.chars().next().unwrap()}
		rule end_of_ident() = !['0'..='9' | '_' | 'a'..='z' | 'A'..='Z']
		/// Sequence of digits
		rule uint_str() -> &'input str = a:$(digit()+ ("_" digit()+)*) { a }
		/// Number in scientific notation format
		rule number() -> f64 = quiet!{a:$(uint_str() ("." uint_str())? (['e'|'E'] (s:['+'|'-'])? uint_str())?) {? a.replace('_',"").parse().map_err(|_| "<number>") }} / expected!("<number>")

		/// Reserved word followed by any non-alphanumberic
		rule reserved() = ("assert" / "else" / "error" / "false" / "for" / "function" / "if" / "import" / "importstr" / "importbin" / "in" / "local" / "null" / "tailstrict" / "then" / "self" / "super" / "true") end_of_ident()
		rule id() -> IStr = v:$(quiet!{ !reserved() alpha() (alpha() / digit())*} / expected!("<identifier>")) { v.into() }

		rule keyword(id: &'static str) -> ()
			= #{|input, pos| input.parse_string_literal(pos, id)} end_of_ident()

		pub rule param(s: &ParserSettings) -> ExprParam = destruct:destruct(s) default:(_ "=" _ default:expr(s){default})? { ExprParam { destruct, default } }
		pub rule params(s: &ParserSettings) -> ExprParams
			= params:param(s) ** comma() comma()? { ExprParams::new(params) }
			/ { ExprParams::new(Vec::new()) }

		pub rule arg(s: &ParserSettings) -> (Option<IStr>, Expr)
			= name:(quiet! { (s:id() _ "=" !['='] _ {s})? } / expected!("<argument name>")) expr:expr(s) {(name, expr)}

		pub rule args(s: &ParserSettings) -> ArgsDesc
			= args:arg(s)**comma() comma()? {?
				let unnamed_count = args.iter().take_while(|(n, _)| n.is_none()).count();
				let mut unnamed = Vec::with_capacity(unnamed_count);
				let mut names = Vec::with_capacity(args.len() - unnamed_count);
				let mut values = Vec::with_capacity(args.len() - unnamed_count);
				let mut named_started = false;
				for (name, value) in args {
					if let Some(name) = name {
						named_started = true;
						names.push(name);
						values.push(value);
					} else {
						if named_started {
							return Err("<named argument>")
						}
						unnamed.push(value);
					}
				}
				Ok(ArgsDesc{unnamed, names, values})
			}

		pub rule destruct_rest() -> DestructRest
			= "..." into:(_ into:id() {into})? {into.map_or_else(|| DestructRest::Drop, DestructRest::Keep)}
		pub rule destruct_array(s: &ParserSettings) -> Destruct
			= "[" _ start:destruct(s)**comma() rest:(
				comma() _ rest:destruct_rest()? end:(
					comma() end:destruct(s)**comma() (_ comma())? {end}
					/ comma()? {Vec::new()}
				) {(rest, end)}
				/ comma()? {(None, Vec::new())}
			) _ "]" {?
				#[cfg(feature = "exp-destruct")] return Ok(Destruct::Array {
					start,
					rest: rest.0,
					end: rest.1,
				});
				#[cfg(not(feature = "exp-destruct"))] Err("!!!experimental destructuring was not enabled")
			}
		pub rule destruct_object(s: &ParserSettings) -> Destruct
			= "{" _
				fields:(name:id() into:(_ ":" _ into:destruct(s) {into})? default:(_ "=" _ v:spanned(<expr(s)>, s) {v})? {(name, into, default)})**comma()
				rest:(
					comma() rest:destruct_rest()? {rest}
					/ comma()? {None}
				)
			_ "}" {?
				#[cfg(feature = "exp-destruct")] return Ok(Destruct::Object {
					fields,
					rest,
				});
				#[cfg(not(feature = "exp-destruct"))] Err("!!!experimental destructuring was not enabled")
			}
		pub rule destruct(s: &ParserSettings) -> Destruct
			= v:spanned(<id()>, s) {Destruct::Full(v)}
			/ "?" {?
				#[cfg(feature = "exp-destruct")] return Ok(Destruct::Skip);
				#[cfg(not(feature = "exp-destruct"))] Err("!!!experimental destructuring was not enabled")
			}
			/ arr:destruct_array(s) {arr}
			/ obj:destruct_object(s) {obj}

		pub rule bind(s: &ParserSettings) -> BindSpec
			= into:destruct(s) _ "=" _ value:expr(s) {BindSpec::Field{into, value}}
			/ name:spanned(<id()>, s) _ "(" _ params:params(s) _ ")" _ "=" _ value:expr(s) {BindSpec::Function{name, params, value}}

		pub rule assertion(s: &ParserSettings) -> AssertStmt
			= keyword("assert") _ assertion:spanned(<expr(s)>, s) message:(_ ":" _ e:expr(s) {e})? { AssertStmt{assertion, message} }

		pub rule whole_line() -> &'input str
			= str:$((!['\n'][_])* "\n") {str}
		pub rule string_block() -> String
			= "|||" chomped:"-"? (!['\n']single_whitespace())* "\n"
			empty_lines:$(['\n']*)
			prefix:[' ' | '\t']+ first_line:whole_line()
			lines:("\n" {"\n"} / [' ' | '\t']*<{prefix.len()}> s:whole_line() {s})*
			[' ' | '\t']*<, {prefix.len() - 1}> "|||"
			{
				let mut l = empty_lines.to_owned();
				l.push_str(first_line);
				l.extend(lines);
				if chomped.is_some() {
					debug_assert!(l.ends_with('\n'));
					l.truncate(l.len() - 1);
				}
				l
			}

		rule hex_char()
			= quiet! { ['0'..='9' | 'a'..='f' | 'A'..='F'] } / expected!("<hex char>")

		rule string_char(c: rule<()>)
			= (!['\\']!c()[_])+
			/ "\\\\"
			/ "\\u" hex_char() hex_char() hex_char() hex_char()
			/ "\\x" hex_char() hex_char()
			/ ['\\'] (quiet! { ['b' | 'f' | 'n' | 'r' | 't' | '"' | '\''] } / expected!("<escape character>"))
		pub rule string() -> String
			= ['"'] str:$(string_char(<"\"">)*) ['"'] {? unescape(str).ok_or("<escaped string>")}
			/ ['\''] str:$(string_char(<"\'">)*) ['\''] {? unescape(str).ok_or("<escaped string>")}
			/ quiet!{ "@'" str:$(("''" / (!['\''][_]))*) "'" {str.replace("''", "'")}
			/ "@\"" str:$(("\"\"" / (!['"'][_]))*) "\"" {str.replace("\"\"", "\"")}
			/ string_block() } / expected!("<string>")

		pub rule field_name(s: &ParserSettings) -> FieldName
			= name:id() {FieldName::Fixed(name)}
			/ name:string() {FieldName::Fixed(name.into())}
			/ "[" _ expr:expr(s) _ "]" {FieldName::Dyn(expr)}
		pub rule visibility() -> Visibility
			= ":::" {Visibility::Unhide}
			/ "::" {Visibility::Hidden}
			/ ":" {Visibility::Normal}
		pub rule field(s: &ParserSettings) -> FieldMember
			= name:spanned(<field_name(s)>, s) _ plus:"+"? _ visibility:visibility() _ value:expr(s) {FieldMember{
				name,
				plus: plus.is_some(),
				params: None,
				visibility,
				value,
			}}
			/ name:spanned(<field_name(s)>, s) _ "(" _ params:params(s) _ ")" _ visibility:visibility() _ value:expr(s) {FieldMember{
				name,
				plus: false,
				params: Some(params),
				visibility,
				value,
			}}
		pub rule obj_local(s: &ParserSettings) -> BindSpec
			= keyword("local") _ bind:bind(s) {bind}
		pub rule member(s: &ParserSettings) -> Member
			= bind:obj_local(s) {Member::BindStmt(bind)}
			/ assertion:assertion(s) {Member::AssertStmt(assertion)}
			/ field:field(s) {Member::Field(field)}
		pub rule objinside(s: &ParserSettings) -> ObjBody
			=  members:(member(s) ** comma()) comma()? _ compspecs:compspecs(s)? {?
				Ok(if let Some(compspecs) = compspecs {
					let mut locals = Vec::new();
					let mut field = None;
					for member in members {
						match member {
							Member::Field(field_member) => if field.replace(field_member).is_some() {
								return Err("<object comprehension can only contain one field>")
							},
							Member::BindStmt(bind_spec) => locals.push(bind_spec),
							Member::AssertStmt(assert_stmt) => return Err("<asserts are unsupported in object comprehension>"),
						}
					}
					ObjBody::ObjComp(ObjComp {
						locals,
						field: Box::new(field.ok_or("<missing object comprehension field>")?),
						compspecs
					})
				} else {
					let mut locals = Vec::new();
					let mut asserts = Vec::new();
					let mut fields = Vec::new();
					for member in members {
						match member {
							Member::Field(field_member) => fields.push(field_member),
							Member::BindStmt(bind_spec) => locals.push(bind_spec),
							Member::AssertStmt(assert_stmt) => asserts.push(assert_stmt),
						}
					}
					ObjBody::MemberList(ObjMembers {
						locals,
						asserts,
						fields
					})
				})
			}
		pub rule ifspec(s: &ParserSettings) -> IfSpecData
			= i:spanned(<keyword("if")>, s) _ cond:expr(s) {IfSpecData { span: i.span, cond }}
		pub rule forspec(s: &ParserSettings) -> ForSpecData
			= keyword("for") _ destruct:destruct(s) _ keyword("in") _ over:expr(s) { ForSpecData { destruct, over } }
		rule ensure_object_iteration()
			= "" {?
				#[cfg(not(feature = "exp-object-iteration"))] return Err("!!!experimental object iteration was not enabled");
				#[cfg(feature = "exp-object-iteration")] Ok(())
			}
		pub rule forobjspec(s: &ParserSettings) -> CompSpec
			= ensure_object_iteration() keyword("for") _ "[" _ key:id() _ "]" _ vis:visibility() _ value:destruct(s) _ keyword("in") _ over:expr(s) {
				#[cfg(feature = "exp-object-iteration")] return CompSpec::ForObjSpec(jrsonnet_ir::ForObjSpecData { key, visibility: vis, value, over });
				#[cfg(not(feature = "exp-object-iteration"))] unreachable!("ensure_object_iteration will fail if feature is not enabled")
			}
		rule compspec(s: &ParserSettings) -> CompSpec
			= i:ifspec(s) { CompSpec::IfSpec(i) } / f:forobjspec(s) { f } / f:forspec(s) {CompSpec::ForSpec(f)}
		pub rule compspecs(s: &ParserSettings) -> Vec<CompSpec>
			= specs:compspec(s) ++ _ {?
				if matches!(specs[0], CompSpec::IfSpec(_)) {
					return Err("<first compspec should be for>")
				}
				Ok(specs)
			}
		pub rule local_expr(s: &ParserSettings) -> Expr
			= keyword("local") _ binds:bind(s) ** comma() (_ ",")? _ ";" _ expr:expr(s) { Expr::LocalExpr(binds, Box::new(expr)) }
		pub rule string_expr(s: &ParserSettings) -> Expr
			= s:string() {Expr::Trivial(TrivialVal::Str(s.into()))}
		pub rule obj_expr(s: &ParserSettings) -> Expr
			= "{" _ body:objinside(s) _ "}" {Expr::Obj(body)}
		pub rule array_expr(s: &ParserSettings) -> Expr
			= "[" _ elems:(expr(s) ** comma()) _ comma()? "]" {Expr::Arr(elems)}
		pub rule array_comp_expr(s: &ParserSettings) -> Expr
			= "[" _ expr:expr(s) _ comma()? _ specs:(r: compspecs(s) _ {r}) "]" {
				Expr::ArrComp(Box::new(expr), specs)
			}
		pub rule number_expr(s: &ParserSettings) -> Expr
			= n:number() {? NumValue::new(n).map_or_else(|| Err("!!!numbers are finite"), |n| Ok(Expr::Trivial(TrivialVal::Num(n))))}

		rule spanned<T: Acyclic>(x: rule<T>, s: &ParserSettings) -> Spanned<T>
			= a:position!() n:x() b:position!() { Spanned::new(n, Span(s.source.clone(), codeidx(a), codeidx(b))) }

		pub rule var_expr(s: &ParserSettings) -> Expr
			= n:spanned(<id()>, s) { Expr::Var(n) }
		pub rule id_loc(s: &ParserSettings) -> Spanned<Expr>
			= a:position!() n:id() b:position!() { Spanned::new(Expr::Trivial(TrivialVal::Str(n)), Span(s.source.clone(), codeidx(a), codeidx(b))) }
		pub rule if_then_else_expr(s: &ParserSettings) -> Expr
			= cond:ifspec(s) _ keyword("then") _ cond_then:expr(s) cond_else:(_ keyword("else") _ e:expr(s) {e})? {Expr::IfElse(Box::new(IfElse{
				cond,
				cond_then,
				cond_else,
			}))}

		pub rule literal(s: &ParserSettings) -> Expr
			= keyword("null") { Expr::Trivial(TrivialVal::Null) }
			/ keyword("true") { Expr::Trivial(TrivialVal::Bool(true)) }
			/ keyword("false") { Expr::Trivial(TrivialVal::Bool(false)) }
			/ a:position!() v:(
				keyword("self") {IdentityKind::This}
				/ keyword("$") {IdentityKind::Dollar}
				/ keyword("super") {IdentityKind::Super}
			) b:position!() { Expr::Identity(Span(s.source.clone(), codeidx(a), codeidx(b)), v) }

		rule import_kind() -> ImportKind
			= keyword("importstr") { ImportKind::Str }
			/ keyword("importbin") { ImportKind::Bin }
			/ keyword("import") { ImportKind::Normal }

		pub rule expr_basic(s: &ParserSettings) -> Expr
			= literal(s)

			/ string_expr(s) / number_expr(s)
			/ array_expr(s)
			/ obj_expr(s)
			/ array_expr(s)
			/ array_comp_expr(s)

			/ kind:spanned(<import_kind()>, s) _ path:expr(s) {Expr::Import(kind, Box::new(path))}

			/ var_expr(s)
			/ local_expr(s)
			/ if_then_else_expr(s)

			/ kw:spanned(<keyword("function")>, s) _ "(" _ params:params(s) _ ")" _ expr:expr(s) {Expr::Function(kw.span, params, Box::new(expr))}
			/ assert:assertion(s) _ ";" _ rest:expr(s) { Expr::AssertExpr(Box::new(AssertExpr{
				assert, rest
			})) }

			/ err_kw:spanned(<keyword("error")>, s) _ expr:expr(s) { Expr::ErrorStmt(err_kw.span, Box::new(expr)) }

		rule slice_part(s: &ParserSettings) -> Option<Spanned<Expr>>
			= _ e:(e:spanned(<expr(s)>, s) _{e})? {e}
		pub rule slice_desc(s: &ParserSettings) -> SliceDesc
			= start:slice_part(s) ":" pair:(end:slice_part(s) step:(":" e:slice_part(s){e})? {(end, step.flatten())})? {
				let (end, step) = if let Some((end, step)) = pair {
					(end, step)
				}else{
					(None, None)
				};

				SliceDesc { start, end, step }
			}

		rule binop(x: rule<()>) -> ()
			= quiet!{ x() } / expected!("<binary op>")
		rule unaryop(x: rule<()>) -> ()
			= quiet!{ x() } / expected!("<unary op>")

		rule ensure_null_coaelse()
			= "" {?
				#[cfg(not(feature = "exp-null-coaelse"))] return Err("!!!experimental null coaelscing was not enabled");
				#[cfg(feature = "exp-null-coaelse")] Ok(())
			}
		use jrsonnet_ir::BinaryOpType::*;
		use jrsonnet_ir::UnaryOpType::*;
		rule expr(s: &ParserSettings) -> Expr
			= precedence! {
				a:(@) _ binop(<"||">) _ b:@ {expr_bin!(a Or b)}
				a:(@) _ binop(<"??">) _ ensure_null_coaelse() b:@ {
					#[cfg(feature = "exp-null-coaelse")] return expr_bin!(a NullCoaelse b);
					unreachable!("ensure_null_coaelse will fail if feature is not enabled")
				}
				--
				a:(@) _ binop(<"&&">) _ b:@ {expr_bin!(a And b)}
				--
				a:(@) _ binop(<"|">) _ b:@ {expr_bin!(a BitOr b)}
				--
				a:@ _ binop(<"^">) _ b:(@) {expr_bin!(a BitXor b)}
				--
				a:(@) _ binop(<"&">) _ b:@ {expr_bin!(a BitAnd b)}
				--
				a:(@) _ binop(<"==">) _ b:@ {expr_bin!(a Eq b)}
				a:(@) _ binop(<"!=">) _ b:@ {expr_bin!(a Neq b)}
				--
				a:(@) _ binop(<"<">) _ b:@ {expr_bin!(a Lt b)}
				a:(@) _ binop(<">">) _ b:@ {expr_bin!(a Gt b)}
				a:(@) _ binop(<"<=">) _ b:@ {expr_bin!(a Lte b)}
				a:(@) _ binop(<">=">) _ b:@ {expr_bin!(a Gte b)}
				a:(@) _ binop(<keyword("in")>) _ b:@ {expr_bin!(a In b)}
				--
				a:(@) _ binop(<"<<">) _ b:@ {expr_bin!(a Lhs b)}
				a:(@) _ binop(<">>">) _ b:@ {expr_bin!(a Rhs b)}
				--
				a:(@) _ binop(<"+">) _ b:@ {expr_bin!(a Add b)}
				a:(@) _ binop(<"-">) _ b:@ {expr_bin!(a Sub b)}
				--
				a:(@) _ binop(<"*">) _ b:@ {expr_bin!(a Mul b)}
				a:(@) _ binop(<"/">) _ b:@ {expr_bin!(a Div b)}
				a:(@) _ binop(<"%">) _ b:@ {expr_bin!(a Mod b)}
				--
						unaryop(<"+">) _ b:@ {expr_un!(Plus b)}
						unaryop(<"-">) _ b:@ {expr_un!(Minus b)}
						unaryop(<"!">) _ b:@ {expr_un!(Not b)}
						unaryop(<"~">) _ b:@ {expr_un!(BitNot b)}
				--
				value:(@) _ "[" _ slice:slice_desc(s) _ "]" {Expr::Slice(Box::new(Slice{value, slice}))}
				indexable:(@) _ parts:index_part(s)+ {Expr::Index{indexable: Box::new(indexable), parts}}
				a:(@) _ args:spanned(<"(" _ a:args(s) _ ")" {a}>, s) ts:(_ keyword("tailstrict"))? {Expr::Apply(Box::new(a), args, ts.is_some())}
				a:(@) _ "{" _ body:objinside(s) _ "}" {Expr::ObjExtend(Box::new(a), body)}
				--
				e:expr_basic(s) {e}
				"(" _ e:expr(s) _ ")" {e}
			}
		pub rule index_part(s: &ParserSettings) -> IndexPart
		= n:("?" _ ensure_null_coaelse())? "." _ value:id_loc(s) {IndexPart {
			span: value.span,
			value: value.value,
			#[cfg(feature = "exp-null-coaelse")]
			null_coaelse: n.is_some(),
		}}
		/ n:("?" _ "." _ ensure_null_coaelse())? value:spanned(<"[" _ v:expr(s) _ "]" {v}>, s) {IndexPart {
			span: value.span,
			value: value.value,
			#[cfg(feature = "exp-null-coaelse")]
			null_coaelse: n.is_some(),
		}}

		pub rule jsonnet(s: &ParserSettings) -> Expr = _ e:expr(s) _ {e}
	}
}

fn codeidx(i: usize) -> u32 {
	u32::try_from(i).expect("code has 4g hard limit")
}

pub type ParseError = peg::error::ParseError<peg::str::LineCol>;
pub fn parse(str: &str, settings: &ParserSettings) -> Result<Expr, ParseError> {
	jsonnet_parser::jsonnet(str, settings)
}
/// Used for importstr values
pub fn string_to_expr(str: IStr, settings: &ParserSettings) -> Spanned<Expr> {
	let len = str.len();
	Spanned::new(
		Expr::Trivial(TrivialVal::Str(str)),
		Span(settings.source.clone(), 0, codeidx(len)),
	)
}

#[cfg(test)]
mod tests {
	#[test]
	#[cfg(not(feature = "exp-null-coaelse"))]
	fn snapshots() {
		use std::fs;

		use insta::{assert_snapshot, glob};
		use jrsonnet_ir::{IStr, Source};

		use crate::{ParserSettings, parse};

		glob!("tests/*.jsonnet", |path| {
			let input = fs::read_to_string(path).expect("read test file");
			let v = parse(
				&input,
				&ParserSettings {
					source: Source::new_virtual("<test>".into(), IStr::empty()),
				},
			)
			.unwrap();
			let v = format!("{v:#?}");
			assert_snapshot!(v);
		});
	}
}
