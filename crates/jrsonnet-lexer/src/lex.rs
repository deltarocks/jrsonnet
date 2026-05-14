use core::ops::Range;

use logos::Logos;

use crate::{
	Span,
	generated::syntax_kinds::SyntaxKind,
	string_block::{StringBlockError, lex_str_block},
};

/// Jsonnet code lexer, use it instead of [`logos::Lexer`] to properly handle custom lexing errors
/// for items such as [`SyntaxKind::STRING_BLOCK`]
pub struct Lexer<'a> {
	inner: logos::Lexer<'a, SyntaxKind>,
}

impl<'a> Lexer<'a> {
	/// Create the lexer for the passed jsonnet code
	pub fn new(input: &'a str) -> Self {
		Self {
			inner: SyntaxKind::lexer(input),
		}
	}
}

impl<'a> Iterator for Lexer<'a> {
	type Item = Lexeme<'a>;

	fn next(&mut self) -> Option<Self::Item> {
		use SyntaxKind::*;

		let mut kind = self.inner.next()?;
		let text = self.inner.slice();

		if kind == Ok(STRING_BLOCK) {
			// We use custom lexer, which skips enough bytes, but not returns error
			// Instead we should call lexer again to verify if there is something wrong with string block
			let mut lexer = logos::Lexer::<SyntaxKind>::new(text);
			// In kinds, string blocks is parsed at least as `|||`
			lexer.bump(3);
			let res = lex_str_block(&mut lexer);
			let next = lexer.next();
			assert!(next.is_none(), "str_block is lexed");
			match res {
				Ok(()) => {}
				Err(e) => {
					kind = Ok(match e {
						StringBlockError::UnexpectedEnd => ERROR_STRING_BLOCK_UNEXPECTED_END,
						StringBlockError::MissingNewLine => ERROR_STRING_BLOCK_MISSING_NEW_LINE,
						StringBlockError::MissingTermination => {
							ERROR_STRING_BLOCK_MISSING_TERMINATION
						}
						StringBlockError::MissingIndent => ERROR_STRING_BLOCK_MISSING_INDENT,
					});
				}
			}
		}

		Some(Self::Item {
			kind: kind.unwrap_or(SyntaxKind::LEXING_ERROR),
			text,
			range: {
				let Range { start, end } = self.inner.span();

				Span(
					u32::try_from(start).expect("code size is limited by 4gb"),
					u32::try_from(end).expect("code size is limited by 4gb"),
				)
			},
		})
	}
}

/// Parsed lexeme
#[derive(Clone, Copy, Debug)]
pub struct Lexeme<'s> {
	/// Token kind, e.g [`SyntaxKind::COLON`]
	pub kind: SyntaxKind,
	/// Token string value can be used to extract the actual jsonnet value
	pub text: &'s str,
	/// Token range
	pub range: Span,
}

/// Lex jsonnet code
pub fn lex(input: &str) -> Vec<Lexeme<'_>> {
	Lexer::new(input).collect()
}
