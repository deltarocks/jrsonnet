use std::collections::BTreeSet;

use jrsonnet_evaluator::{
	Either, IStr, Val, bail,
	error::{ErrorKind::*, Result},
	function::builtin,
	typed::{Either2, FromUntyped, M1},
	val::{ArrValue, IndexableVal},
};

#[builtin]
pub const fn builtin_codepoint(str: char) -> u32 {
	str as u32
}

#[builtin]
pub fn builtin_substr(str: IStr, from: usize, len: usize) -> String {
	str.chars().skip(from).take(len).collect()
}

#[builtin]
pub fn builtin_char(n: u32) -> Result<char> {
	Ok(std::char::from_u32(n).ok_or_else(|| InvalidUnicodeCodepointGot(n))?)
}

#[builtin]
pub fn builtin_str_replace(str: String, from: IStr, to: IStr) -> Result<String> {
	if from.is_empty() {
		bail!("`from` string must not be zero length");
	}
	Ok(str.replace(&from as &str, &to as &str))
}

#[builtin]
pub fn builtin_escape_string_bash(str_: String) -> String {
	const QUOTE: char = '\'';
	let mut out = str_.replace(QUOTE, "'\"'\"'");
	out.insert(0, QUOTE);
	out.push(QUOTE);
	out
}

#[builtin]
pub fn builtin_escape_string_dollars(str_: String) -> String {
	str_.replace('$', "$$")
}

#[builtin]
pub fn builtin_is_empty(str: String) -> bool {
	str.is_empty()
}

#[builtin]
pub fn builtin_equals_ignore_case(str1: String, str2: String) -> bool {
	str1.eq_ignore_ascii_case(&str2)
}

#[builtin]
pub fn builtin_splitlimit(str: IStr, c: IStr, maxsplits: Either![usize, M1]) -> ArrValue {
	use Either2::*;
	match maxsplits {
		A(n) => str.splitn(n + 1, &c as &str).map(Val::string).collect(),
		B(_) => str.split(&c as &str).map(Val::string).collect(),
	}
}

#[builtin]
pub fn builtin_splitlimitr(str: IStr, c: IStr, maxsplits: Either![usize, M1]) -> ArrValue {
	use Either2::*;
	match maxsplits {
		A(n) =>
		// rsplitn does not implement DoubleEndedIterator so collect into
		// a temporary vec
		{
			str.rsplitn(n + 1, &c as &str)
				.map(Val::string)
				.collect::<Vec<_>>()
				.into_iter()
				.rev()
				.collect()
		}
		B(_) => str.split(&c as &str).map(Val::string).collect(),
	}
}

#[builtin]
pub fn builtin_split(str: IStr, c: IStr) -> ArrValue {
	use Either2::*;
	builtin_splitlimit(str, c, B(M1))
}

#[builtin]
pub fn builtin_ascii_upper(str: IStr) -> String {
	str.to_ascii_uppercase()
}

#[builtin]
pub fn builtin_ascii_lower(str: IStr) -> String {
	str.to_ascii_lowercase()
}

#[builtin]
pub fn builtin_find_substr(pat: IStr, str: IStr) -> ArrValue {
	if pat.is_empty() || str.is_empty() || pat.len() > str.len() {
		return ArrValue::empty();
	}

	let str = str.as_str();
	let pat = pat.as_bytes();
	let strb = str.as_bytes();

	let max_pos = str.len() - pat.len();

	let mut out: Vec<Val> = Vec::new();
	for (ch_idx, (i, _)) in str
		.char_indices()
		.take_while(|(i, _)| i <= &max_pos)
		.enumerate()
	{
		if &strb[i..i + pat.len()] == pat {
			out.push(Val::Num(
				ch_idx.try_into().expect("unrealisticly long string"),
			));
		}
	}
	out.into()
}

#[builtin]
pub fn builtin_parse_int(str: IStr) -> Result<f64> {
	if let Some(raw) = str.strip_prefix('-') {
		if raw.is_empty() {
			bail!("integer only consists of a minus")
		}

		parse_nat::<10>(raw).map(|value| -value)
	} else {
		if str.is_empty() {
			bail!("empty integer")
		}

		parse_nat::<10>(str.as_str())
	}
}

#[builtin]
pub fn builtin_parse_octal(str: IStr) -> Result<f64> {
	if str.is_empty() {
		bail!("empty octal integer");
	}

	parse_nat::<8>(str.as_str())
}

#[builtin]
pub fn builtin_parse_hex(str: IStr) -> Result<f64> {
	if str.is_empty() {
		bail!("empty hexadecimal integer");
	}

	parse_nat::<16>(str.as_str())
}

