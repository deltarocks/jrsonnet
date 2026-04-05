use std::borrow::Cow;

use jrsonnet_evaluator::{
	IStr, ObjValue, Result, ResultExt, Val, bail, in_description_frame,
	manifest::{ManifestFormat, escape_string_json_buf},
	val::ArrValue,
};

pub struct TomlFormat<'s> {
	/// Padding before fields, i.e
	/// ```toml
	/// [a]
	///   b = 1
	/// ## <- this
	/// ```
	padding: Cow<'s, str>,
	/// Do not emit sections for objects, consisting only from sections:
	/// ```toml
	/// # false
	/// [a]
	/// [a.b]
	///
	/// # true
	/// [a.b]
	/// ```
	skip_empty_sections: bool,
	/// If true - then order of fields is preserved as written,
	/// instead of sorting alphabetically
	#[cfg(feature = "exp-preserve-order")]
	preserve_order: bool,
}
impl TomlFormat<'_> {
	pub fn cli(
		padding: usize,
		#[cfg(feature = "exp-preserve-order")] preserve_order: bool,
	) -> Self {
		let padding = " ".repeat(padding);
		Self {
			padding: Cow::Owned(padding),
			skip_empty_sections: true,
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
		}
	}
	pub fn std_to_toml(
		padding: String,
		#[cfg(feature = "exp-preserve-order")] preserve_order: bool,
	) -> Self {
		Self {
			padding: Cow::Owned(padding),
			skip_empty_sections: false,
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
		}
	}
}

