use std::{borrow::Cow, fmt::Write, io, ptr};

use crate::{Result, ResultExt, Val, bail, in_description_frame};

pub trait ManifestFormat {
	fn manifest_buf(&self, val: Val, buf: &mut String) -> Result<()>;
	fn manifest(&self, val: Val) -> Result<String> {
		let mut out = String::new();
		self.manifest_buf(val, &mut out)?;
		Ok(out)
	}
	/// Write manifested output directly to a writer, enabling streaming for large outputs.
	///
	/// The default implementation buffers into a String and writes it all at once.
	/// Streaming formats (like YamlStreamFormat) override this to write incrementally,
	/// keeping only one stream element in memory at a time.
	///
	/// Returns `true` if any output was written, `false` if the output was empty.
	fn manifest_write(&self, val: Val, out: &mut dyn io::Write) -> Result<bool> {
		let s = self.manifest(val)?;
		if !s.is_empty() {
			write_io(out, s.as_bytes())?;
			Ok(true)
		} else {
			Ok(false)
		}
	}
	/// When outputing to file, is it safe to append a trailing newline (I.e newline won't change
	/// the meaning).
	///
	/// Default implementation returns `true`
	fn file_trailing_newline(&self) -> bool {
		true
	}
}
impl<T> ManifestFormat for Box<T>
where
	T: ManifestFormat + ?Sized,
{
	fn manifest_buf(&self, val: Val, buf: &mut String) -> Result<()> {
		let inner = &**self;
		inner.manifest_buf(val, buf)
	}
	fn manifest_write(&self, val: Val, out: &mut dyn io::Write) -> Result<bool> {
		let inner = &**self;
		inner.manifest_write(val, out)
	}
	fn file_trailing_newline(&self) -> bool {
		let inner = &**self;
		inner.file_trailing_newline()
	}
}
impl<T> ManifestFormat for &'_ T
where
	T: ManifestFormat + ?Sized,
{
	fn manifest_buf(&self, val: Val, buf: &mut String) -> Result<()> {
		let inner = &**self;
		inner.manifest_buf(val, buf)
	}
	fn manifest_write(&self, val: Val, out: &mut dyn io::Write) -> Result<bool> {
		let inner = &**self;
		inner.manifest_write(val, out)
	}
	fn file_trailing_newline(&self) -> bool {
		let inner = &**self;
		inner.file_trailing_newline()
	}
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum JsonFormatting {
	// Applied in manifestification
	Manifest,
	/// Used for std.manifestJson
	/// Empty array/objects extends to "[\n\n]" instead of "[ ]" as in manifest
	Std,
	/// No line breaks, used in `obj+''`
	ToString,
	/// Minified json
	Minify,
}

pub struct JsonFormat<'s> {
	padding: Cow<'s, str>,
	mtype: JsonFormatting,
	newline: &'s str,
	key_val_sep: &'s str,
	#[cfg(feature = "exp-preserve-order")]
	preserve_order: bool,
	#[cfg(feature = "exp-bigint")]
	preserve_bigints: bool,
	debug_truncate_strings: Option<usize>,
}

