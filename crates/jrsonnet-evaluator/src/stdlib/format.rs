//! faster std.format impl
#![allow(clippy::too_many_arguments)]
#![expect(
	clippy::cast_possible_truncation,
	clippy::cast_sign_loss,
	reason = "many safe integer casts, behavior on overflow is not specified"
)]

use std::fmt::Write as _;

use jrsonnet_gcmodule::Trace;
use jrsonnet_interner::IStr;
use jrsonnet_types::ValType;
use num_bigint::BigUint;
use thiserror::Error;

use crate::{
	Error, ObjValue, Result, Val, bail,
	error::{ErrorKind::*, format_found, suggest_object_fields},
	typed::{Codepoint, FromUntyped},
};

#[derive(Debug, Clone, Error, Trace)]
pub enum FormatError {
	#[error("truncated format code")]
	TruncatedFormatCode,
	#[error("unrecognized conversion type: {0}")]
	UnrecognizedConversionType(char),

	#[error("not enough values")]
	NotEnoughValues,

	#[error("cannot use * width with object")]
	CannotUseStarWidthWithObject,
	#[error("mapping keys required")]
	MappingKeysRequired,
	#[error("no such format field: {0}")]
	NoSuchFormatField(IStr),

	#[error("expected subfield <{0}> to be an object, got {1} instead")]
	SubfieldDidntYieldAnObject(IStr, ValType),
	#[error("subfield not found: <[{full}]{current}>{}", format_found(.found, "subfield"))]
	SubfieldNotFound {
		current: IStr,
		full: IStr,
		found: Box<Vec<IStr>>,
	},
}

impl From<FormatError> for Error {
	fn from(e: FormatError) -> Self {
		Self::new(Format(e))
	}
}

use FormatError::*;

type ParseResult<'t, T> = std::result::Result<(T, &'t str), FormatError>;

pub fn try_parse_mapping_key(str: &str) -> ParseResult<'_, &str> {
	if str.is_empty() {
		return Err(TruncatedFormatCode);
	}
	let bytes = str.as_bytes();
	if bytes[0] == b'(' {
		let mut i = 1;
		while i < bytes.len() {
			if bytes[i] == b')' {
				return Ok((&str[1..i], &str[i + 1..]));
			}
			i += 1;
		}
		Err(TruncatedFormatCode)
	} else {
		Ok(("", str))
	}
}

#[cfg(test)]
pub mod tests_key {
	use super::*;

	#[test]
	fn parse_key() {
		assert_eq!(
			try_parse_mapping_key("(hello ) world").unwrap(),
			("hello ", " world")
		);
		assert_eq!(try_parse_mapping_key("() world").unwrap(), ("", " world"));
		assert_eq!(try_parse_mapping_key(" world").unwrap(), ("", " world"));
		assert_eq!(
			try_parse_mapping_key(" () world").unwrap(),
			("", " () world")
		);
	}

	#[test]
	#[should_panic = "TruncatedFormatCode"]
	fn parse_key_missing_start() {
		try_parse_mapping_key("").unwrap();
	}

	#[test]
	#[should_panic = "TruncatedFormatCode"]
	fn parse_key_missing_end() {
		try_parse_mapping_key("(   ").unwrap();
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Default, Debug)]
pub struct CFlags {
	pub alt: bool,
	pub zero: bool,
	pub left: bool,
	pub blank: bool,
	pub sign: bool,
}

