use std::{borrow::Cow, fmt::Write};

use jrsonnet_evaluator::{
	Result, ResultExt, Val, bail, in_description_frame,
	manifest::{ManifestFormat, escape_string_json_buf},
};

pub struct YamlFormat<'s> {
	/// Padding before fields, i.e
	/// ```yaml
	/// a:
	///   b:
	/// ## <- this
	/// ```
	padding: Cow<'s, str>,
	/// Padding before array elements in objects
	/// ```yaml
	/// a:
	///   - 1
	/// ## <- this
	/// ```
	arr_element_padding: Cow<'s, str>,
	/// Should yaml keys appear unescaped, when possible
	/// ```yaml
	/// "safe_key": 1
	/// # vs
	/// safe_key: 1
	/// ```
	quote_keys: bool,
	quote_values: bool,
	/// If true - then order of fields is preserved as written,
	/// instead of sorting alphabetically
	#[cfg(feature = "exp-preserve-order")]
	preserve_order: bool,
}
impl YamlFormat<'_> {
	pub fn cli(
		padding: usize,
		#[cfg(feature = "exp-preserve-order")] preserve_order: bool,
	) -> Self {
		let padding = " ".repeat(padding);
		Self {
			padding: Cow::Owned(padding.clone()),
			arr_element_padding: Cow::Owned(padding),
			quote_keys: false,
			quote_values: false,
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
		}
	}
	pub fn std_to_yaml(
		indent_array_in_object: bool,
		quote_keys: bool,
		#[cfg(feature = "exp-preserve-order")] preserve_order: bool,
	) -> Self {
		Self {
			padding: Cow::Borrowed("  "),
			arr_element_padding: Cow::Borrowed(if indent_array_in_object { "  " } else { "" }),
			quote_keys,
			quote_values: true,
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
		}
	}
}
impl ManifestFormat for YamlFormat<'_> {
	fn manifest_buf(&self, val: &Val, buf: &mut String) -> Result<()> {
		manifest_yaml_ex_buf(val, buf, &mut String::new(), self)
	}
}

fn bare_safe(key: &str) -> bool {
	fn count_char_u(k: &str, c: char) -> usize {
		let cu = c.to_ascii_uppercase();
		k.chars().filter(|v| *v == c || *v == cu).count()
	}
	fn count_char(k: &str, c: char) -> usize {
		k.chars().filter(|v| *v == c).count()
	}
	fn is_reserved(key: &str) -> bool {
		const RESERVED: &[&str] = &[
			// Boolean types taken from https://yaml.org/type/bool.html
			"true", "false", "yes", "no", "on", "off", "y", "n",
			// Numerical words taken from https://yaml.org/type/float.html
			".nan", "-.inf", "+.inf", ".inf", "null",
			// Invalid keys that contain no invalid characters
			"-", "---", "",
		];
		RESERVED.iter().any(|k| key.eq_ignore_ascii_case(k))
	}

	#[allow(clippy::if_same_then_else)]
	// Check for unsafe characters
	if !key
		.chars()
		.all(|v| matches!(v, 'a'..='z' | 'A'..='Z' | '0'..='9'  | '-' | '_' | '.' | '/'))
	{
		return false;
	}
	// Check for reserved words
	else if is_reserved(key) {
		return false;
	}
	// Check for timestamp values.  Since spaces and colons are already forbidden,
	// all that could potentially pass is the standard date format (ex MM-DD-YYYY, YYYY-DD-MM, etc).
	// This check is even more conservative: Keys that meet all of the following:
	// - all characters match [0-9\-]
	// - has exactly 2 dashes
	// are considered dates.
	else if key.chars().all(|v| matches!(v, '0'..='9' | '-')) && count_char(key, '-') == 2 {
		return false;
	}
	// Check for integers.  Keys that meet all of the following:
	// - all characters match [0-9_\-]
	// - has at most 1 dash
	// are considered integers.
	else if key.chars().all(|v| matches!(v, '0'..='9' | '-' | '_')) && count_char(key, '-') < 2 {
		return false;
	}
	// Check for binary integers.  Keys that meet all of the following:
	// - all characters match [0-9b_\-]
	// - has at least 3 characters
	// - starts with (-)0b
	// are considered binary integers.
	else if key
		.chars()
		.all(|v| matches!(v, '0'..='9' | '-' | '_' | 'b' | 'B'))
		&& (key.starts_with("0b") || key.starts_with("-0b"))
		&& key.len() > 2
	{
		return false;
	}
	// Check for floats. Keys that meet all of the following:
	// - all characters match [0-9e._\-]
	// - has at most a single period
	// - has at most two dashes
	// - has at most 1 'e'
	// are considered floats.
	else if key
		.chars()
		.all(|v| matches!(v, '0'..='9' | '-' | '_' | 'e' | 'E' | '.'))
		&& count_char_u(key, 'e') < 2
		&& count_char(key, '-') < 3
		&& count_char(key, '.') <= 1
	{
		return false;
	}
	// Check for hexadecimals.  Keys that meet all of the following:
	// - all characters match [0-9a-fx_\-]
	// - has at most 1 dash
	// - has at least 3 characters
	// - starts with (-)0x
	// are considered hexadecimals.
	else if key
		.chars()
		.all(|v| matches!(v, '0'..='9' | '-' | '_' | 'x' | 'X' |  'a'..='f' | 'A'..='F' ))
		&& key.len() >= 3
		&& count_char(key, '-') < 2
		&& (key.starts_with("-0x") || key.starts_with("0x"))
	{
		return false;
	}
	true
}