impl<'s> JsonFormat<'s> {
	// Minifying format
	pub fn minify(#[cfg(feature = "exp-preserve-order")] preserve_order: bool) -> Self {
		Self {
			padding: Cow::Borrowed(""),
			mtype: JsonFormatting::Minify,
			newline: "\n",
			key_val_sep: ":",
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
			#[cfg(feature = "exp-bigint")]
			preserve_bigints: false,
			debug_truncate_strings: None,
		}
	}
	/// Same format as std.toString, except does not keeps top-level string as-is
	/// To avoid confusion, the format is private in jrsonnet, use [`ToStringFormat`] instead
	const fn std_to_string_helper() -> Self {
		Self {
			padding: Cow::Borrowed(""),
			mtype: JsonFormatting::ToString,
			newline: "\n",
			key_val_sep: ": ",
			#[cfg(feature = "exp-preserve-order")]
			preserve_order: false,
			#[cfg(feature = "exp-bigint")]
			preserve_bigints: false,
			debug_truncate_strings: None,
		}
	}
	pub fn std_to_json(
		padding: String,
		newline: &'s str,
		key_val_sep: &'s str,
		#[cfg(feature = "exp-preserve-order")] preserve_order: bool,
	) -> Self {
		Self {
			padding: Cow::Owned(padding),
			mtype: JsonFormatting::Std,
			newline,
			key_val_sep,
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
			#[cfg(feature = "exp-bigint")]
			preserve_bigints: false,
			debug_truncate_strings: None,
		}
	}
	// Same format as CLI manifestification
	pub fn cli(
		padding: usize,
		#[cfg(feature = "exp-preserve-order")] preserve_order: bool,
	) -> Self {
		if padding == 0 {
			return Self::minify(
				#[cfg(feature = "exp-preserve-order")]
				preserve_order,
			);
		}
		Self {
			padding: Cow::Owned(" ".repeat(padding)),
			mtype: JsonFormatting::Manifest,
			newline: "\n",
			key_val_sep: ": ",
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
			#[cfg(feature = "exp-bigint")]
			preserve_bigints: false,
			debug_truncate_strings: None,
		}
	}
	// Same format as CLI manifestification
	pub fn debug() -> Self {
		Self {
			padding: Cow::Borrowed("   "),
			mtype: JsonFormatting::Manifest,
			newline: "\n",
			key_val_sep: ": ",
			#[cfg(feature = "exp-preserve-order")]
			preserve_order: true,
			#[cfg(feature = "exp-bigint")]
			preserve_bigints: true,
			debug_truncate_strings: Some(256),
		}
	}
}
impl Default for JsonFormat<'static> {
	fn default() -> Self {
		Self {
			padding: Cow::Borrowed("    "),
			mtype: JsonFormatting::Manifest,
			newline: "\n",
			key_val_sep: ": ",
			#[cfg(feature = "exp-preserve-order")]
			preserve_order: false,
			#[cfg(feature = "exp-bigint")]
			preserve_bigints: false,
			debug_truncate_strings: None,
		}
	}
}

pub fn manifest_json_ex(val: &Val, options: &JsonFormat<'_>) -> Result<String> {
	let mut out = String::new();
	manifest_json_ex_buf(val, &mut out, &mut String::new(), options)?;
	Ok(out)
}

#[allow(clippy::too_many_lines)]
fn manifest_json_ex_buf(
	val: &Val,
	buf: &mut String,
	cur_padding: &mut String,
	options: &JsonFormat<'_>,
) -> Result<()> {
	use JsonFormatting::*;

	let mtype = options.mtype;
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
			let flat = s.clone().into_flat();
			if let Some(truncate) = options.debug_truncate_strings {
				if flat.len() > truncate {
					let (start, end) = flat.split_at(truncate / 2);
					let (_, end) = end.split_at(end.len() - truncate / 2);
					escape_string_json_buf(&format!("{start}..{end}"), buf);
				} else {
					escape_string_json_buf(&flat, buf);
				}
			} else {
				escape_string_json_buf(&flat, buf);
			}
		}
		Val::Num(n) => write!(buf, "{n}").unwrap(),
		#[cfg(feature = "exp-bigint")]
		Val::BigInt(n) => {
			if options.preserve_bigints {
				write!(buf, "{n}").unwrap();
			} else {
				write!(buf, "{:?}", n.to_string()).unwrap();
			}
		}
		Val::Arr(items) => {
			buf.push('[');

			let old_len = cur_padding.len();
			cur_padding.push_str(&options.padding);

			let mut had_items = false;
			for (i, item) in items.iter().enumerate() {
				had_items = true;
				let item = item.with_description(|| format!("elem <{i}> evaluation"))?;

				if i != 0 {
					buf.push(',');
				}
				match mtype {
					Manifest | Std => {
						buf.push_str(options.newline);
						buf.push_str(cur_padding);
					}
					ToString if i != 0 => buf.push(' '),
					Minify | ToString => {}
				}

				in_description_frame(
					|| format!("elem <{i}> manifestification"),
					|| manifest_json_ex_buf(&item, buf, cur_padding, options),
				)?;
			}

			cur_padding.truncate(old_len);

			match mtype {
				Manifest | ToString if !had_items => {
					// Empty array as "[ ]"
					buf.push(' ');
				}
				Manifest => {
					buf.push_str(options.newline);
					buf.push_str(cur_padding);
				}
				Std => {
					if !had_items {
						// Stdlib formats empty array as "[\n\n]"
						buf.push_str(options.newline);
					}
					buf.push_str(options.newline);
					buf.push_str(cur_padding);
				}
				Minify | ToString => {}
			}

			buf.push(']');
		}
		Val::Obj(obj) => {
			obj.run_assertions()?;
			buf.push('{');

			let old_len = cur_padding.len();
			cur_padding.push_str(&options.padding);

			let mut had_fields = false;
			for (i, (key, value)) in obj
				.iter(
					#[cfg(feature = "exp-preserve-order")]
					options.preserve_order,
				)
				.enumerate()
			{
				had_fields = true;
				let value = value.with_description(|| format!("field <{key}> evaluation"))?;

				if i != 0 {
					buf.push(',');
				}
				match mtype {
					Manifest | Std => {
						buf.push_str(options.newline);
						buf.push_str(cur_padding);
					}
					ToString if i != 0 => buf.push(' '),
					Minify | ToString => {}
				}

				escape_string_json_buf(&key, buf);
				buf.push_str(options.key_val_sep);
				in_description_frame(
					|| format!("field <{key}> manifestification"),
					|| manifest_json_ex_buf(&value, buf, cur_padding, options),
				)?;
			}

			cur_padding.truncate(old_len);

			match mtype {
				Manifest | ToString if !had_fields => {
					// Empty object as "{ }"
					buf.push(' ');
				}
				Manifest => {
					buf.push_str(options.newline);
					buf.push_str(cur_padding);
				}
				Std => {
					if !had_fields {
						// Stdlib formats empty object as "{\n\n}"
						buf.push_str(options.newline);
					}
					buf.push_str(options.newline);
					buf.push_str(cur_padding);
				}
				Minify | ToString => {}
			}

			buf.push('}');
		}
		Val::Func(_) => bail!("tried to manifest function"),
	}
	Ok(())
}