pub fn try_parse_cflags(str: &str) -> ParseResult<'_, CFlags> {
	if str.is_empty() {
		return Err(TruncatedFormatCode);
	}
	let bytes = str.as_bytes();
	let mut i = 0;
	let mut out = CFlags::default();
	loop {
		if bytes.len() == i {
			return Err(TruncatedFormatCode);
		}
		match bytes[i] {
			b'#' => out.alt = true,
			b'0' => out.zero = true,
			b'-' => out.left = true,
			b' ' => out.blank = true,
			b'+' => out.sign = true,
			_ => break,
		}
		i += 1;
	}
	Ok((out, &str[i..]))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Width {
	Star,
	Fixed(u16),
}
pub fn try_parse_field_width(str: &str) -> ParseResult<'_, Width> {
	if str.is_empty() {
		return Err(TruncatedFormatCode);
	}
	let bytes = str.as_bytes();
	if bytes[0] == b'*' {
		return Ok((Width::Star, &str[1..]));
	}
	let mut out: u16 = 0;
	let mut digits = 0;
	while let Some(digit) = (bytes[digits] as char).to_digit(10) {
		out *= 10;
		out += digit as u16;
		digits += 1;
		if digits == bytes.len() {
			return Err(TruncatedFormatCode);
		}
	}
	Ok((Width::Fixed(out), &str[digits..]))
}

pub fn try_parse_precision(str: &str) -> ParseResult<'_, Option<Width>> {
	if str.is_empty() {
		return Err(TruncatedFormatCode);
	}
	let bytes = str.as_bytes();
	if bytes[0] == b'.' {
		try_parse_field_width(&str[1..]).map(|(r, s)| (Some(r), s))
	} else {
		Ok((None, str))
	}
}

// Only skips
pub fn try_parse_length_modifier(str: &str) -> ParseResult<'_, ()> {
	if str.is_empty() {
		return Err(TruncatedFormatCode);
	}
	let bytes = str.as_bytes();
	let mut idx = 0;
	while bytes[idx] == b'h' || bytes[idx] == b'l' || bytes[idx] == b'L' {
		idx += 1;
		if bytes.len() == idx {
			return Err(TruncatedFormatCode);
		}
	}
	Ok(((), &str[idx..]))
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConvTypeV {
	Decimal,
	Octal,
	Hexadecimal,
	Scientific,
	Float,
	Shorter,
	Char,
	String,
	Percent,
}
pub struct ConvType {
	v: ConvTypeV,
	caps: bool,
}

pub fn parse_conversion_type(str: &str) -> ParseResult<'_, ConvType> {
	if str.is_empty() {
		return Err(TruncatedFormatCode);
	}

	let code = str.as_bytes()[0];
	let v: (ConvTypeV, bool) = match code {
		b'd' | b'i' | b'u' => (ConvTypeV::Decimal, false),
		b'o' => (ConvTypeV::Octal, false),
		b'x' => (ConvTypeV::Hexadecimal, false),
		b'X' => (ConvTypeV::Hexadecimal, true),
		b'e' => (ConvTypeV::Scientific, false),
		b'E' => (ConvTypeV::Scientific, true),
		b'f' => (ConvTypeV::Float, false),
		b'F' => (ConvTypeV::Float, true),
		b'g' => (ConvTypeV::Shorter, false),
		b'G' => (ConvTypeV::Shorter, true),
		b'c' => (ConvTypeV::Char, false),
		b's' => (ConvTypeV::String, false),
		b'%' => (ConvTypeV::Percent, false),
		c => return Err(UnrecognizedConversionType(c as char)),
	};

	Ok((ConvType { v: v.0, caps: v.1 }, &str[1..]))
}

#[derive(Debug)]
pub struct Code<'s> {
	mkey: &'s str,
	cflags: CFlags,
	width: Width,
	precision: Option<Width>,
	convtype: ConvTypeV,
	caps: bool,
}
pub fn parse_code(str: &str) -> ParseResult<'_, Code<'_>> {
	if str.is_empty() {
		return Err(TruncatedFormatCode);
	}
	let (mkey, str) = try_parse_mapping_key(str)?;
	let (cflags, str) = try_parse_cflags(str)?;
	let (width, str) = try_parse_field_width(str)?;
	let (precision, str) = try_parse_precision(str)?;
	let ((), str) = try_parse_length_modifier(str)?;
	let (convtype, str) = parse_conversion_type(str)?;

	Ok((
		Code {
			mkey,
			cflags,
			width,
			precision,
			convtype: convtype.v,
			caps: convtype.caps,
		},
		str,
	))
}

