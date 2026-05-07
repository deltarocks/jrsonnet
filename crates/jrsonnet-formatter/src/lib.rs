use std::{any::type_name, rc::Rc};

use children::{children_between, trivia_before};
use dprint_core::formatting::{
	ConditionResolver, ConditionResolverContext, LineNumber, PrintItems, PrintOptions,
	condition_helpers::is_multiple_lines,
	condition_resolvers::true_resolver,
	ir_helpers::{new_line_group, with_indent},
};
use hi_doc::{Formatting, SnippetBuilder};
use jrsonnet_lexer::collect_lexed_str_block;
use jrsonnet_rowan_parser::{
	AstNode, AstToken as _, SyntaxToken,
	nodes::{
		Arg, ArgsDesc, Assertion, BinaryOperator, Bind, CompSpec, Destruct, DestructArrayPart,
		DestructRest, Expr, ExprArray, ExprBase, FieldName, ForObjSpec, ForSpec, IfSpec,
		ImportKind, Literal, Member, Name, Number, ObjBody, ObjLocal, ParamsDesc, SliceDesc,
		SourceFile, Stmt, Suffix, Text, TextKind, UnaryOperator, Visibility,
	},
};

use crate::{
	children::{Child, EndingComments, trivia_after},
	comments::{CommentLocation, format_comments},
};

mod children;
mod comments;
mod tests;

fn with_indent_eoi(cond: ConditionResolver, o: PrintItems, e: EndingComments) -> PrintItems {
	let end_comments_items = {
		let mut items = PrintItems::new();
		if e.should_start_with_newline {
			p!(&mut items, nl);
		}
		format_comments(&e.trivia, CommentLocation::EndOfItems, &mut items);
		items.into_rc_path()
	};
	let items = new_line_group(pi!(@i; items(o) items(end_comments_items.into()))).into_rc_path();

	let indented = with_indent(pi!(@i; nl items(items.into())));

	pi!(@i; if_else("indented body", cond, items(indented))(str(" ") items(items.into())))
}

pub trait Printable {
	fn print(&self, out: &mut PrintItems);
}