fn parse_nat<const BASE: u32>(raw: &str) -> Result<f64> {
	const ZERO_CODE: u32 = '0' as u32;
	const UPPER_A_CODE: u32 = 'A' as u32;
	const LOWER_A_CODE: u32 = 'a' as u32;

	#[inline]
	fn checked_sub_if(condition: bool, lhs: u32, rhs: u32) -> Option<u32> {
		if condition {
			lhs.checked_sub(rhs)
		} else {
			None
		}
	}

	debug_assert!(
		1 <= BASE && BASE <= 16,
		"integer base should be between 1 and 16"
	);

	let base = f64::from(BASE);

	raw.chars().try_fold(0f64, |aggregate, digit| {
		let digit = digit as u32;
		// if-let-else looks better here than Option combinators
		#[allow(clippy::option_if_let_else)]
		let digit = if let Some(digit) = checked_sub_if(BASE > 10, digit, LOWER_A_CODE) {
			digit + 10
		} else if let Some(digit) = checked_sub_if(BASE > 10, digit, UPPER_A_CODE) {
			digit + 10
		} else {
			digit.checked_sub(ZERO_CODE).unwrap_or(BASE)
		};

		if digit < BASE {
			Ok(base.mul_add(aggregate, f64::from(digit)))
		} else {
			bail!("{raw:?} is not a base {BASE} integer");
		}
	})
}

#[cfg(feature = "exp-bigint")]
#[builtin]
pub fn builtin_bigint(v: Either![f64, IStr]) -> Result<Val> {
	use Either2::*;
	use jrsonnet_evaluator::runtime_error;
	Ok(match v {
		A(a) => {
			Val::BigInt(Box::new(a.to_string().parse().map_err(|e| {
				runtime_error!("number is not convertible to bigint: {e}")
			})?))
		}
		B(b) => Val::BigInt(Box::new(
			b.as_str()
				.parse()
				.map_err(|e| runtime_error!("bad bigint: {e}"))?,
		)),
	})
}

#[builtin]
pub fn builtin_string_chars(str: IStr) -> ArrValue {
	str.chars().collect()
}

#[builtin]
pub fn builtin_lstrip_chars(str: IStr, chars: IndexableVal) -> Result<IStr> {
	if str.is_empty() || chars.is_empty() {
		return Ok(str);
	}

	let pattern = new_trim_pattern(chars)?;
	Ok(str.as_str().trim_start_matches(pattern).into())
}

#[builtin]
pub fn builtin_rstrip_chars(str: IStr, chars: IndexableVal) -> Result<IStr> {
	if str.is_empty() || chars.is_empty() {
		return Ok(str);
	}

	let pattern = new_trim_pattern(chars)?;
	Ok(str.as_str().trim_end_matches(pattern).into())
}

#[builtin]
pub fn builtin_strip_chars(str: IStr, chars: IndexableVal) -> Result<IStr> {
	if str.is_empty() || chars.is_empty() {
		return Ok(str);
	}

	let pattern = new_trim_pattern(chars)?;
	Ok(str.as_str().trim_matches(pattern).into())
}

#[builtin]
pub fn builtin_trim(str: IStr) -> String {
	let filter =
		|v: char| {
			v == ' '
				|| v == '\t' || v == '\n'
				|| v == '\u{000c}'
				|| v == '\r' || v == '\u{0085}'
				|| v == '\u{00a0}'
		};
	str.as_str().trim_matches(filter).to_string()
}

fn new_trim_pattern(chars: IndexableVal) -> Result<impl Fn(char) -> bool> {
	let chars: BTreeSet<char> = match chars {
		IndexableVal::Str(chars) => chars.chars().collect(),
		IndexableVal::Arr(chars) => chars
			.iter()
			.filter_map(|it| it.map(|it| char::from_untyped(it).ok()).transpose())
			.collect::<Result<_, _>>()?,
	};

	Ok(move |char| chars.contains(&char))
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
	use super::*;

	#[test]
	fn parse_nat_base_8() {
		assert_eq!(parse_nat::<8>("0").unwrap(), 0.);
		assert_eq!(parse_nat::<8>("5").unwrap(), 5.);
		assert_eq!(parse_nat::<8>("32").unwrap(), f64::from(0o32));
		assert_eq!(parse_nat::<8>("761").unwrap(), f64::from(0o761));
	}

	#[test]
	fn parse_nat_base_10() {
		assert_eq!(parse_nat::<10>("0").unwrap(), 0.);
		assert_eq!(parse_nat::<10>("3").unwrap(), 3.);
		assert_eq!(parse_nat::<10>("27").unwrap(), 27.);
		assert_eq!(parse_nat::<10>("123").unwrap(), 123.);
	}

	#[test]
	fn parse_nat_base_16() {
		assert_eq!(parse_nat::<16>("0").unwrap(), 0.);
		assert_eq!(parse_nat::<16>("A").unwrap(), 10.);
		assert_eq!(parse_nat::<16>("a9").unwrap(), f64::from(0xA9));
		assert_eq!(parse_nat::<16>("BbC").unwrap(), f64::from(0xBBC));
	}
}