#[derive(Debug)]
pub enum Element<'s> {
	String(&'s str),
	Code(Code<'s>),
}
pub fn parse_codes(mut str: &str) -> Result<Vec<Element<'_>>> {
	let mut bytes = str.as_bytes();
	let mut out = vec![];
	let mut offset = 0;

	loop {
		while offset != bytes.len() && bytes[offset] != b'%' {
			offset += 1;
		}
		if offset != 0 {
			out.push(Element::String(&str[0..offset]));
		}
		if offset == bytes.len() {
			return Ok(out);
		}
		str = &str[offset + 1..];
		let code;
		(code, str) = parse_code(str)?;
		bytes = str.as_bytes();
		offset = 0;

		out.push(Element::Code(code));
	}
}

const NUMBERS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

#[allow(clippy::cast_precision_loss, reason = "fits exactly")]
const TWO_POW_64: f64 = (1u128 << 64) as f64;

#[inline]
#[allow(clippy::fn_params_excessive_bools)]
pub fn render_integer(
	out: &mut String,
	neg: bool,
	iv: f64,
	padding: u16,
	precision: u16,
	blank: bool,
	sign: bool,
	radix: i64,
	zero_prefix: &str,
	prefix_in_padding: bool,
	caps: bool,
) {
	debug_assert!(iv >= 0.0, "render_integer receives sign using arg");
	let iv = iv.floor();
	// Digit char indexes in reverse order, i.e
	// for radix = 16 and n = 12f: [15, 2, 1]
	let digits = if iv == 0.0 {
		vec![0u8]
	} else if iv < TWO_POW_64 {
		let mut v = iv as u64;
		let radix = radix as u64;
		let mut nums = Vec::with_capacity(1);
		while v != 0 {
			nums.push((v % radix) as u8);
			v /= radix;
		}
		nums
	} else {
		let (mantissa, exp) = decompose_f64(iv);
		let mut v = BigUint::from(mantissa) << exp.unsigned_abs();
		let radix = BigUint::from(radix as u64);
		let mut nums = Vec::new();
		while v != BigUint::ZERO {
			nums.push(u8::try_from(&v % &radix).expect("digit < radix"));
			v /= &radix;
		}
		nums
	};
	#[allow(clippy::bool_to_int_with_if)]
	let zp = padding.saturating_sub(if neg || blank || sign { 1 } else { 0 });

	let pref_len = zero_prefix.len() as u16;
	let zp2 = zp
		.saturating_sub(if prefix_in_padding { 0 } else { pref_len })
		.max(precision)
		.saturating_sub(if prefix_in_padding { pref_len } else { 0 } + digits.len() as u16);

	if neg {
		out.push('-');
	} else if sign {
		out.push('+');
	} else if blank {
		out.push(' ');
	}

	out.reserve(zp2 as usize);
	if iv != 0.0 {
		out.push_str(zero_prefix);
	}
	for _ in 0..zp2 {
		out.push('0');
	}

	for digit in digits.into_iter().rev() {
		let ch = NUMBERS[digit as usize] as char;
		out.push(if caps { ch.to_ascii_uppercase() } else { ch });
	}
}

pub fn render_decimal(
	out: &mut String,
	neg: bool,
	iv: f64,
	padding: u16,
	precision: u16,
	blank: bool,
	sign: bool,
) {
	render_integer(
		out, neg, iv, padding, precision, blank, sign, 10, "", false, false,
	);
}
#[allow(clippy::fn_params_excessive_bools)]
pub fn render_octal(
	out: &mut String,
	neg: bool,
	iv: f64,
	padding: u16,
	precision: u16,
	alt: bool,
	blank: bool,
	sign: bool,
) {
	render_integer(
		out,
		neg,
		iv,
		padding,
		precision,
		blank,
		sign,
		8,
		if alt && iv != 0.0 { "0" } else { "" },
		true,
		false,
	);
}

