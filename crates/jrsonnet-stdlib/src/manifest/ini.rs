use std::collections::BTreeMap;

use jrsonnet_evaluator::{
	IStr, ObjValue, Result, ResultExt, Val,
	manifest::{ManifestFormat, ToStringFormat},
	typed::{FromUntyped, Typed},
};

pub struct IniFormat {
	#[cfg(feature = "exp-preserve-order")]
	preserve_order: bool,
	final_newline: bool,
}

impl IniFormat {
	pub fn std(#[cfg(feature = "exp-preserve-order")] preserve_order: bool) -> Self {
		Self {
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
			final_newline: true,
		}
	}
	pub fn cli(#[cfg(feature = "exp-preserve-order")] preserve_order: bool) -> Self {
		Self {
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
			final_newline: false,
		}
	}
}

impl ManifestFormat for IniFormat {
	fn manifest_buf(&self, val: &Val, buf: &mut String) -> Result<()> {
		manifest_ini_obj(
			self,
			IniObj::from_untyped(val.clone()).description("ini object structure")?,
			buf,
		)
	}
}

fn manifest_ini_body(
	#[cfg(feature = "exp-preserve-order")] format: &IniFormat,
	body: ObjValue,
	out: &mut String,
) -> Result<()> {
	for (i, (key, value)) in body
		.iter(
			#[cfg(feature = "exp-preserve-order")]
			format.preserve_order,
		)
		.enumerate()
	{
		if i != 0 || !out.is_empty() {
			out.push('\n');
		}
		let value = value.with_description(|| format!("field <{key}> evaluation"))?;
		let manifest_desc = || format!("field <{key}> manifestification");
		if let Some(arr) = value.as_arr() {
			for (i, ele) in arr.iter().enumerate() {
				if i != 0 {
					out.push('\n');
				}
				let ele = ele
					.with_description(|| format!("elem <{i}> evaluation"))
					.with_description(manifest_desc)?;
				out.push_str(&key);
				out.push_str(" = ");
				ToStringFormat
					.manifest_buf(&ele, out)
					.with_description(manifest_desc)?;
			}
		} else {
			out.push_str(&key);
			out.push_str(" = ");
			ToStringFormat
				.manifest_buf(&value, out)
				.with_description(manifest_desc)?;
		}
	}
	Ok(())
}

#[derive(Typed, FromUntyped)]
struct IniObj {
	main: Option<ObjValue>,
	// TODO: Preserve section order?
	sections: BTreeMap<IStr, ObjValue>,
}

fn manifest_ini_obj(format: &IniFormat, obj: IniObj, out: &mut String) -> Result<()> {
	if let Some(main) = obj.main {
		manifest_ini_body(
			#[cfg(feature = "exp-preserve-order")]
			format,
			main,
			out,
		)
		.description("<main> manifestification")?;
	}
	for (i, (section, val)) in obj.sections.into_iter().enumerate() {
		if i != 0 || !out.is_empty() {
			out.push('\n');
		}
		out.push('[');
		out.push_str(&section);
		out.push(']');
		manifest_ini_body(
			#[cfg(feature = "exp-preserve-order")]
			format,
			val,
			out,
		)
		.with_description(|| format!("<{section}> section manifestification"))?;
	}
	if format.final_newline {
		out.push('\n');
	}
	Ok(())
}
