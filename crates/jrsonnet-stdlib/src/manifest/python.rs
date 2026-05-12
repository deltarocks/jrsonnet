use jrsonnet_evaluator::{
	Result, Val, bail,
	manifest::{ManifestFormat, ToStringFormat, escape_string_json_buf},
};

pub struct PythonFormat {
	#[cfg(feature = "exp-preserve-order")]
	preserve_order: bool,
}

impl PythonFormat {
	pub fn std(#[cfg(feature = "exp-preserve-order")] preserve_order: bool) -> Self {
		Self {
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
		}
	}
}

impl ManifestFormat for PythonFormat {
	fn manifest_buf(&self, val: &Val, buf: &mut String) -> Result<()> {
		match val {
			Val::Bool(true) => buf.push_str("True"),
			Val::Bool(false) => buf.push_str("False"),
			Val::Null => buf.push_str("None"),
			Val::Str(s) => escape_string_json_buf(&s.to_string(), buf),
			Val::Num(_) => ToStringFormat.manifest_buf(val, buf)?,
			#[cfg(feature = "exp-bigint")]
			Val::BigInt(_) => ToStringFormat.manifest_buf(val, buf)?,
			Val::Arr(arr) => {
				buf.push('[');
				for (i, el) in arr.iter().enumerate() {
					let el = el?;
					if i != 0 {
						buf.push_str(", ");
					}
					self.manifest_buf(&el, buf)?;
				}
				buf.push(']');
			}
			Val::Obj(obj) => {
				obj.run_assertions()?;
				buf.push('{');
				let fields = obj.fields(
					#[cfg(feature = "exp-preserve-order")]
					self.preserve_order,
				);
				for (i, field) in fields.into_iter().enumerate() {
					if i != 0 {
						buf.push_str(", ");
					}
					escape_string_json_buf(&field, buf);
					buf.push_str(": ");
					let value = obj.get(field)?.expect("field exists");
					self.manifest_buf(&value, buf)?;
				}
				buf.push('}');
			}
			Val::Func(_) => bail!("tried to manifest function"),
		}
		Ok(())
	}
}

pub struct PythonVarsFormat {
	#[cfg(feature = "exp-preserve-order")]
	preserve_order: bool,
}

impl PythonVarsFormat {
	pub fn std(#[cfg(feature = "exp-preserve-order")] preserve_order: bool) -> Self {
		Self {
			#[cfg(feature = "exp-preserve-order")]
			preserve_order,
		}
	}
}

impl ManifestFormat for PythonVarsFormat {
	fn manifest_buf(&self, val: &Val, buf: &mut String) -> Result<()> {
		let inner = PythonFormat {
			#[cfg(feature = "exp-preserve-order")]
			preserve_order: self.preserve_order,
		};
		let Val::Obj(obj) = val else {
			bail!("python vars root should be object");
		};
		obj.run_assertions()?;

		let fields = obj.fields(
			#[cfg(feature = "exp-preserve-order")]
			self.preserve_order,
		);

		for field in fields {
			// Yep, no escaping
			buf.push_str(&field);
			buf.push_str(" = ");
			inner.manifest_buf(&obj.get(field)?.expect("field exists"), buf)?;
			buf.push('\n');
		}
		Ok(())
	}
}