#[allow(clippy::fn_params_excessive_bools)]
pub fn render_hexadecimal(
	out: &mut String,
	iv: f64,
	padding: u16,
	precision: u16,
	alt: bool,
	blank: bool,
	sign: bool,
	caps: bool,
) {
	render_integer(
		out,
		iv < 0.0,
		iv.abs(),
		padding,
		precision,
		blank,
		sign,
		16,
		match (alt, caps) {
			(true, true) => "0X",
			(true, false) => "0x",
			(false, _) => "",
		},
		false,
		caps,
	);
}

#[allow(clippy::fn_params_excessive_bools)]
pub fn render_float(
	out: &mut String,
	n: f64,
	mut padding: u16,
	precision: u16,
	blank: bool,
	sign: bool,
	ensure_pt: bool,
	trailing: bool,
) {
	let abs = n.abs();
	let (frac, carry) = round_frac(abs, precision);
	let whole = if carry {
		abs.floor() + 1.0
	} else {
		abs.floor()
	};

	#[allow(clippy::bool_to_int_with_if)]
	let dot_size = if precision == 0 && !ensure_pt { 0 } else { 1 };
	padding = padding.saturating_sub(dot_size + precision);
	render_decimal(out, n < 0.0, whole, padding, 0, blank, sign);

	if precision == 0 {
		if ensure_pt {
			out.push('.');
		}
		return;
	}

	// For %g (trailing == false) trailing zeros are then trimmed in place
	let dot_at = out.len();
	out.push('.');
	let digits_at = out.len();
	frac.write_padded(out, precision);
	if !trailing {
		let kept = digits_at + out[digits_at..].trim_end_matches('0').len();
		out.truncate(kept);
		if out.len() == digits_at && !ensure_pt {
			out.truncate(dot_at);
		}
	}
}

fn decompose_f64(n: f64) -> (u64, i32) {
	const BITS: u32 = f64::MANTISSA_DIGITS - 1;
	const BIAS: i32 = 1023;

	let pattern = n.to_bits();
	let exp_field = ((pattern >> BITS) & 0x7ff) as i32;
	let mantissa = pattern & ((1u64 << BITS) - 1);
	if exp_field == 0 {
		(mantissa, 1 - BIAS - BITS.cast_signed())
	} else {
		(
			mantissa | (1u64 << BITS),
			exp_field - BIAS - BITS.cast_signed(),
		)
	}
}

enum Frac {
	Small(u128),
	Big(BigUint),
}
impl Frac {
	fn write_padded(&self, out: &mut String, width: u16) {
		let width = usize::from(width);
		match self {
			Self::Small(v) => write!(out, "{v:0width$}"),
			Self::Big(v) => write!(out, "{v:0width$}"),
		}
		.expect("writing to a String never fails");
	}
}

/// bool signifies carry, e.g 0.999 at precision 2 results in 1.000, thus carry.
// floor((2 * low_bits * 10**prec + 2**k) / 2**(k+1))
fn round_frac(abs: f64, prec: u16) -> (Frac, bool) {
	let (mantissa, exp) = decompose_f64(abs);
	if exp >= 0 {
		// abs is integral: no fractional bits.
		return (Frac::Small(0), false);
	}
	let k = exp.unsigned_abs();
	// Fractional part is exactly `low_bits / 2**k` (mantissa is 53 bits).
	let low_bits = if k >= 64 {
		mantissa
	} else {
		mantissa & ((1u64 << k) - 1)
	};
	if low_bits == 0 {
		return (Frac::Small(0), false);
	}
	round_frac_u128(low_bits, k, prec).unwrap_or_else(|| round_frac_big(low_bits, k, prec))
}

fn round_frac_u128(low_bits: u64, k: u32, prec: u16) -> Option<(Frac, bool)> {
	if k > 126 {
		return None;
	}
	let ten_pow = 10u128.checked_pow(u32::from(prec))?;
	let numerator = u128::from(low_bits)
		.checked_mul(ten_pow)?
		.checked_mul(2)?
		.checked_add(1u128 << k)?;
	let frac = numerator >> (k + 1);
	Some(if frac == ten_pow {
		(Frac::Small(0), true)
	} else {
		(Frac::Small(frac), false)
	})
}

