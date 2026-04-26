use base64::{Engine, engine::general_purpose::STANDARD};
use jrsonnet_evaluator::{
	IBytes, IStr, Result, bail,
	function::builtin,
	runtime_error,
	typed::{Either, Either2},
};

#[builtin]
pub fn builtin_encode_utf8(str: IStr) -> IBytes {
	str.cast_bytes()
}

#[builtin]
pub fn builtin_decode_utf8(arr: IBytes, #[default(true)] lossy: bool) -> Result<IStr> {
	match arr.clone().cast_str() {
		Some(s) => Ok(s),
		None if lossy => Ok(String::from_utf8_lossy(arr.as_slice()).into()),
		None => {
			bail!("bad utf8")
		}
	}
}

#[builtin]
pub fn builtin_base64(input: Either![IStr, IBytes]) -> String {
	use Either2::*;
	match input {
		A(l) => STANDARD.encode(l.as_bytes()),
		B(a) => STANDARD.encode(a.as_slice()),
	}
}

#[builtin]
pub fn builtin_base64_decode_bytes(str: IStr) -> Result<IBytes> {
	Ok(STANDARD
		.decode(str.as_bytes())
		.map_err(|e| runtime_error!("invalid base64: {e}"))?
		.as_slice()
		.into())
}

#[builtin]
pub fn builtin_base64_decode(str: IStr, #[default(false)] lossy: bool) -> Result<String> {
	let bytes = STANDARD
		.decode(str.as_bytes())
		.map_err(|e| runtime_error!("invalid base64: {e}"))?;
	if lossy {
		Ok(String::from_utf8_lossy(&bytes).to_string())
	} else {
		String::from_utf8(bytes).map_err(|e| runtime_error!("bad utf8: {e}"))
	}
}