macro_rules! pi {
	(@i; $($t:tt)*) => {{
		#[allow(unused_mut)]
		let mut o = dprint_core::formatting::PrintItems::new();
		pi!(@s; o: $($t)*);
		o
	}};
	(@s; $o:ident: str($e:expr $(,)?) $($t:tt)*) => {{
		$o.push_string($e.to_owned());
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: string($e:expr $(,)?) $($t:tt)*) => {{
		$o.push_string($e);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: nl $($t:tt)*) => {{
		$o.push_signal(dprint_core::formatting::Signal::NewLine);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: sonl $($t:tt)*) => {{
		$o.push_signal(dprint_core::formatting::Signal::SpaceOrNewLine);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: tab $($t:tt)*) => {{
		$o.push_signal(dprint_core::formatting::Signal::Tab);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: >i $($t:tt)*) => {{
		$o.push_signal(dprint_core::formatting::Signal::StartIndent);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: <i $($t:tt)*) => {{
		$o.push_signal(dprint_core::formatting::Signal::FinishIndent);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: >ii $($t:tt)*) => {{
		$o.push_signal(dprint_core::formatting::Signal::StartIgnoringIndent);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: <ii $($t:tt)*) => {{
		$o.push_signal(dprint_core::formatting::Signal::FinishIgnoringIndent);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: info($v:expr) $($t:tt)*) => {{
		$o.push_info($v);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: ln_anchor($v:expr) $($t:tt)*) => {{
		$o.push_anchor(LineNumberAnchor::new($v));
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: if($s:literal, $cond:expr, $($i:tt)*) $($t:tt)*) => {{
		$o.push_condition(dprint_core::formatting::conditions::if_true(
			$s,
			$cond.clone(),
			{
				let mut o = PrintItems::new();
				p!(o, $($i)*);
				o
			},
		));
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: if_else($s:literal, $cond:expr, $($i:tt)*)($($e:tt)+) $($t:tt)*) => {{
		$o.push_condition(dprint_core::formatting::conditions::if_true_or(
			$s,
			$cond.clone(),
			{
				let mut o = PrintItems::new();
				p!(o, $($i)*);
				o
			},
			{
				let mut o = PrintItems::new();
				p!(o, $($e)*);
				o
			},
		));
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: if_not($s:literal, $cond:expr, $($e:tt)*) $($t:tt)*) => {{
		$o.push_condition(dprint_core::formatting::conditions::if_true_or(
			$s,
			$cond.clone(),
			{
				let o = PrintItems::new();
				o
			},
			{
				let mut o = PrintItems::new();
				p!(o, $($e)*);
				o
			},
		));
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: {$expr:expr} $($t:tt)*) => {{
		$expr.print($o);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: items($expr:expr) $($t:tt)*) => {{
		$o.extend($expr);
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: if ($e:expr)($($then:tt)*) $($t:tt)*) => {{
		if $e {
			pi!(@s; $o: $($then)*);
		}
		pi!(@s; $o: $($t)*);
	}};
	(@s; $o:ident: ifelse ($e:expr)($($then:tt)*)($($else:tt)*) $($t:tt)*) => {{
		if $e {
			pi!(@s; $o: $($then)*);
		} else {
			pi!(@s; $o: $($else)*);
		}
		pi!(@s; $o: $($t)*);
	}};
	(@s; $i:ident:) => {}
}
macro_rules! p {
	($o:ident, $($t:tt)*) => {
		pi!(@s; $o: $($t)*)
	};
	(&mut $o:ident, $($t:tt)*) => {
		let om = &mut $o;
		pi!(@s; om: $($t)*)
	};
}
pub(crate) use p;
pub(crate) use pi;

impl<P> Printable for Option<P>
where
	P: Printable,
{
	fn print(&self, out: &mut PrintItems) {
		if let Some(v) = self {
			v.print(out);
		} else {
			p!(
				out,
				string(format!(
					"/*missing {}*/",
					type_name::<P>().replace("jrsonnet_rowan_parser::generated::nodes::", "")
				),)
			);
		}
	}
}

impl Printable for SyntaxToken {
	fn print(&self, out: &mut PrintItems) {
		p!(out, string(self.to_string()));
	}
}

impl Printable for Text {
	fn print(&self, out: &mut PrintItems) {
		if matches!(self.kind(), TextKind::StringBlock) {
			let text = self.text();
			let mut text = collect_lexed_str_block(&text[3..])
				.expect("formatting is not performed on code with parsing errors");

			if text.truncate && text.lines.ends_with(&[""]) {
				text.truncate = false;
				text.lines.pop();
			}

			p!(out, str("|||"));
			if text.truncate {
				p!(out, str("-"));
			}
			p!(out, nl > i);
			for ele in text.lines {
				if ele.is_empty() {
					p!(out, >ii nl <ii);
				} else {
					p!(out, string(ele.to_string()) nl);
				}
			}
			p!(out, <i str("|||"));

			return;
		}
		p!(out, string(format!("{}", self)));
	}
}
impl Printable for Number {
	fn print(&self, out: &mut PrintItems) {
		p!(out, string(format!("{}", self)));
	}
}

impl Printable for Name {
	fn print(&self, out: &mut PrintItems) {
		p!(out, { self.ident_lit() });
	}
}

impl Printable for DestructRest {
	fn print(&self, out: &mut PrintItems) {
		p!(out, str("..."));
		if let Some(name) = self.into() {
			p!(out, { name });
		}
	}
}

impl Printable for Destruct {
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::DestructFull(f) => {
				p!(out, { f.name() });
			}
			Self::DestructSkip(_) => p!(out, str("?")),
			Self::DestructArray(a) => {
				p!(out, str("[") >i nl);
				for el in a.destruct_array_parts() {
					match el {
						DestructArrayPart::DestructArrayElement(e) => {
							p!(out, {e.destruct()} str(",") nl);
						}
						DestructArrayPart::DestructRest(d) => {
							p!(out, {d} str(",") nl);
						}
					}
				}
				p!(out, <i str("]"));
			}
			Self::DestructObject(o) => {
				p!(out, str("{") >i nl);
				for item in o.destruct_object_fields() {
					p!(out, { item.field() });
					if let Some(des) = item.destruct() {
						p!(out, str(": ") {des});
					}
					if let Some(def) = item.expr() {
						p!(out, str(" = ") {def});
					}
					p!(out, str(",") nl);
				}
				if let Some(rest) = o.destruct_rest() {
					p!(out, {rest} nl);
				}
				p!(out, <i str("}"));
			}
		}
	}
}

impl Printable for FieldName {
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::FieldNameFixed(f) => {
				if let Some(id) = f.id() {
					p!(out, { id });
				} else if let Some(str) = f.text() {
					p!(out, { str });
				} else {
					p!(out, str("/*missing FieldName*/"));
				}
			}
			Self::FieldNameDynamic(d) => {
				p!(out, str("[") {d.expr()} str("]"));
			}
		}
	}
}

impl Printable for Visibility {
	fn print(&self, out: &mut PrintItems) {
		p!(out, string(self.to_string()));
	}
}

impl Printable for ObjLocal {
	fn print(&self, out: &mut PrintItems) {
		p!(out, str("local ") {self.bind()});
	}
}

impl Printable for Assertion {
	fn print(&self, out: &mut PrintItems) {
		p!(out, str("assert ") {self.condition()});
		if self.colon_token().is_some() || self.message().is_some() {
			p!(out, str(": ") {self.message()});
		}
	}
}

impl Printable for ParamsDesc {
	fn print(&self, out: &mut PrintItems) {
		p!(out, str("(") >i nl);
		for param in self.params() {
			p!(out, { param.destruct() });
			if param.assign_token().is_some() || param.expr().is_some() {
				p!(out, str(" = ") {param.expr()});
			}
			p!(out, str(",") nl);
		}
		p!(out, <i str(")"));
	}
}
impl Printable for ArgsDesc {
	fn print(&self, out: &mut PrintItems) {
		fn gen_args(children: Vec<Child<Arg>>, multi_line: ConditionResolver) -> PrintItems {
			let mut out = PrintItems::new();

			let mut args = children.into_iter().peekable();
			while let Some(ele) = args.next() {
				if ele.should_start_with_newline {
					p!(out, nl);
				}
				format_comments(&ele.before_trivia, CommentLocation::AboveItem, &mut out);
				let arg = ele.value;
				if arg.name().is_some() || arg.assign_token().is_some() {
					p!(&mut out, {arg.name()} str(" = "));
				}
				p!(&mut out, { arg.expr() });
				let has_more = args.peek().is_some();
				if has_more {
					p!(out, str(","));
				} else {
					p!(out, if("trailing comma", multi_line, str(",")));
				}
				format_comments(&ele.inline_trivia, CommentLocation::ItemInline, &mut out);
				if has_more {
					p!(out, if_else("arg separator", multi_line, nl)(sonl));
				}
			}

			out
		}

		let start = LineNumber::new("args start line");
		let end = LineNumber::new("args end line");
		let multi_line = Rc::new(move |condition_context: &mut ConditionResolverContext| {
			is_multiple_lines(condition_context, start, end)
		});

		let (children, end_comments) = children_between::<Arg>(
			self.syntax().clone(),
			self.l_paren_token().map(Into::into).as_ref(),
			self.r_paren_token().map(Into::into).as_ref(),
			None,
		);

		let args_items = new_line_group(gen_args(children, multi_line.clone())).into_rc_path();
		let args_indented = with_indent(pi!(@i; nl items(args_items.into())));

		p!(out, str("(") info(start));
		p!(out, if_else("args body", multi_line, items(args_indented) nl)(items(args_items.into())));
		if end_comments.should_start_with_newline {
			p!(out, nl);
		}
		format_comments(&end_comments.trivia, CommentLocation::EndOfItems, out);
		p!(out, str(")") info(end));
	}
}
impl Printable for SliceDesc {
	fn print(&self, out: &mut PrintItems) {
		p!(out, str("["));
		if self.from().is_some() {
			p!(out, { self.from() });
		}
		p!(out, str(":"));
		if self.end().is_some() {
			p!(out, { self.end().map(|e| e.expr()) });
		}
		// Keep only one : in case if we don't need step
		if self.step().is_some() {
			p!(out, str(":") {self.step().map(|e|e.expr())});
		}
		p!(out, str("]"));
	}
}

impl Printable for Member {
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::MemberBindStmt(b) => {
				p!(out, { b.obj_local() });
			}
			Self::MemberAssertStmt(ass) => {
				p!(out, { ass.assertion() });
			}
			Self::MemberFieldNormal(n) => {
				p!(out, {n.field_name()} if(n.plus_token().is_some())({n.plus_token()}) {n.visibility()} str(" ") {n.expr()});
			}
			Self::MemberFieldMethod(m) => {
				p!(out, {m.field_name()} {m.params_desc()} {m.visibility()} str(" ") {m.expr()});
			}
		}
	}
}

impl Printable for ObjBody {
	#[allow(clippy::too_many_lines)]
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::ObjBodyComp(l) => {
				fn gen_obj_comp(
					members: Vec<Child<Member>>,
					member_end_comments: EndingComments,
					compspecs: Vec<Child<CompSpec>>,
					multi_line: ConditionResolver,
				) -> PrintItems {
					let mut out = PrintItems::new();
					for mem in members {
						if mem.should_start_with_newline {
							p!(out, nl);
						}
						format_comments(&mem.before_trivia, CommentLocation::AboveItem, &mut out);
						p!(&mut out, { mem.value });
						p!(out, if("trailing comma", multi_line, str(",")));
						format_comments(&mem.inline_trivia, CommentLocation::ItemInline, &mut out);
						p!(out, if_else("member-comp sep", multi_line, nl)(sonl));
					}

					if member_end_comments.should_start_with_newline {
						p!(out, nl);
					}
					format_comments(
						&member_end_comments.trivia,
						CommentLocation::EndOfItems,
						&mut out,
					);

					for mem in compspecs {
						if mem.should_start_with_newline {
							p!(out, nl);
						}
						format_comments(&mem.before_trivia, CommentLocation::AboveItem, &mut out);
						p!(&mut out, { mem.value });
						format_comments(&mem.inline_trivia, CommentLocation::ItemInline, &mut out);
						p!(out, if_else("comp spec sep", multi_line, nl)(sonl));
					}

					out
				}

				let (children, mut end_comments) = children_between::<Member>(
					l.syntax().clone(),
					l.l_brace_token().map(Into::into).as_ref(),
					Some(
						&(l.comp_specs()
							.next()
							.expect("at least one spec is defined")
							.syntax()
							.clone())
						.into(),
					),
					None,
				);
				let trailing_for_comp = end_comments.extract_trailing();

				let (compspecs, comp_end_comments) = children_between::<CompSpec>(
					l.syntax().clone(),
					l.member_comps()
						.last()
						.map(|m| m.syntax().clone())
						.map(Into::into)
						.or_else(|| l.l_brace_token().map(Into::into))
						.as_ref(),
					l.r_brace_token().map(Into::into).as_ref(),
					Some(trailing_for_comp),
				);

				let source_is_multiline = children.iter().any(|c| c.triggers_multiline)
					|| compspecs.iter().any(|c| c.triggers_multiline)
					|| end_comments.should_start_with_newline
					|| comp_end_comments.should_start_with_newline;

				let start = LineNumber::new("obj comp start line");
				let end = LineNumber::new("obj comp end line");
				let multi_line: ConditionResolver = if source_is_multiline {
					true_resolver()
				} else {
					Rc::new(move |ctx: &mut ConditionResolverContext| {
						is_multiple_lines(ctx, start, end)
					})
				};

				let body = new_line_group(gen_obj_comp(
					children,
					end_comments,
					compspecs,
					multi_line.clone(),
				))
				.into_rc_path();

				let body = with_indent_eoi(multi_line, body.into(), comp_end_comments);

				p!(out, str("{") info(start));
				p!(out, items(body));
				p!(out, str("}") info(end));
			}
			Self::ObjBodyMemberList(l) => {
				fn gen_members(
					children: Vec<Child<Member>>,
					multi_line: ConditionResolver,
				) -> PrintItems {
					let mut out = PrintItems::new();
					let mut members = children.into_iter().peekable();
					while let Some(mem) = members.next() {
						if mem.should_start_with_newline {
							p!(out, nl);
						}
						format_comments(&mem.before_trivia, CommentLocation::AboveItem, &mut out);
						p!(&mut out, { mem.value });
						let has_more = members.peek().is_some();
						if has_more {
							p!(out, str(","));
						} else {
							p!(out, if("trailing comma", multi_line, str(",")));
						}
						format_comments(&mem.inline_trivia, CommentLocation::ItemInline, &mut out);
						p!(out, if_else("member separator", multi_line, nl)(sonl));
					}
					out
				}

				let (children, end_comments) = children_between::<Member>(
					l.syntax().clone(),
					l.l_brace_token().map(Into::into).as_ref(),
					l.r_brace_token().map(Into::into).as_ref(),
					None,
				);
				if children.is_empty() && end_comments.is_empty() {
					p!(out, str("{ }"));
					return;
				}

				let source_is_multiline = children.iter().any(|c| c.triggers_multiline)
					|| end_comments.should_start_with_newline;

				let start = LineNumber::new("obj start line");
				let end = LineNumber::new("obj end line");
				let multi_line: ConditionResolver = if source_is_multiline {
					true_resolver()
				} else {
					Rc::new(move |ctx: &mut ConditionResolverContext| {
						is_multiple_lines(ctx, start, end)
					})
				};

				let members_items =
					new_line_group(gen_members(children, multi_line.clone())).into_rc_path();

				let members = with_indent_eoi(multi_line, members_items.into(), end_comments);

				p!(out, str("{") info(start));
				p!(out, items(members));
				p!(out, str("}") info(end));
			}
		}
	}
}
impl Printable for UnaryOperator {
	fn print(&self, out: &mut PrintItems) {
		p!(out, string(self.text().to_string()));
	}
}
impl Printable for BinaryOperator {
	fn print(&self, out: &mut PrintItems) {
		p!(out, string(self.text().to_string()));
	}
}
impl Printable for Bind {
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::BindDestruct(d) => {
				p!(out, {d.into()} str(" = ") {d.value()});
			}
			Self::BindFunction(f) => {
				p!(out, {f.name()} {f.params()} str(" = ") {f.value()});
			}
		}
	}
}
impl Printable for Literal {
	fn print(&self, out: &mut PrintItems) {
		p!(out, string(self.syntax().to_string()));
	}
}
impl Printable for ImportKind {
	fn print(&self, out: &mut PrintItems) {
		p!(out, string(self.syntax().to_string()));
	}
}
impl Printable for ForSpec {
	fn print(&self, out: &mut PrintItems) {
		p!(out, str("for ") {self.bind()} str(" in ") {self.expr()});
	}
}
impl Printable for ForObjSpec {
	fn print(&self, out: &mut PrintItems) {
		p!(out, str("for [") {self.key()} str("]") {self.visibility()} str(" ") {self.value()} str(" in ") {self.expr()});
	}
}
impl Printable for IfSpec {
	fn print(&self, out: &mut PrintItems) {
		p!(out, str("if ") {self.expr()});
	}
}
impl Printable for CompSpec {
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::ForSpec(f) => f.print(out),
			Self::ForObjSpec(f) => f.print(out),
			Self::IfSpec(i) => i.print(out),
		}
	}
}
impl Printable for Expr {
	fn print(&self, out: &mut PrintItems) {
		let (stmts, _ending) = children_between::<Stmt>(
			self.syntax().clone(),
			None,
			self.expr_base()
				.as_ref()
				.map(ExprBase::syntax)
				.cloned()
				.map(Into::into)
				.as_ref(),
			None,
		);
		for stmt in stmts {
			p!(out, { stmt.value });
		}
		p!(out, { self.expr_base() });
		let (suffixes, _ending) = children_between::<Suffix>(
			self.syntax().clone(),
			self.expr_base()
				.as_ref()
				.map(ExprBase::syntax)
				.cloned()
				.map(Into::into)
				.as_ref(),
			None,
			None,
		);
		for suffix in suffixes {
			p!(out, { suffix.value });
		}
	}
}
impl Printable for Suffix {
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::SuffixIndex(i) => {
				if i.question_mark_token().is_some() {
					p!(out, str("?"));
				}
				p!(out, str(".") {i.index()});
			}
			Self::SuffixIndexExpr(e) => {
				if e.question_mark_token().is_some() {
					p!(out, str(".?"));
				}
				p!(out, str("[") {e.index()} str("]"));
			}
			Self::SuffixSlice(d) => {
				p!(out, { d.slice_desc() });
			}
			Self::SuffixApply(a) => {
				p!(out, { a.args_desc() });
			}
		}
	}
}
impl Printable for Stmt {
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::StmtLocal(l) => {
				let (binds, end_comments) = children_between::<Bind>(
					l.syntax().clone(),
					l.local_kw_token().map(Into::into).as_ref(),
					l.semi_token().map(Into::into).as_ref(),
					None,
				);
				if binds.len() == 1 {
					let bind = &binds[0];
					format_comments(&bind.before_trivia, CommentLocation::AboveItem, out);
					p!(out, str("local ") {bind.value});
				// TODO: keep end_comments, child.inline_trivia somehow, force multiple locals formatting in case of presence?
				} else {
					p!(out,str("local") >i nl);
					for bind in binds {
						if bind.should_start_with_newline {
							p!(out, nl);
						}
						format_comments(&bind.before_trivia, CommentLocation::AboveItem, out);
						p!(out, {bind.value} str(","));
						format_comments(&bind.inline_trivia, CommentLocation::ItemInline, out);
						p!(out, nl);
					}
					if end_comments.should_start_with_newline {
						p!(out, nl);
					}
					format_comments(&end_comments.trivia, CommentLocation::EndOfItems, out);
					p!(out,<i);
				}
				p!(out,str(";") nl);
			}
			Self::StmtAssert(a) => {
				p!(out, {a.assertion()} str(";") nl);
			}
		}
	}
}

