// `never`
#![cfg(test)]

use hi_doc::{Formatting, SnippetBuilder, Text};

use crate::{AstNode, parse};

fn process(text: &str) -> String {
	use std::fmt::Write;
	let mut out = String::new();
	let (node, errors) = parse(text);
	write!(out, "{:#?}", node.syntax()).unwrap();
	if !errors.is_empty() && !text.is_empty() {
		writeln!(out, "===").unwrap();
		for err in &errors {
			writeln!(out, "{err:?}").unwrap();
		}
		let mut code = text.to_string();

		// Prettier errors at EOF position
		if code.ends_with('\n') {
			code.truncate(code.len() - 1);
			code += " ";
		}
		code += " ";

		let mut s = SnippetBuilder::new(code);

		for error in errors {
			s.error(Text::fragment(
				format!("{}", error.error),
				Formatting::default(),
			))
			.range(error.range.start().into()..=error.range.end().into())
			.build();
		}

		writeln!(out, "===").unwrap();
		let ansi = hi_doc::source_to_ansi(&s.build());
		let text = strip_ansi_escapes::strip_str(&ansi);
		out.push_str(&text);
	}
	out.split('\n')
		.map(|s| s.trim_end().to_string())
		.collect::<Vec<String>>()
		.join("\n")
		.trim_end()
		.to_string()
}
macro_rules! mk_test {
		($($name:ident => $test:expr)+) => {$(
			#[test]
			fn $name() {
				let src = indoc::indoc!($test);
				let result = process(&src);
				insta::assert_snapshot!(stringify!($name), result, src);
			}
		)+};
	}
mk_test!(
	empty => r#" "#
	function => r#"
		function(a, b = 1) a + b
	"#
	function_error_no_value => r#"
		function(a, b = ) a + b
	"#
	function_error_rparen => r#"
		function(a, b
	"#
	function_error_body => r#"
		function(a, b)
	"#
	local_novalue => r#"
		local a =
	"#
	local_no_value_recovery => r#"
		local a =
		local b = 3;
		1
	"#


	no_rhs => r#"
		a +
	"#
	no_lhs => r#"
		* 2
	"#
	no_operator => "
		2 2
	"

	named_before_positional => "
		a(1, 2, b=4, 3, 5, k = 12, 6)
	"

	wrong_field_end => "
		{
			a: 1;
			b: 2;
		}
	"


	plain_call => "
		std.substr(a, 0, std.length(b)) == b
	"

	destruct => "
		local [a, b, c] = arr;
		local [a, ...] = arr_rest;
		local [..., a] = rest_arr;
		local [...] = rest_in_arr;
		local [a, ...n] = arr_rest_n;
		local [...n, a] = rest_arr_n;
		local [...n] = rest_in_arr_n;

		local {a, b, c} = obj;
		local {a, b, c, ...} = obj_rest;
		local {a, b, c, ...n} = obj_rest_n;

		null
	"

	str_block_missing_indent => "
		|||
	"
	str_block_missing_termination => "
		|||
			hello
	"
	str_block_missing_newline => "
		|||hello
	"
	str_block_missing_indent_text => "
		|||
		hello
	"

	unexpected_destruct => "
		local * = 1;
		a
	"
	arr_compspec => r#"
		[a for a in [1, 2, 3]]
	"#
	arr_compspec_comma => "
		[a, for a in [1, 2, 3]]
	"
	arr_compspec_no_elems => "
		[for a in [1, 2, 3]]
	"
	arr_compspec_incompatible_with_multiple_elems => r#"
		[a for a in [1, 2, 3], b]
	"#
	arr_compspec_incompatible_with_multiple_elems_w => r#"
		[a, b, for a in [1, 2, 3], c]
	"#

	obj_compspec => r#"
		{a:1 for a in [1, 2, 3]}
	"#
	obj_compspec_comma => "
		{a:1, for a in [1, 2, 3]}
	"
	obj_compspec_no_elems => "
		{for a in [1, 2, 3]}
	"
	obj_compspec_incompatible_with_multiple_elems => r#"
		{a:1 for a in [1, 2, 3], b:1}
	"#
	obj_compspec_incompatible_with_multiple_elems_w => r#"
		{a:1, b:1, for a in [1, 2, 3], c:1}
	"#

	obj_compspec_incompatible_with_asserts => r#"
		{assert 1, a: 1 for a in [1,2,3]}
	"#

	local_method => r#"
		local
			a(x) = x,
			a = function(x) x,
		; c
	"#
	obj_method => r#"
		{
			a(x): x,
			a: function(x) x,
		}
	"#

	continue_after_total_failure => r#"
		local intr = $intrinsic(test);

		local a = 1, b = 2, c = a + b;

		[c]
	"#

	super_nesting => r#"
		super.a + super.b
	"#

	string_block_trim => r#"
		|||-
			Trimmed text block
		|||
	"#

	visibilities => r#"
		{
			normal: 1,
			hide:: 2,
			unhide::: 3,
		}
	"#

	unary_not => r#"
		!false
	"#

	unary_not_in_call => r#"
		std.assertEqual(!false, true)
	"#

	local_in_binop_rhs => r#"
		a + local x = 1; x
	"#

	for_obj_spec_visible => r#"
		{ [k]: v for [k]: v in obj }
	"#
	for_obj_spec_hidden => r#"
		{ [k]: v for [k]:: v in obj }
	"#
	for_obj_spec_force_visible => r#"
		{ [k]: v for [k]::: v in obj }
	"#
	for_obj_spec_value_destruct => r#"
		{ [k]: a + b for [k]: [a, b] in obj }
	"#

	multi_line_comment_doc => r#"
		{ a:: 1, /** doc **/ b:: 2 }
	"#
	multi_line_comment_empty => r#"
		/**/ 1
	"#
	multi_line_comment_triple_star => r#"
		/***/ 2
	"#
	multi_line_comment_inner_star => r#"
		/* * */ 3
	"#
	multi_line_comment_too_short => r#"
		/*/
	"#
	multi_line_comment_unterminated => r#"
		/** unterminated
	"#
);

#[test]
fn eval_simple() {
	let src = "local a = 1, b = 2; a + local c = 1; c";
	let (node, _errors) = parse(src);

	dbg!(node);
}