impl ManifestFormat for JsonFormat<'_> {
	fn manifest_buf(&self, val: Val, buf: &mut String) -> Result<()> {
		manifest_json_ex_buf(&val, buf, &mut String::new(), self)
	}
}

/// Same as [`JsonFormat`] with pre-set options, but top-level string is serialized as-is,
/// without quoting.
pub struct ToStringFormat;
impl ManifestFormat for ToStringFormat {
	fn manifest_buf(&self, val: Val, out: &mut String) -> Result<()> {
		const JSON_TO_STRING: JsonFormat = JsonFormat::std_to_string_helper();
		if let Some(str) = val.as_str() {
			out.push_str(&str);
			return Ok(());
		}
		#[cfg(feature = "exp-bigint")]
		if let Some(int) = val.as_bigint() {
			out.push_str(&int.to_str_radix(10));
			return Ok(());
		}
		JSON_TO_STRING.manifest_buf(val, out)
	}
	fn file_trailing_newline(&self) -> bool {
		false
	}
}
pub struct StringFormat;
impl ManifestFormat for StringFormat {
	fn manifest_buf(&self, val: Val, out: &mut String) -> Result<()> {
		let Val::Str(s) = val else {
			bail!(
				"output should be string for string manifest format, got {}",
				val.value_type()
			)
		};
		write!(out, "{s}").unwrap();
		Ok(())
	}
	fn file_trailing_newline(&self) -> bool {
		false
	}
}

pub struct YamlStreamFormat<I> {
	inner: I,
	c_document_end: bool,
	end_newline: bool,
}
impl<I> YamlStreamFormat<I> {
	pub fn std_yaml_stream(inner: I, c_document_end: bool) -> Self {
		Self {
			inner,
			c_document_end,
			// Stdlib format always inserts useless newline at the end
			end_newline: true,
		}
	}
	pub fn cli(inner: I) -> Self {
		Self {
			inner,
			c_document_end: true,
			end_newline: false,
		}
	}
}
impl<I: ManifestFormat> ManifestFormat for YamlStreamFormat<I> {
	fn manifest_buf(&self, val: Val, out: &mut String) -> Result<()> {
		let Val::Arr(arr) = val else {
			bail!(
				"output should be array for yaml stream format, got {}",
				val.value_type()
			)
		};
		for (i, v) in arr.iter().enumerate() {
			if i != 0 {
				out.push('\n');
			}
			let v = v.with_description(|| format!("elem <{i}> evaluation"))?;
			out.push_str("---\n");
			in_description_frame(
				|| format!("elem <{i}> manifestification"),
				|| self.inner.manifest_buf(v, out),
			)?;
		}
		if self.c_document_end {
			out.push('\n');
			out.push_str("...");
		}
		if self.end_newline {
			out.push('\n');
		}
		Ok(())
	}