impl Printable for ExprArray {
	fn print(&self, out: &mut PrintItems) {
		fn gen_elements(children: Vec<Child<Expr>>, multi_line: ConditionResolver) -> PrintItems {
			let mut out = PrintItems::new();
			let mut els = children.into_iter().peekable();
			while let Some(el) = els.next() {
				if el.should_start_with_newline {
					p!(out, nl);
				}
				format_comments(&el.before_trivia, CommentLocation::AboveItem, &mut out);
				p!(&mut out, { el.value });
				let has_more = els.peek().is_some();
				if has_more {
					p!(out, str(","));
				} else {
					p!(out, if("trailing comma", multi_line, str(",")));
				}
				format_comments(&el.inline_trivia, CommentLocation::ItemInline, &mut out);
				p!(out, if_else("element separator", multi_line, nl)(sonl));
			}
			out
		}

		let (children, end_comments) = children_between::<Expr>(
			self.syntax().clone(),
			self.l_brack_token().map(Into::into).as_ref(),
			self.r_brack_token().map(Into::into).as_ref(),
			None,
		);
		if children.is_empty() && end_comments.is_empty() {
			p!(out, str("[ ]"));
			return;
		}

		let source_is_multiline =
			children.iter().any(|c| c.triggers_multiline) || end_comments.should_start_with_newline;

		let start = LineNumber::new("arr start line");
		let end = LineNumber::new("arr end line");
		let multi_line: ConditionResolver = if source_is_multiline {
			true_resolver()
		} else {
			Rc::new(move |ctx: &mut ConditionResolverContext| is_multiple_lines(ctx, start, end))
		};

		let els_items = new_line_group(gen_elements(children, multi_line.clone())).into_rc_path();

		let els = with_indent_eoi(multi_line, els_items.into(), end_comments);

		p!(out, str("[") info(start) items(els) str("]") info(end));
	}
}