fn round_frac_big(low_bits: u64, k: u32, prec: u16) -> (Frac, bool) {
	let ten_pow = BigUint::from(10u32).pow(u32::from(prec));
	let frac = round_ratio(
		&(BigUint::from(low_bits) * &ten_pow),
		&(BigUint::from(1u32) << k),
	);
	if frac == ten_pow {
		(Frac::Small(0), true)
	} else {
		(Frac::Big(frac), false)
	}
}

fn round_ratio(num: &BigUint, den: &BigUint) -> BigUint {
	((num << 1u32) + den) / (den << 1u32)
}

fn shorter_exponent(value: f64, sig: u16) -> i32 {
	if value == 0.0 {
		return 0;
	}
	let abs = value.abs();
	let e0 = abs.log10().floor() as i32;
	if rounds_up_to_next_pow10(abs, sig, e0) {
		e0 + 1
	} else {
		e0
	}
}

fn rounds_up_to_next_pow10(abs: f64, sig: u16, e0: i32) -> bool {
	let (mantissa, exp) = decompose_f64(abs);
	// abs * 10**(sig-1-e0) == num / den
	let t = i32::from(sig) - 1 - e0;
	let mut num = BigUint::from(mantissa);
	let mut den = BigUint::from(1u32);
	let exp_abs = exp.unsigned_abs();
	if exp >= 0 {
		num <<= exp_abs;
	} else {
		den <<= exp_abs;
	}
	let pow10_t_abs = BigUint::from(10u32).pow(t.unsigned_abs());
	if t >= 0 {
		num *= pow10_t_abs;
	} else {
		den *= pow10_t_abs;
	}
	round_ratio(&num, &den) >= BigUint::from(10u32).pow(u32::from(sig))
}

#[allow(clippy::fn_params_excessive_bools)]
pub fn render_float_sci(
	out: &mut String,
	n: f64,
	mut padding: u16,
	precision: u16,
	blank: bool,
	sign: bool,
	ensure_pt: bool,
	trailing: bool,
	caps: bool,
) {
	let exponent = if n == 0.0 {
		0
	} else {
		shorter_exponent(n, precision.saturating_add(1))
	};

	let mantissa = if exponent == -324 {
		n * 10.0 / 10.0_f64.powi(exponent + 1)
	} else {
		n / 10.0_f64.powi(exponent)
	};
	let mut exponent_str = String::new();
	render_decimal(
		&mut exponent_str,
		exponent < 0,
		f64::from(exponent.abs()),
		3,
		0,
		false,
		true,
	);

	// +1 for e
	padding = padding.saturating_sub(exponent_str.len() as u16 + 1);

	render_float(
		out, mantissa, padding, precision, blank, sign, ensure_pt, trailing,
	);
	out.push(if caps { 'E' } else { 'e' });
	out.push_str(&exponent_str);
}