	fn manifest_write(&self, val: Val, out: &mut dyn io::Write) -> Result<bool> {
		let Val::Arr(arr) = val else {
			bail!(
				"output should be array for yaml stream format, got {}",
				val.value_type()
			)
		};
		let mut buf = String::new();
		for (i, v) in arr.iter().enumerate() {
			let v = v.with_description(|| format!("elem <{i}> evaluation"))?;
			buf.clear();
			if i != 0 {
				buf.push('\n');
			}
			buf.push_str("---\n");
			in_description_frame(
				|| format!("elem <{i}> manifestification"),
				|| self.inner.manifest_buf(v, &mut buf),
			)?;
			write_io(out, buf.as_bytes())?;
		}
		if self.c_document_end {
			write_io(out, b"\n...")?;
		}
		if self.end_newline {
			write_io(out, b"\n")?;
		}
		Ok(true)
	}
}

fn write_io(out: &mut dyn io::Write, data: &[u8]) -> Result<()> {
	out.write_all(data)
		.map_err(|e| crate::error::ErrorKind::ImportIo(e.to_string()))?;
	Ok(())
}

pub fn escape_string_json(s: &str) -> String {
	let mut buf = String::new();
	escape_string_json_buf(s, &mut buf);
	buf
}

// Json string encoding was borrowed from https://github.com/serde-rs/json

const BB: u8 = b'b'; // \x08
const TT: u8 = b't'; // \x09
const NN: u8 = b'n'; // \x0A
const FF: u8 = b'f'; // \x0C
const RR: u8 = b'r'; // \x0D
const QU: u8 = b'"'; // \x22
const BS: u8 = b'\\'; // \x5C
const UU: u8 = b'u'; // \x00...\x1F except the ones above
const __: u8 = 0;

// Lookup table of escape sequences. A value of b'x' at index i means that byte
// i is escaped as "\x" in JSON. A value of 0 means that byte i is not escaped.
static ESCAPE: [u8; 256] = [
	//   1   2   3   4   5   6   7   8   9   A   B   C   D   E   F
	UU, UU, UU, UU, UU, UU, UU, UU, BB, TT, NN, UU, FF, RR, UU, UU, // 0
	UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, UU, // 1
	__, __, QU, __, __, __, __, __, __, __, __, __, __, __, __, __, // 2
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 3
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 4
	__, __, __, __, __, __, __, __, __, __, __, __, BS, __, __, __, // 5
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 6
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 7
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 8
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // 9
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // A
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // B
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // C
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // D
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // E
	__, __, __, __, __, __, __, __, __, __, __, __, __, __, __, __, // F
];

pub fn escape_string_json_buf(value: &str, buf: &mut String) {
	// Safety: we only write correct utf-8 in this function
	let buf: &mut Vec<u8> = unsafe { &mut *ptr::from_mut(buf).cast::<Vec<u8>>() };
	let bytes = value.as_bytes();

	// Perfect for ascii strings, removes any reallocations
	buf.reserve(value.len() + 2);

	buf.push(b'"');

	let mut start = 0;

	for (i, &byte) in bytes.iter().enumerate() {
		let escape = ESCAPE[byte as usize];
		if escape == __ {
			continue;
		}

		if start < i {
			buf.extend_from_slice(&bytes[start..i]);
		}
		start = i + 1;

		match escape {
			self::BB | self::TT | self::NN | self::FF | self::RR | self::QU | self::BS => {
				buf.extend_from_slice(&[b'\\', escape]);
			}
			self::UU => {
				static HEX_DIGITS: [u8; 16] = *b"0123456789abcdef";
				let bytes = &[
					b'\\',
					b'u',
					b'0',
					b'0',
					HEX_DIGITS[(byte >> 4) as usize],
					HEX_DIGITS[(byte & 0xF) as usize],
				];
				buf.extend_from_slice(bytes);
			}
			_ => unreachable!(),
		}
	}

	if start == bytes.len() {
		buf.push(b'"');
		return;
	}

	buf.extend_from_slice(&bytes[start..]);
	buf.push(b'"');
}
