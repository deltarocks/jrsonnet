#[derive(Debug)]
pub struct KindsSrc {
	/// Key - how this token appears in ungrammar
	defined_tokens: IndexMap<String, TokenKind>,
	defined_node_names: HashSet<String>,
	pub nodes: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum TokenKind {
	/// May exist in token tree, but never in source code
	Meta { grammar_name: String, name: String },
	/// Specific parsing/lexing errors may be emitted as this type of kind
	Error {
		grammar_name: String,
		name: String,
		#[allow(dead_code)]
		/// Is this error returned by lexer directly, or from lex.rs
		is_lexer_error: bool,
		regex: Option<String>,
		priority: Option<u32>,
		description: String,
	},
	/// Keyword - literal match of token
	Keyword {
		/// How this keyword appears in grammar/code, should be same as Kinds key
		code: String,
		name: String,
	},
	/// Literal - something defined by user, i.e strings, identifiers, smth
	Literal {
		/// How this keyword appears in grammar, should be same as Kinds key
		grammar_name: String,
		name: String,
		/// Regex for Logos lexer
		regex: String,
		/// Path to custom lexer
		lexer: Option<String>,
	},
}

impl TokenKind {
	pub fn grammar_name(&self) -> &str {
		match self {
			Self::Keyword { code, .. } => code,
			Self::Literal { grammar_name, .. }
			| Self::Meta { grammar_name, .. }
			| Self::Error { grammar_name, .. } => grammar_name,
		}
	}
	/// How this keyword should appear in kinds enum, screaming snake cased
	pub fn name(&self) -> &str {
		match self {
			Self::Keyword { name, .. }
			| Self::Literal { name, .. }
			| Self::Meta { name, .. }
			| Self::Error { name, .. } => name,
		}
	}
	pub fn expand_kind(&self, lexer: bool) -> TokenStream {
		let name = format_ident!("{}", self.name());
		let attr = match self {
			Self::Keyword { code, .. } => quote! {#[token(#code)]},
			Self::Literal { regex, lexer, .. } => {
				let lexer = lexer
					.as_deref()
					.map(TokenStream::from_str)
					.map(|r| r.expect("path is correct"));
				quote! {#[regex(#regex, #lexer)]}
			}
			Self::Error {
				regex, priority, ..
			} if regex.is_some() => {
				let priority = priority.map(|p| quote! {, priority = #p});
				quote! {#[regex(#regex #priority)]}
			}
			_ => quote! {},
		};
		let attr = if lexer {
			attr
		} else {
			quote! {}
		};
		quote! {
			#attr
			#name
		}
	}
	pub fn expand_t_macros(&self) -> Option<TokenStream> {
		match self {
			Self::Keyword { code, name } => {
				let code = escape_token_macro(code);
				let name = format_ident!("{name}");
				Some(quote! {
					[#code] => {$crate::SyntaxKind::#name}
				})
			}
			// Meta items should not appear in T![_]
			_ => None,
		}
	}

	/// How this token should be referenced in code
	/// Keywords are referenced with `T![_]` macro,
	/// and literals are referenced directly by name
	pub fn reference(&self) -> TokenStream {
		if let Self::Keyword { code, .. } = self {
			let code = escape_token_macro(code);
			quote! {T![#code]}
		} else {
			let name = self.name();
			let ident = format_ident!("{name}");
			quote! {#ident}
		}
	}

	pub fn display_name(&self) -> String {
		match self {
			Self::Keyword { code, .. } => format!("'{code}'"),
			Self::Literal { name, .. } => match name.as_str() {
				"FLOAT" => "number".to_owned(),
				"IDENT" => "identifier".to_owned(),
				"STRING_DOUBLE" => "\"string\"".to_owned(),
				"STRING_SINGLE" => "'string'".to_owned(),
				"STRING_DOUBLE_VERBATIM" => "@\"string\"".to_owned(),
				"STRING_SINGLE_VERBATIM" => "@'string'".to_owned(),
				"STRING_BLOCK" => "|||string|||".to_owned(),
				"WHITESPACE" => "whitespace".to_owned(),
				"SINGLE_LINE_SLASH_COMMENT" => "//comment".to_owned(),
				"SINGLE_LINE_HASH_COMMENT" => "#comment".to_owned(),
				"MULTI_LINE_COMMENT" => "/*comment*/".to_owned(),
				_ => name.to_lowercase(),
			},
			Self::Meta { name, .. } => name.to_lowercase(),
			Self::Error { description, .. } => description.clone(),
		}
	}

	pub fn method_name(&self) -> Ident {
		match self {
			Self::Keyword { name, .. } => {
				format_ident!("{}_token", name.to_lowercase())
			}
			Self::Literal { name, .. } => {
				format_ident!("{}_lit", name.to_lowercase())
			}
			Self::Meta { name, .. } => format_ident!("{}_meta", name.to_lowercase()),
			Self::Error { name, .. } => format_ident!("{}_error", name.to_lowercase()),
		}
	}
}

#[macro_export]
macro_rules! define_kinds {
	($into:ident = lit($name:literal) => $regex:literal $(, $lexer:literal)? $(; $($rest:tt)*)?) => {{
		$into.define_token(TokenKind::Literal {
			grammar_name: format!("LIT_{}!", $name),
			name: $name.to_owned(),
			regex: $regex.to_owned(),
			lexer: None $(.or_else(|| Some($lexer.to_string())))?,
		});
		$(define_kinds!($into = $($rest)*))?
	}};
	($into:ident = error($name:literal, $desc:literal $(, priority = $priority:literal)? $(, lexer = $lexer:literal)?) $(=> $regex:literal)? $(; $($rest:tt)*)?) => {{
		{
			let regex = None$(.or(Some($regex.to_owned())))?;
			let priority = None$(.or(Some($priority)))?;
			$into.define_token(TokenKind::Error {
				grammar_name: format!("ERROR_{}!", $name),
				name: format!("ERROR_{}", $name),
				is_lexer_error: false $(|| $lexer)? || regex.is_some() || priority.is_some(),
				regex,
				priority,
				description: $desc.to_owned(),
			});
		}
		$(define_kinds!($into = $($rest)*))?
	}};
	($into:ident = $tok:literal => $name:literal $(; $($rest:tt)*)?) => {{
		$into.define_token(TokenKind::Keyword {
			code: format!("{}", $tok),
			name: $name.to_owned(),
		});
		$(define_kinds!($into = $($rest)*))?
	}};
	($into:ident =) => {{}}
}
use std::{collections::HashSet, str::FromStr};

use indexmap::IndexMap;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

use super::escape_token_macro;

impl KindsSrc {
	pub fn new() -> Self {
		Self {
			defined_tokens: IndexMap::new(),
			defined_node_names: HashSet::new(),
			nodes: Vec::new(),
		}
	}
	pub fn define_token(&mut self, token: TokenKind) {
		assert!(
			self.defined_node_names.insert(token.name().to_owned()),
			"node name already defined: {}",
			token.name()
		);
		assert!(
			self.defined_tokens
				.insert(token.grammar_name().to_owned(), token.clone())
				.is_none(),
			"token already defined: {}",
			token.grammar_name()
		);
	}
	pub fn define_node(&mut self, node: &str) {
		assert!(
			self.defined_node_names.insert(node.to_owned()),
			"node name already defined: {node}"
		);
		self.nodes.push(node.to_string());
	}
	pub fn token(&self, tok: &str) -> Option<&TokenKind> {
		self.defined_tokens.get(tok)
	}
	pub fn is_token(&self, tok: &str) -> bool {
		self.defined_tokens.contains_key(tok)
	}
	pub fn tokens(&self) -> impl Iterator<Item = &TokenKind> {
		self.defined_tokens.iter().map(|(_, v)| v)
	}
}

pub fn jsonnet_kinds() -> KindsSrc {
	let mut kinds = KindsSrc::new();
	define_kinds![kinds =
		"||" => "OR";
		"??" => "NULL_COAELSE";
		"&&" => "AND";
		"|" => "BIT_OR";
		"^" => "BIT_XOR";
		"&" => "BIT_AND";
		"==" => "EQ";
		"!=" => "NE";
		"<" => "LT";
		">" => "GT";
		"<=" => "LE";
		">=" => "GE";
		"<<" => "LHS";
		">>" => "RHS";
		"+" => "PLUS";
		"-" => "MINUS";
		"*" => "MUL";
		"/" => "DIV";
		"%" => "MODULO";
		"!" => "NOT";
		"~" => "BIT_NOT";
		"[" => "L_BRACK";
		"]" => "R_BRACK";
		"(" => "L_PAREN";
		")" => "R_PAREN";
		"{" => "L_BRACE";
		"}" => "R_BRACE";
		":" => "COLON";
		";" => "SEMI";
		"." => "DOT";
		"..." => "DOTDOTDOT";
		"," => "COMMA";
		"$" => "DOLLAR";
		"=" => "ASSIGN";
		"?" => "QUESTION_MARK";
		// Literals
		lit("FLOAT") => r"(?:0|[1-9][0-9]*(?:_[0-9]+)*)(?:\.[0-9]+(?:_[0-9]+)*)?(?:[eE][+-]?[0-9]+(?:_[0-9]+)*)?";
		error("FLOAT_JUNK_AFTER_POINT", "junk after decimal point in number literal") => r"(?:0|[1-9][0-9]*(?:_[0-9]+)*)\.[^0-9]";
		error("FLOAT_JUNK_AFTER_EXPONENT", "junk after exponent in number literal") => r"(?:0|[1-9][0-9]*(?:_[0-9]+)*)(?:\.[0-9]+(?:_[0-9]+)*)?[eE][^+\-0-9]";
		error("FLOAT_JUNK_AFTER_EXPONENT_SIGN", "junk after exponent sign in number literal") => r"(?:0|[1-9][0-9]*(?:_[0-9]+)*)(?:\.[0-9]+(?:_[0-9]+)*)?[eE][+-][^0-9]";
		lit("STRING_DOUBLE") => "\"(?s:[^\"\\\\]|\\\\.)*\"";
		error("STRING_DOUBLE_UNTERMINATED", "unterminated double-quoted string") => "\"(?s:[^\"\\\\]|\\\\.)*";
		lit("STRING_SINGLE") => "'(?s:[^'\\\\]|\\\\.)*'";
		error("STRING_SINGLE_UNTERMINATED", "unterminated single-quoted string") => "'(?s:[^'\\\\]|\\\\.)*";
		lit("STRING_DOUBLE_VERBATIM") => "@\"(?:[^\"]|\"\")*\"";
		error("STRING_DOUBLE_VERBATIM_UNTERMINATED", "unterminated verbatim double-quoted string") => "@\"(?:[^\"]|\"\")*";
		lit("STRING_SINGLE_VERBATIM") => "@'(?:[^']|'')*'";
		error("STRING_SINGLE_VERBATIM_UNTERMINATED", "unterminated verbatim single-quoted string") => "@'(?:[^']|'')*";
		error("STRING_VERBATIM_MISSING_QUOTES", "verbatim string missing opening quotes") => "@[^\"'\\s]\\S+";
		lit("STRING_BLOCK") => r"\|\|\|", "crate::string_block::lex_str_block_test";
		error("STRING_BLOCK_UNEXPECTED_END", "unexpected end of text block", lexer = true);
		error("STRING_BLOCK_MISSING_NEW_LINE", "text block requires new line after |||", lexer = true);
		error("STRING_BLOCK_MISSING_TERMINATION", "unterminated text block", lexer = true);
		error("STRING_BLOCK_MISSING_INDENT", "text block first line must be indented", lexer = true);
		lit("IDENT") => r"[_a-zA-Z][_a-zA-Z0-9]*";
		lit("WHITESPACE") => r"[ \t\n\r]+";
		lit("SINGLE_LINE_SLASH_COMMENT") => r"//[^\r\n]*?(\r\n|\n)?";
		lit("SINGLE_LINE_HASH_COMMENT") => r"#[^\r\n]*?(\r\n|\n)?";
		lit("MULTI_LINE_COMMENT") => r"/\*([^*]|\*+[^*/])*\*+/";
		error("COMMENT_TOO_SHORT", "comment too short") => r"/\*/";
		error("COMMENT_UNTERMINATED", "unterminated multi-line comment") =>  r"/\*([^*/]|\*[^/])+";
		error("NO_OPERATOR", "expected operator");
		error("MISSING_TOKEN", "missing token");
		error("UNEXPECTED_TOKEN", "unexpected token");
		error("CUSTOM", "error");
	];
	kinds
}