#[allow(dead_code)]
fn manifest_yaml_ex(val: &Val, options: &YamlFormat<'_>) -> Result<String> {
	let mut out = String::new();
	manifest_yaml_ex_buf(val, &mut out, &mut String::new(), options)?;
	Ok(out)
}

#[allow(clippy::too_many_lines)]
fn manifest_yaml_ex_buf(
	val: &Val,
	buf: &mut String,
	cur_padding: &mut String,
	options: &YamlFormat<'_>,
) -> Result<()> {
	match val {
		Val::Bool(v) => {
			if *v {
				buf.push_str("true");
			} else {
				buf.push_str("false");
			}
		}
		Val::Null => buf.push_str("null"),
		Val::Str(s) => {
			let s = s.clone().into_flat();
			if s.is_empty() {
				buf.push_str("\"\"");
			} else if let Some(s) = s.strip_suffix('\n') {
				buf.push('|');
				for line in s.split('\n') {
					buf.push('\n');
					buf.push_str(cur_padding);
					buf.push_str(&options.padding);
					buf.push_str(line);
				}
			} else if s.contains('\n') {
				buf.push_str("|-");
				for line in s.split('\n') {
					buf.push('\n');
					buf.push_str(cur_padding);
					buf.push_str(&options.padding);
					buf.push_str(line);
				}
			} else if !options.quote_values && bare_safe(&s) {
				buf.push_str(&s);
			} else {
				escape_string_json_buf(&s, buf);
			}
		}
		Val::Num(n) => write!(buf, "{}", *n).unwrap(),
		#[cfg(feature = "exp-bigint")]
		Val::BigInt(n) => write!(buf, "{}", *n).unwrap(),
		Val::Arr(a) => {
			let mut had_items = false;
			for (i, item) in a.iter().enumerate() {
				had_items = true;
				let item = item.with_description(|| format!("elem <{i}> evaluation"))?;
				if i != 0 {
					buf.push('\n');
					buf.push_str(cur_padding);
				}
				buf.push('-');
				match &item {
					Val::Arr(a) if !a.is_empty() => {
						buf.push('\n');
						buf.push_str(cur_padding);
						buf.push_str(&options.padding);
					}
					_ => buf.push(' '),
				}
				let extra_padding = match &item {
					Val::Arr(a) => !a.is_empty(),
					Val::Obj(o) => !o.is_empty(),
					_ => false,
				};
				let prev_len = cur_padding.len();
				if extra_padding {
					cur_padding.push_str(&options.padding);
				}
				in_description_frame(
					|| format!("elem <{i}> manifestification"),
					|| manifest_yaml_ex_buf(&item, buf, cur_padding, options),
				)?;
				cur_padding.truncate(prev_len);
			}
			if !had_items {
				buf.push_str("[]");
			}
		}
		Val::Obj(o) => {
			let mut had_fields = false;
			for (i, (key, value)) in o
				.iter(
					#[cfg(feature = "exp-preserve-order")]
					options.preserve_order,
				)
				.enumerate()
			{
				had_fields = true;
				let value = value.with_description(|| format!("field <{key}> evaluation"))?;
				if i != 0 {
					buf.push('\n');
					buf.push_str(cur_padding);
				}
				if !options.quote_keys && bare_safe(&key) {
					buf.push_str(&key);
				} else {
					escape_string_json_buf(&key, buf);
				}
				buf.push(':');
				let prev_len = cur_padding.len();
				match &value {
					Val::Arr(a) if !a.is_empty() => {
						buf.push('\n');
						buf.push_str(cur_padding);
						buf.push_str(&options.arr_element_padding);
						cur_padding.push_str(&options.arr_element_padding);
					}
					Val::Obj(o) if !o.is_empty() => {
						buf.push('\n');
						buf.push_str(cur_padding);
						buf.push_str(&options.padding);
						cur_padding.push_str(&options.padding);
					}
					_ => buf.push(' '),
				}
				in_description_frame(
					|| format!("field <{key}> manifestification"),
					|| manifest_yaml_ex_buf(&value, buf, cur_padding, options),
				)?;
				cur_padding.truncate(prev_len);
			}
			if !had_fields {
				buf.push_str("{}");
			}
		}
		Val::Func(_) => bail!("tried to manifest function"),
	}
	Ok(())
}