impl Printable for ExprBase {
	fn print(&self, out: &mut PrintItems) {
		match self {
			Self::ExprBinary(b) => {
				p!(out, {b.lhs()} str(" ") {b.binary_operator()} str(" ") {b.rhs()});
			}
			Self::ExprUnary(u) => p!(out, {u.unary_operator()} {u.rhs()}),
			// Self::ExprSlice(s) => {
			// 	p!(new: {s.expr()} {s.slice_desc()})
			// }
			// Self::ExprIndex(i) => {
			// 	p!(new: {i.expr()} str(".") {i.index()})
			// }
			// Self::ExprIndexExpr(i) => p!(new: {i.base()} str("[") {i.index()} str("]")),
			// Self::ExprApply(a) => {
			// 	let mut pi = p!(new: {a.expr()} {a.args_desc()});
			// 	if a.tailstrict_kw_token().is_some() {
			// 		p!(out,str(" tailstrict"));
			// 	}
			// 	pi
			// }
			Self::ExprObjExtend(ex) => {
				p!(out, {ex.lhs()} str(" ") {ex.rhs()});
			}
			Self::ExprParened(p) => {
				p!(out, str("(") {p.expr()} str(")"));
			}
			Self::ExprString(s) => p!(out, { s.text() }),
			Self::ExprNumber(n) => p!(out, { n.number() }),
			Self::ExprArray(a) => {
				p!(out, { a });
			}
			Self::ExprObject(obj) => {
				p!(out, { obj.obj_body() });
			}
			Self::ExprArrayComp(arr) => {
				p!(out, str("[") {arr.expr()});
				for spec in arr.comp_specs() {
					p!(out, str(" ") {spec});
				}
				p!(out, str("]"));
			}
			Self::ExprImport(v) => {
				p!(out, {v.import_kind()} str(" ") {v.text()});
			}
			Self::ExprVar(n) => p!(out, { n.name() }),
			// Self::ExprLocal(l) => {
			// }
			Self::ExprIfThenElse(ite) => {
				p!(out, str("if ") {ite.cond()} str(" then ") {ite.then().map(|t| t.expr())});
				if ite.else_kw_token().is_some() || ite.else_().is_some() {
					p!(out, str(" else ") {ite.else_().map(|t| t.expr())});
				}
			}
			Self::ExprFunction(f) => p!(out, str("function") {f.params_desc()} nl {f.expr()}),
			// Self::ExprAssert(a) => p!(new: {a.assertion()} str("; ") {a.expr()}),
			Self::ExprError(e) => p!(out, str("error ") {e.expr()}),
			Self::ExprLiteral(l) => {
				p!(out, { l.literal() });
			}
		}
	}
}