fn bare_allowed(s: &str) -> bool {
	s.bytes()
		.all(|c| matches!(c, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

fn escape_key_toml_buf(key: &str, buf: &mut String) {
	if bare_allowed(key) {
		buf.push_str(key);
	} else {
		escape_string_json_buf(key, buf);
	}
}

fn is_section(val: &Val) -> Result<bool> {
	Ok(match val {
		Val::Arr(a) => {
			if a.is_empty() {
				return Ok(false);
			}
			for e in a.iter() {
				let e = e?;
				if !matches!(e, Val::Obj(_)) {
					return Ok(false);
				}
			}
			true
		}
		Val::Obj(_) => true,
		_ => false,
	})
}

fn manifest_value(
	val: &Val,
	inline: bool,
	buf: &mut String,
	cur_padding: &str,
	options: &TomlFormat<'_>,
) -> Result<()> {
	use std::fmt::Write;
	match val {
		Val::Bool(true) => buf.push_str("true"),
		Val::Bool(false) => buf.push_str("false"),
		Val::Str(s) => {
			escape_string_json_buf(&s.clone().into_flat(), buf);
		}
		Val::Num(n) => write!(buf, "{n}").unwrap(),
		#[cfg(feature = "exp-bigint")]
		Val::BigInt(n) => write!(buf, "{n}").unwrap(),
		Val::Arr(a) => {
			buf.push('[');

			let mut had_items = false;
			for (i, e) in a.iter().enumerate() {
				had_items = true;
				let e = e.with_description(|| format!("elem <{i}> evaluation"))?;

				if i != 0 {
					buf.push(',');
				}
				if inline {
					buf.push(' ');
				} else {
					buf.push('\n');
					buf.push_str(cur_padding);
					buf.push_str(&options.padding);
				}

				in_description_frame(
					|| format!("elem <{i}> manifestification"),
					|| manifest_value(&e, true, buf, "", options),
				)?;
			}

			if !had_items {
			} else if inline {
				buf.push(' ');
			} else {
				buf.push('\n');
				buf.push_str(cur_padding);
			}
			buf.push(']');
		}
		Val::Obj(o) => {
			o.run_assertions()?;
			buf.push('{');

			let mut had_fields = false;
			for (i, (k, v)) in o
				.iter(
					#[cfg(feature = "exp-preserve-order")]
					options.preserve_order,
				)
				.enumerate()
			{
				had_fields = true;
				let v = v.with_description(|| format!("field <{k}> evaluation"))?;

				if i != 0 {
					buf.push(',');
				}
				buf.push(' ');

				escape_key_toml_buf(&k, buf);
				buf.push_str(" = ");
				in_description_frame(
					|| format!("field <{k}> manifestification"),
					|| manifest_value(&v, true, buf, "", options),
				)?;
			}

			if had_fields {
				buf.push(' ');
			}

			buf.push('}');
		}
		Val::Null => {
			bail!("tried to manifest null")
		}
		Val::Func(_) => {
			bail!("tried to manifest function")
		}
	}
	Ok(())
}

fn manifest_table_internal(
	obj: &ObjValue,
	path: &mut Vec<IStr>,
	buf: &mut String,
	cur_padding: &mut String,
	options: &TomlFormat<'_>,
) -> Result<()> {
	let mut sections = Vec::new();
	let mut first = true;
	for (key, value) in obj.iter(
		#[cfg(feature = "exp-preserve-order")]
		options.preserve_order,
	) {
		let value = value.with_description(|| format!("field <{key}> evaluation"))?;
		if is_section(&value)? {
			sections.push((key, value));
		} else {
			if !first {
				buf.push('\n');
			}
			first = false;
			buf.push_str(cur_padding);
			escape_key_toml_buf(&key, buf);
			buf.push_str(" = ");
			in_description_frame(
				|| format!("table <{key}> manifestification"),
				|| manifest_value(&value, false, buf, cur_padding, options),
			)?;
		}
	}
	for (k, v) in sections {
		if !first {
			buf.push_str("\n\n");
		}
		first = false;
		path.push(k.clone());
		in_description_frame(
			|| format!("section <{k}> manifestification"),
			|| match v {
				Val::Obj(obj) => manifest_table(&obj, path, buf, cur_padding, options),
				Val::Arr(arr) => manifest_table_array(&arr, path, buf, cur_padding, options),
				_ => unreachable!("iterating over sections"),
			},
		)?;
		path.pop();
	}
	Ok(())
}

fn manifest_table(
	obj: &ObjValue,
	path: &mut Vec<IStr>,
	buf: &mut String,
	cur_padding: &mut String,
	options: &TomlFormat<'_>,
) -> Result<()> {
	if options.skip_empty_sections
		&& !obj.is_empty()
		&& obj
			.iter(
				#[cfg(feature = "exp-preserve-order")]
				false,
			)
			.try_fold(true, |c, (_, v)| Ok(c && is_section(&v?)?) as Result<bool>)?
	{
		manifest_table_internal(obj, path, buf, cur_padding, options)?;
		return Ok(());
	}
	buf.push_str(cur_padding);
	buf.push('[');
	for (i, k) in path.iter().enumerate() {
		if i != 0 {
			buf.push('.');
		}
		escape_key_toml_buf(k, buf);
	}
	buf.push(']');
	if obj.is_empty() {
		return Ok(());
	}
	buf.push('\n');
	let prev_len = cur_padding.len();
	cur_padding.push_str(&options.padding);
	manifest_table_internal(obj, path, buf, cur_padding, options)?;
	cur_padding.truncate(prev_len);
	Ok(())
}
fn manifest_table_array(
	arr: &ArrValue,
	path: &mut Vec<IStr>,
	buf: &mut String,
	cur_padding: &mut String,
	options: &TomlFormat<'_>,
) -> Result<()> {
	let mut formatted_path = String::new();
	{
		formatted_path.push_str(cur_padding);
		formatted_path.push_str("[[");
		for (i, k) in path.iter().enumerate() {
			if i != 0 {
				formatted_path.push('.');
			}
			escape_key_toml_buf(k, &mut formatted_path);
		}
		formatted_path.push_str("]]");
	}
	let prev_len = cur_padding.len();
	cur_padding.push_str(&options.padding);
	for (i, e) in arr.iter().enumerate() {
		let obj = e.expect("already tested").as_obj().expect("already tested");
		if i != 0 {
			buf.push_str("\n\n");
		}
		buf.push_str(&formatted_path);
		if obj.is_empty() {
			continue;
		}
		buf.push('\n');
		manifest_table_internal(&obj, path, buf, cur_padding, options)?;
	}
	cur_padding.truncate(prev_len);
	Ok(())
}

impl ManifestFormat for TomlFormat<'_> {
	fn manifest_buf(&self, val: Val, buf: &mut String) -> jrsonnet_evaluator::Result<()> {
		match val {
			Val::Obj(obj) => {
				manifest_table_internal(&obj, &mut Vec::new(), buf, &mut String::new(), self)
			}
			_ => bail!("toml body should be object"),
		}
	}
}
