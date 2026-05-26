#![deny(unused_must_use)]

use event::Sink;
use generated::nodes::{SourceFile, Trivia};
use parser::Parser;
pub use rowan;

mod ast;
mod event;
mod generated;
mod language;
mod lex;
mod marker;
mod parser;
mod precedence;
mod tests;
mod token_set;

pub use ast::{AstChildren, AstNode, AstToken};
pub use generated::{nodes, syntax_kinds::SyntaxKind};
pub use language::*;
pub use parser::LocatedSyntaxError;
pub use token_set::SyntaxKindSet;

pub fn parse(input: &str) -> (SourceFile, Vec<LocatedSyntaxError>) {
	let lexemes = lex::lex(input);
	let kinds = lexemes
		.iter()
		.map(|l| l.kind)
		.filter(|k| !Trivia::can_cast(*k))
		.collect();
	let parser = Parser::new(kinds);
	let events = parser.parse();
	let sink = Sink::new(events, &lexemes);

	let parse = sink.finish();
	(
		SourceFile {
			syntax: parse.syntax(),
		},
		parse.errors,
	)
}
