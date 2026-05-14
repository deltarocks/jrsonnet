//! Jrsonnet jsonnet lexer shared between multiple parser implementations
#![deny(missing_docs)]

mod generated;
mod lex;
pub mod string;
pub mod string_block;

/// Represents code range in bytes
#[derive(Clone, Copy, Debug)]
pub struct Span(pub u32, pub u32);

pub use generated::syntax_kinds::SyntaxKind;
pub use lex::{Lexeme, Lexer, lex};
pub use string::unescape;
pub use string_block::{CollectStrBlock, collect_lexed_str_block};
