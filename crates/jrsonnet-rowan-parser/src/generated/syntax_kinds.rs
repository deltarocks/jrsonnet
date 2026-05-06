//! This is a generated file, please do not edit manually. Changes can be
//! made in codegeneration that lives in `xtask` top-level dir.

#![allow(
	bad_style,
	missing_docs,
	unreachable_pub,
	clippy::manual_non_exhaustive,
	clippy::match_like_matches_macro
)]
#[doc = r" The kind of syntax node, e.g. `IDENT`, `USE_KW`, or `STRUCT`."]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u16)]
pub enum SyntaxKind {
	#[doc(hidden)]
	TOMBSTONE,
	#[doc(hidden)]
	EOF,
	OR,
	NULL_COAELSE,
	AND,
	BIT_OR,
	BIT_XOR,
	BIT_AND,
	EQ,
	NE,
	LT,
	GT,
	LE,
	GE,
	LHS,
	RHS,
	PLUS,
	MINUS,
	MUL,
	DIV,
	MODULO,
	NOT,
	BIT_NOT,
	L_BRACK,
	R_BRACK,
	L_PAREN,
	R_PAREN,
	L_BRACE,
	R_BRACE,
	COLON,
	SEMI,
	DOT,
	DOTDOTDOT,
	COMMA,
	DOLLAR,
	ASSIGN,
	QUESTION_MARK,
	FLOAT,
	ERROR_FLOAT_JUNK_AFTER_POINT,
	ERROR_FLOAT_JUNK_AFTER_EXPONENT,
	ERROR_FLOAT_JUNK_AFTER_EXPONENT_SIGN,
	STRING_DOUBLE,
	ERROR_STRING_DOUBLE_UNTERMINATED,
	STRING_SINGLE,
	ERROR_STRING_SINGLE_UNTERMINATED,
	STRING_DOUBLE_VERBATIM,
	ERROR_STRING_DOUBLE_VERBATIM_UNTERMINATED,
	STRING_SINGLE_VERBATIM,
	ERROR_STRING_SINGLE_VERBATIM_UNTERMINATED,
	ERROR_STRING_VERBATIM_MISSING_QUOTES,
	STRING_BLOCK,
	ERROR_STRING_BLOCK_UNEXPECTED_END,
	ERROR_STRING_BLOCK_MISSING_NEW_LINE,
	ERROR_STRING_BLOCK_MISSING_TERMINATION,
	ERROR_STRING_BLOCK_MISSING_INDENT,
	IDENT,
	WHITESPACE,
	SINGLE_LINE_SLASH_COMMENT,
	SINGLE_LINE_HASH_COMMENT,
	MULTI_LINE_COMMENT,
	ERROR_COMMENT_TOO_SHORT,
	ERROR_COMMENT_UNTERMINATED,
	ERROR_NO_OPERATOR,
	ERROR_MISSING_TOKEN,
	ERROR_UNEXPECTED_TOKEN,
	ERROR_CUSTOM,
	TAILSTRICT_KW,
	LOCAL_KW,
	IMPORTSTR_KW,
	IMPORTBIN_KW,
	IMPORT_KW,
	IF_KW,
	THEN_KW,
	ELSE_KW,
	FUNCTION_KW,
	ERROR_KW,
	IN_KW,
	META_OBJECT_APPLY,
	NULL_KW,
	TRUE_KW,
	FALSE_KW,
	SELF_KW,
	SUPER_KW,
	FOR_KW,
	ASSERT_KW,
	LEXING_ERROR,
	__LAST_TOKEN,
	SOURCE_FILE,
	EXPR,
	SUFFIX_INDEX,
	NAME,
	SUFFIX_INDEX_EXPR,
	SUFFIX_SLICE,
	SLICE_DESC,
	SUFFIX_APPLY,
	ARGS_DESC,
	STMT_LOCAL,
	STMT_ASSERT,
	ASSERTION,
	EXPR_BINARY,
	EXPR_UNARY,
	EXPR_OBJ_EXTEND,
	EXPR_PARENED,
	EXPR_LITERAL,
	EXPR_STRING,
	EXPR_NUMBER,
	EXPR_ARRAY,
	EXPR_OBJECT,
	EXPR_ARRAY_COMP,
	EXPR_IMPORT,
	EXPR_VAR,
	EXPR_IF_THEN_ELSE,
	TRUE_EXPR,
	FALSE_EXPR,
	EXPR_FUNCTION,
	PARAMS_DESC,
	EXPR_ERROR,
	SLICE_DESC_END,
	SLICE_DESC_STEP,
	ARG,
	OBJ_BODY_COMP,
	OBJ_BODY_MEMBER_LIST,
	MEMBER_BIND_STMT,
	OBJ_LOCAL,
	MEMBER_ASSERT_STMT,
	MEMBER_FIELD_NORMAL,
	VISIBILITY,
	MEMBER_FIELD_METHOD,
	FIELD_NAME_FIXED,
	FIELD_NAME_DYNAMIC,
	FOR_SPEC,
	FOR_OBJ_SPEC,
	IF_SPEC,
	BIND_DESTRUCT,
	BIND_FUNCTION,
	PARAM,
	DESTRUCT_FULL,
	DESTRUCT_SKIP,
	DESTRUCT_ARRAY,
	DESTRUCT_OBJECT,
	DESTRUCT_OBJECT_FIELD,
	DESTRUCT_REST,
	DESTRUCT_ARRAY_ELEMENT,
	SUFFIX,
	BIND,
	STMT,
	OBJ_BODY,
	COMP_SPEC,
	EXPR_BASE,
	MEMBER_COMP,
	MEMBER,
	FIELD_NAME,
	DESTRUCT,
	DESTRUCT_ARRAY_PART,
	BINARY_OPERATOR,
	UNARY_OPERATOR,
	LITERAL,
	TEXT,
	NUMBER,
	IMPORT_KIND,
	TRIVIA,
	CUSTOM_ERROR,
	#[doc(hidden)]
	__LAST,
}
use self::SyntaxKind::*;
impl SyntaxKind {
	pub fn is_keyword(self) -> bool {
		match self {
			OR | NULL_COAELSE | AND | BIT_OR | BIT_XOR | BIT_AND | EQ | NE | LT | GT | LE | GE
			| LHS | RHS | PLUS | MINUS | MUL | DIV | MODULO | NOT | BIT_NOT | L_BRACK | R_BRACK
			| L_PAREN | R_PAREN | L_BRACE | R_BRACE | COLON | SEMI | DOT | DOTDOTDOT | COMMA
			| DOLLAR | ASSIGN | QUESTION_MARK | TAILSTRICT_KW | LOCAL_KW | IMPORTSTR_KW
			| IMPORTBIN_KW | IMPORT_KW | IF_KW | THEN_KW | ELSE_KW | FUNCTION_KW | ERROR_KW
			| IN_KW | NULL_KW | TRUE_KW | FALSE_KW | SELF_KW | SUPER_KW | FOR_KW | ASSERT_KW => true,
			_ => false,
		}
	}
	pub fn is_enum(self) -> bool {
		match self {
			SUFFIX | BIND | STMT | OBJ_BODY | COMP_SPEC | EXPR_BASE | MEMBER_COMP | MEMBER
			| FIELD_NAME | DESTRUCT | DESTRUCT_ARRAY_PART | BINARY_OPERATOR | UNARY_OPERATOR
			| LITERAL | TEXT | NUMBER | IMPORT_KIND | TRIVIA | CUSTOM_ERROR => true,
			_ => false,
		}
	}
	pub fn error_description(self) -> Option<&'static str> {
		match self {
			ERROR_FLOAT_JUNK_AFTER_POINT => {
				::core::option::Option::Some("junk after decimal point in number literal")
			}
			ERROR_FLOAT_JUNK_AFTER_EXPONENT => {
				::core::option::Option::Some("junk after exponent in number literal")
			}
			ERROR_FLOAT_JUNK_AFTER_EXPONENT_SIGN => {
				::core::option::Option::Some("junk after exponent sign in number literal")
			}
			ERROR_STRING_DOUBLE_UNTERMINATED => {
				::core::option::Option::Some("unterminated double-quoted string")
			}
			ERROR_STRING_SINGLE_UNTERMINATED => {
				::core::option::Option::Some("unterminated single-quoted string")
			}
			ERROR_STRING_DOUBLE_VERBATIM_UNTERMINATED => {
				::core::option::Option::Some("unterminated verbatim double-quoted string")
			}
			ERROR_STRING_SINGLE_VERBATIM_UNTERMINATED => {
				::core::option::Option::Some("unterminated verbatim single-quoted string")
			}
			ERROR_STRING_VERBATIM_MISSING_QUOTES => {
				::core::option::Option::Some("verbatim string missing opening quotes")
			}
			ERROR_STRING_BLOCK_UNEXPECTED_END => {
				::core::option::Option::Some("unexpected end of text block")
			}
			ERROR_STRING_BLOCK_MISSING_NEW_LINE => {
				::core::option::Option::Some("text block requires new line after |||")
			}
			ERROR_STRING_BLOCK_MISSING_TERMINATION => {
				::core::option::Option::Some("unterminated text block")
			}
			ERROR_STRING_BLOCK_MISSING_INDENT => {
				::core::option::Option::Some("text block first line must be indented")
			}
			ERROR_COMMENT_TOO_SHORT => ::core::option::Option::Some("comment too short"),
			ERROR_COMMENT_UNTERMINATED => {
				::core::option::Option::Some("unterminated multi-line comment")
			}
			ERROR_NO_OPERATOR => ::core::option::Option::Some("expected operator"),
			ERROR_MISSING_TOKEN => ::core::option::Option::Some("missing token"),
			ERROR_UNEXPECTED_TOKEN => ::core::option::Option::Some("unexpected token"),
			ERROR_CUSTOM => ::core::option::Option::Some("error"),
			LEXING_ERROR => ::core::option::Option::Some("unexpected character"),
			_ => None,
		}
	}
	pub fn display_name(self) -> &'static str {
		match self {
			OR => "'||'",
			NULL_COAELSE => "'??'",
			AND => "'&&'",
			BIT_OR => "'|'",
			BIT_XOR => "'^'",
			BIT_AND => "'&'",
			EQ => "'=='",
			NE => "'!='",
			LT => "'<'",
			GT => "'>'",
			LE => "'<='",
			GE => "'>='",
			LHS => "'<<'",
			RHS => "'>>'",
			PLUS => "'+'",
			MINUS => "'-'",
			MUL => "'*'",
			DIV => "'/'",
			MODULO => "'%'",
			NOT => "'!'",
			BIT_NOT => "'~'",
			L_BRACK => "'['",
			R_BRACK => "']'",
			L_PAREN => "'('",
			R_PAREN => "')'",
			L_BRACE => "'{'",
			R_BRACE => "'}'",
			COLON => "':'",
			SEMI => "';'",
			DOT => "'.'",
			DOTDOTDOT => "'...'",
			COMMA => "','",
			DOLLAR => "'$'",
			ASSIGN => "'='",
			QUESTION_MARK => "'?'",
			FLOAT => "number",
			ERROR_FLOAT_JUNK_AFTER_POINT => "junk after decimal point in number literal",
			ERROR_FLOAT_JUNK_AFTER_EXPONENT => "junk after exponent in number literal",
			ERROR_FLOAT_JUNK_AFTER_EXPONENT_SIGN => "junk after exponent sign in number literal",
			STRING_DOUBLE => "\"string\"",
			ERROR_STRING_DOUBLE_UNTERMINATED => "unterminated double-quoted string",
			STRING_SINGLE => "'string'",
			ERROR_STRING_SINGLE_UNTERMINATED => "unterminated single-quoted string",
			STRING_DOUBLE_VERBATIM => "@\"string\"",
			ERROR_STRING_DOUBLE_VERBATIM_UNTERMINATED => {
				"unterminated verbatim double-quoted string"
			}
			STRING_SINGLE_VERBATIM => "@'string'",
			ERROR_STRING_SINGLE_VERBATIM_UNTERMINATED => {
				"unterminated verbatim single-quoted string"
			}
			ERROR_STRING_VERBATIM_MISSING_QUOTES => "verbatim string missing opening quotes",
			STRING_BLOCK => "|||string|||",
			ERROR_STRING_BLOCK_UNEXPECTED_END => "unexpected end of text block",
			ERROR_STRING_BLOCK_MISSING_NEW_LINE => "text block requires new line after |||",
			ERROR_STRING_BLOCK_MISSING_TERMINATION => "unterminated text block",
			ERROR_STRING_BLOCK_MISSING_INDENT => "text block first line must be indented",
			IDENT => "identifier",
			WHITESPACE => "whitespace",
			SINGLE_LINE_SLASH_COMMENT => "//comment",
			SINGLE_LINE_HASH_COMMENT => "#comment",
			MULTI_LINE_COMMENT => "/*comment*/",
			ERROR_COMMENT_TOO_SHORT => "comment too short",
			ERROR_COMMENT_UNTERMINATED => "unterminated multi-line comment",
			ERROR_NO_OPERATOR => "expected operator",
			ERROR_MISSING_TOKEN => "missing token",
			ERROR_UNEXPECTED_TOKEN => "unexpected token",
			ERROR_CUSTOM => "error",
			TAILSTRICT_KW => "'tailstrict'",
			LOCAL_KW => "'local'",
			IMPORTSTR_KW => "'importstr'",
			IMPORTBIN_KW => "'importbin'",
			IMPORT_KW => "'import'",
			IF_KW => "'if'",
			THEN_KW => "'then'",
			ELSE_KW => "'else'",
			FUNCTION_KW => "'function'",
			ERROR_KW => "'error'",
			IN_KW => "'in'",
			META_OBJECT_APPLY => "meta_object_apply",
			NULL_KW => "'null'",
			TRUE_KW => "'true'",
			FALSE_KW => "'false'",
			SELF_KW => "'self'",
			SUPER_KW => "'super'",
			FOR_KW => "'for'",
			ASSERT_KW => "'assert'",
			LEXING_ERROR => "unexpected character",
			_ => "unknown",
		}
	}
	pub fn from_raw(r: u16) -> Self {
		assert!(r < Self::__LAST as u16);
		unsafe { std::mem::transmute(r) }
	}
	pub fn into_raw(self) -> u16 {
		self as u16
	}
}
#[macro_export]
macro_rules ! T { [||] => { $ crate :: SyntaxKind :: OR } ; [??] => { $ crate :: SyntaxKind :: NULL_COAELSE } ; [&&] => { $ crate :: SyntaxKind :: AND } ; [|] => { $ crate :: SyntaxKind :: BIT_OR } ; [^] => { $ crate :: SyntaxKind :: BIT_XOR } ; [&] => { $ crate :: SyntaxKind :: BIT_AND } ; [==] => { $ crate :: SyntaxKind :: EQ } ; [!=] => { $ crate :: SyntaxKind :: NE } ; [<] => { $ crate :: SyntaxKind :: LT } ; [>] => { $ crate :: SyntaxKind :: GT } ; [<=] => { $ crate :: SyntaxKind :: LE } ; [>=] => { $ crate :: SyntaxKind :: GE } ; [<<] => { $ crate :: SyntaxKind :: LHS } ; [>>] => { $ crate :: SyntaxKind :: RHS } ; [+] => { $ crate :: SyntaxKind :: PLUS } ; [-] => { $ crate :: SyntaxKind :: MINUS } ; [*] => { $ crate :: SyntaxKind :: MUL } ; [/] => { $ crate :: SyntaxKind :: DIV } ; [%] => { $ crate :: SyntaxKind :: MODULO } ; [!] => { $ crate :: SyntaxKind :: NOT } ; [~] => { $ crate :: SyntaxKind :: BIT_NOT } ; ['['] => { $ crate :: SyntaxKind :: L_BRACK } ; [']'] => { $ crate :: SyntaxKind :: R_BRACK } ; ['('] => { $ crate :: SyntaxKind :: L_PAREN } ; [')'] => { $ crate :: SyntaxKind :: R_PAREN } ; ['{'] => { $ crate :: SyntaxKind :: L_BRACE } ; ['}'] => { $ crate :: SyntaxKind :: R_BRACE } ; [:] => { $ crate :: SyntaxKind :: COLON } ; [;] => { $ crate :: SyntaxKind :: SEMI } ; [.] => { $ crate :: SyntaxKind :: DOT } ; [...] => { $ crate :: SyntaxKind :: DOTDOTDOT } ; [,] => { $ crate :: SyntaxKind :: COMMA } ; ['$'] => { $ crate :: SyntaxKind :: DOLLAR } ; [=] => { $ crate :: SyntaxKind :: ASSIGN } ; [?] => { $ crate :: SyntaxKind :: QUESTION_MARK } ; [tailstrict] => { $ crate :: SyntaxKind :: TAILSTRICT_KW } ; [local] => { $ crate :: SyntaxKind :: LOCAL_KW } ; [importstr] => { $ crate :: SyntaxKind :: IMPORTSTR_KW } ; [importbin] => { $ crate :: SyntaxKind :: IMPORTBIN_KW } ; [import] => { $ crate :: SyntaxKind :: IMPORT_KW } ; [if] => { $ crate :: SyntaxKind :: IF_KW } ; [then] => { $ crate :: SyntaxKind :: THEN_KW } ; [else] => { $ crate :: SyntaxKind :: ELSE_KW } ; [function] => { $ crate :: SyntaxKind :: FUNCTION_KW } ; [error] => { $ crate :: SyntaxKind :: ERROR_KW } ; [in] => { $ crate :: SyntaxKind :: IN_KW } ; [null] => { $ crate :: SyntaxKind :: NULL_KW } ; [true] => { $ crate :: SyntaxKind :: TRUE_KW } ; [false] => { $ crate :: SyntaxKind :: FALSE_KW } ; [self] => { $ crate :: SyntaxKind :: SELF_KW } ; [super] => { $ crate :: SyntaxKind :: SUPER_KW } ; [for] => { $ crate :: SyntaxKind :: FOR_KW } ; [assert] => { $ crate :: SyntaxKind :: ASSERT_KW } }
#[allow(unused_imports)]
pub use T;