impl Printable for SourceFile {
	fn print(&self, out: &mut PrintItems) {
		let before = trivia_before(
			self.syntax().clone(),
			self.expr()
				.map(|e| e.syntax().clone())
				.map(Into::into)
				.as_ref(),
		);
		let after = trivia_after(
			self.syntax().clone(),
			self.expr()
				.map(|e| e.syntax().clone())
				.map(Into::into)
				.as_ref(),
		);
		format_comments(&before, CommentLocation::AboveItem, out);
		p!(out, {self.expr()} nl);
		format_comments(&after, CommentLocation::EndOfItems, out);
	}
}

pub struct FormatOptions {
	pub indent: u8,
	pub use_tabs: bool,
	pub max_width: u32,
}

impl FormatOptions {
	pub fn new() -> Self {
		Self {
			indent: 4,
			use_tabs: true,
			max_width: 100,
		}
	}
}

impl Default for FormatOptions {
	fn default() -> Self {
		Self::new()
	}
}

#[allow(
	clippy::result_large_err,
	reason = "TODO: there should be an intermediate representation for such reports"
)]
pub fn format(input: &str, opts: &FormatOptions) -> Result<String, SnippetBuilder> {
	let (parsed, errors) = jrsonnet_rowan_parser::parse(input);
	if !errors.is_empty() {
		// Reserve one char for EOF display
		let input = format!("{input} ");
		let mut builder = hi_doc::SnippetBuilder::new(input);
		for error in errors {
			builder
				.error(hi_doc::Text::fragment(
					format!("{:?}", error.error),
					Formatting::default(),
				))
				.range(
					error.range.start().into()
						..=(usize::from(error.range.end()) - 1).max(error.range.start().into()),
				)
				.build();
		}
		// let snippet = builder.build();
		return Err(builder);
		// It is possible to recover from this failure, but the output may be broken, as formatter is free to skip
		// ERROR rowan nodes.
		// Recovery needs to be enabled for LSP, though.
	}
	Ok(dprint_core::formatting::format(
		|| {
			let mut out = PrintItems::new();
			parsed.print(&mut out);
			out
		},
		PrintOptions {
			indent_width: opts.indent,
			max_width: opts.max_width,
			use_tabs: opts.use_tabs,
			new_line_text: "\n",
		},
	))
}