#[allow(clippy::too_many_lines)]
pub fn format_code(
	out: &mut String,
	value: &Val,
	code: &Code<'_>,
	width: u16,
	precision: Option<u16>,
) -> Result<()> {
	let clfags = &code.cflags;
	let (fpprec, iprec) = precision.map_or((6, 0), |v| (v, v));
	let padding = if clfags.zero && !clfags.left {
		width
	} else {
		0
	};

	// TODO: If left padded, can optimize by writing directly to out
	let mut tmp_out = String::new();

	match code.convtype {
		ConvTypeV::String => tmp_out.push_str(&value.clone().to_string()?),
		ConvTypeV::Decimal => {
			let value = f64::from_untyped(value.clone())?;
			render_decimal(
				&mut tmp_out,
				value <= -1.0,
				value.abs(),
				padding,
				iprec,
				clfags.blank,
				clfags.sign,
			);
		}
		ConvTypeV::Octal => {
			let value = f64::from_untyped(value.clone())?;
			render_octal(
				&mut tmp_out,
				value <= -1.0,
				value.abs(),
				padding,
				iprec,
				clfags.alt,
				clfags.blank,
				clfags.sign,
			);
		}
		ConvTypeV::Hexadecimal => {
			let value = f64::from_untyped(value.clone())?;
			render_hexadecimal(
				&mut tmp_out,
				value,
				padding,
				iprec,
				clfags.alt,
				clfags.blank,
				clfags.sign,
				code.caps,
			);
		}
		ConvTypeV::Scientific => {
			let value = f64::from_untyped(value.clone())?;
			render_float_sci(
				&mut tmp_out,
				value,
				padding,
				fpprec,
				clfags.blank,
				clfags.sign,
				clfags.alt,
				true,
				code.caps,
			);
		}
		ConvTypeV::Float => {
			let value = f64::from_untyped(value.clone())?;
			render_float(
				&mut tmp_out,
				value,
				padding,
				fpprec,
				clfags.blank,
				clfags.sign,
				clfags.alt,
				true,
			);
		}
		ConvTypeV::Shorter => {
			let value = f64::from_untyped(value.clone())?;
			let fpprec = fpprec.max(1);
			let exponent = shorter_exponent(value, fpprec);
			if exponent < -4 || exponent >= i32::from(fpprec) {
				render_float_sci(
					&mut tmp_out,
					value,
					padding,
					fpprec - 1,
					clfags.blank,
					clfags.sign,
					clfags.alt,
					clfags.alt,
					code.caps,
				);
			} else {
				let frac_prec = (i32::from(fpprec) - 1 - exponent).max(0) as u16;
				render_float(
					&mut tmp_out,
					value,
					padding,
					frac_prec,
					clfags.blank,
					clfags.sign,
					clfags.alt,
					clfags.alt,
				);
			}
		}
		ConvTypeV::Char => match value.clone() {
			v @ Val::Num(_) => {
				let codepoint = Codepoint::from_untyped(v)?;
				tmp_out.push(codepoint.try_char()?);
			}
			Val::Str(s) => {
				let s = s.into_flat();
				if s.chars().count() != 1 {
					bail!("%c expected 1 char string, got {}", s.chars().count());
				}
				tmp_out.push_str(&s);
			}
			_ => {
				bail!(TypeMismatch(
					"%c requires number/string",
					vec![ValType::Num, ValType::Str],
					value.value_type(),
				));
			}
		},
		ConvTypeV::Percent => tmp_out.push('%'),
	}

	let padding = width.saturating_sub(tmp_out.len() as u16);

	if !clfags.left {
		for _ in 0..padding {
			out.push(' ');
		}
	}
	out.push_str(&tmp_out);
	if clfags.left {
		for _ in 0..padding {
			out.push(' ');
		}
	}

	Ok(())
}

pub fn format_arr(str: &str, mut values: &[Val]) -> Result<String> {
	let codes = parse_codes(str)?;
	let mut out = String::new();
	let value_count = values.len();

	for code in codes {
		match code {
			Element::String(s) => {
				out.push_str(s);
			}
			Element::Code(c) => {
				let width = match c.width {
					Width::Star => {
						if values.is_empty() {
							bail!(NotEnoughValues);
						}
						let value = &values[0];
						values = &values[1..];
						u16::from_untyped(value.clone())?
					}
					Width::Fixed(n) => n,
				};
				let precision = match c.precision {
					Some(Width::Star) => {
						if values.is_empty() {
							bail!(NotEnoughValues);
						}
						let value = &values[0];
						values = &values[1..];
						Some(u16::from_untyped(value.clone())?)
					}
					Some(Width::Fixed(n)) => Some(n),
					None => None,
				};

				// %% should not consume a value
				let value = if c.convtype == ConvTypeV::Percent {
					&Val::Null
				} else {
					if values.is_empty() {
						bail!(NotEnoughValues);
					}
					let value = &values[0];
					values = &values[1..];
					value
				};

				format_code(&mut out, value, &c, width, precision)?;
			}
		}
	}

	if !values.is_empty() {
		bail!(
			"too many values to format, expected {value_count}, got {}",
			value_count + values.len()
		)
	}

	Ok(out)
}

