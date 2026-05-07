use jrsonnet_evaluator::{
	Either, ObjValue, Result, ResultExt, Val, bail, in_description_frame,
	manifest::{ManifestFormat, ToStringFormat},
	typed::{ComplexValType, Either2, FromUntyped, Typed, ValType},
	val::ArrValue,
};

pub struct XmlJsonmlFormat {
	force_closing: bool,
}
impl XmlJsonmlFormat {
	pub fn std_to_xml() -> Self {
		Self {
			force_closing: true,
		}
	}
	pub fn cli() -> Self {
		Self {
			force_closing: false,
		}
	}
}

enum JSONMLValue {
	Tag {
		tag: String,
		attrs: ObjValue,
		children: Vec<JSONMLValue>,
	},
	String(String),
}
impl Typed for JSONMLValue {
	const TYPE: &'static ComplexValType = &ComplexValType::Simple(ValType::Arr);
}
impl FromUntyped for JSONMLValue {
	fn from_untyped(untyped: Val) -> Result<Self> {
		let val = <Either![ArrValue, String]>::from_untyped(untyped)
			.description("parsing JSONML value (an array or string)")?;
		let arr = match val {
			Either2::A(a) => a,
			Either2::B(s) => return Ok(Self::String(s)),
		};
		if arr.is_empty() {
			bail!("JSONML value should have tag (array length should be >=1)");
		}
		let tag = String::from_untyped(
			arr.get32(0)
				.description("getting JSONML tag")?
				.expect("length checked"),
		)
		.description("parsing JSONML tag")?;

		let (has_attrs, attrs) = if arr.len32() >= 2 {
			let maybe_attrs = arr
				.get32(1)
				.with_description(|| "getting JSONML attrs")?
				.expect("length checked");
			if let Val::Obj(attrs) = maybe_attrs {
				(true, attrs)
			} else {
				(false, ObjValue::empty())
			}
		} else {
			(false, ObjValue::empty())
		};
		Ok(Self::Tag {
			tag,
			attrs,
			children: in_description_frame(
				|| "parsing children".to_owned(),
				|| FromUntyped::from_untyped(Val::Arr(arr.slice(if has_attrs { 2 } else { 1 }..))),
			)?,
		})
	}
}

impl ManifestFormat for XmlJsonmlFormat {
	fn manifest_buf(&self, val: Val, buf: &mut String) -> Result<()> {
		let val = JSONMLValue::from_untyped(val).with_description(|| "parsing JSONML value")?;
		manifest_jsonml(&val, buf, self)
	}
}

fn manifest_jsonml(v: &JSONMLValue, buf: &mut String, opts: &XmlJsonmlFormat) -> Result<()> {
	match v {
		JSONMLValue::Tag {
			tag,
			attrs,
			children,
		} => {
			let has_children = !children.is_empty();
			buf.push('<');
			buf.push_str(tag);
			attrs.run_assertions()?;
			for (key, value) in attrs.iter(
				// Not much sense to preserve order here
				#[cfg(feature = "exp-preserve-order")]
				false,
			) {
				buf.push(' ');
				buf.push_str(&key);
				buf.push('=');
				buf.push('"');
				let value = value?;
				let value = if let Val::Str(s) = value {
					s.to_string()
				} else {
					ToStringFormat.manifest(value)?
				};
				escape_string_xml_buf(&value, buf);
				buf.push('"');
			}
			if !has_children && !opts.force_closing {
				buf.push('/');
			}
			buf.push('>');
			for child in children {
				manifest_jsonml(child, buf, opts)?;
			}
			if has_children || opts.force_closing {
				buf.push('<');
				buf.push('/');
				buf.push_str(tag);
				buf.push('>');
			}
			Ok(())
		}
		JSONMLValue::String(s) => {
			escape_string_xml_buf(s, buf);
			Ok(())
		}
	}
}

pub fn escape_string_xml(str: &str) -> String {
	let mut out = String::new();
	escape_string_xml_buf(str, &mut out);
	out
}

fn escape_string_xml_buf(str: &str, out: &mut String) {
	if str.is_empty() {
		return;
	}
	let mut remaining = str;

	let mut found = false;
	while let Some(position) = remaining
		.bytes()
		.position(|c| matches!(c, b'<' | b'>' | b'&' | b'"' | b'\''))
	{
		found = true;

		let (plain, rem) = remaining.split_at(position);
		out.push_str(plain);

		out.push_str(match rem.as_bytes()[0] {
			b'<' => "&lt;",
			b'>' => "&gt;",
			b'&' => "&amp;",
			b'"' => "&quot;",
			b'\'' => "&apos;",
			_ => unreachable!("position() searches for those matches"),
		});

		remaining = &rem[1..];
	}
	if !found {
		// No match - no escapes required
		out.push_str(str);
		return;
	}
	out.push_str(remaining);
}