fn get_dotted_field(obj: ObjValue, field: &str) -> Result<Val> {
	let mut current = Val::Obj(obj);
	let mut name_offset = 0;
	for component in field.split('.') {
		let end_offset = name_offset + component.len();
		current = if let Val::Obj(obj) = current {
			if let Some(value) = obj.get(component.into())? {
				value
			} else {
				let current = &field[name_offset..end_offset];
				let full = &field[..name_offset];
				let found = Box::new(suggest_object_fields(&obj, current.into()));
				bail!(SubfieldNotFound {
					current: current.into(),
					full: full.into(),
					found,
				})
			}
		} else {
			// No underflow may happen, initially we always start with an object
			let subfield = &field[..name_offset - 1];
			bail!(SubfieldDidntYieldAnObject(
				subfield.into(),
				current.value_type()
			));
		};
		name_offset = end_offset + 1;
	}
	Ok(current)
}

pub fn format_obj(str: &str, values: &ObjValue) -> Result<String> {
	let codes = parse_codes(str)?;
	let mut out = String::new();

	for code in codes {
		match code {
			Element::String(s) => {
				out.push_str(s);
			}
			Element::Code(c) => {
				// TODO: Operate on ref
				let f: IStr = c.mkey.into();
				let width = match c.width {
					Width::Star => {
						bail!(CannotUseStarWidthWithObject);
					}
					Width::Fixed(n) => n,
				};
				let precision = match c.precision {
					Some(Width::Star) => {
						bail!(CannotUseStarWidthWithObject);
					}
					Some(Width::Fixed(n)) => Some(n),
					None => None,
				};

				let value = if c.convtype == ConvTypeV::Percent {
					Val::Null
				} else {
					if f.is_empty() {
						bail!(MappingKeysRequired);
					}
					if let Some(v) = values.get(f.clone())? {
						v
					} else {
						get_dotted_field(values.clone(), &f)?
					}
				};

				format_code(&mut out, &value, &c, width, precision)?;
			}
		}
	}

	Ok(out)
}

#[cfg(test)]
pub mod test_format {
	use super::*;
	use crate::NumValue;

	#[test]
	fn parse() {
		assert_eq!(
			parse_codes(
				"How much error budget is left looking at our %.3f%% availability gurantees?"
			)
			.unwrap()
			.len(),
			4
		);
	}

	fn num(v: f64) -> Val {
		Val::Num(NumValue::new(v).expect("finite"))
	}

	#[test]
	fn octals() {
		assert_eq!(format_arr("%#o", &[num(8.0)]).unwrap(), "010");
		assert_eq!(format_arr("%#4o", &[num(8.0)]).unwrap(), " 010");
		assert_eq!(format_arr("%4o", &[num(8.0)]).unwrap(), "  10");
		assert_eq!(format_arr("%04o", &[num(8.0)]).unwrap(), "0010");
		assert_eq!(format_arr("%+4o", &[num(8.0)]).unwrap(), " +10");
		assert_eq!(format_arr("%+04o", &[num(8.0)]).unwrap(), "+010");
		assert_eq!(format_arr("%-4o", &[num(8.0)]).unwrap(), "10  ");
		assert_eq!(format_arr("%+-4o", &[num(8.0)]).unwrap(), "+10 ");
		assert_eq!(format_arr("%+-04o", &[num(8.0)]).unwrap(), "+10 ");
	}

	#[test]
	fn percent_doesnt_consumes_values() {
		assert_eq!(
			format_arr(
				"How much error budget is left looking at our %.3f%% availability gurantees?",
				&[num(4.0)]
			)
			.unwrap(),
			"How much error budget is left looking at our 4.000% availability gurantees?"
		);
	}
}
