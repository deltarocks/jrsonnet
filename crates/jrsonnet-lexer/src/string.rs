//! Jsonnet-specific non-verbatim string unescaping

use std::str::Chars;

fn decode_unicode(chars: &mut Chars) -> Option<u16> {
	#[expect(
		clippy::cast_possible_truncation,
		reason = "truncated value is a single digit, thus it would never truncate anything"
	)]
	IntoIterator::into_iter([chars.next()?, chars.next()?, chars.next()?, chars.next()?])
		.map(|c| c.to_digit(16).map(|f| f as u16))
		.try_fold(0u16, |acc, v| Some((acc << 4) | (v?)))
}

/// Unescape escape characters in jsonnet string
pub fn unescape(s: &str) -> Option<String> {
	let mut chars = s.chars();
	let mut out = String::with_capacity(s.len());

	while let Some(c) = chars.next() {
		if c != '\\' {
			out.push(c);
			continue;
		}
		match chars.next()? {
			c @ ('\\' | '"' | '\'') => out.push(c),
			'b' => out.push('\u{0008}'),
			'f' => out.push('\u{000c}'),
			'n' => out.push('\n'),
			'r' => out.push('\r'),
			't' => out.push('\t'),
			'u' => match decode_unicode(&mut chars)? {
				// May only be second byte
				0xDC00..=0xDFFF => return None,
				// Surrogate pair
				n1 @ 0xD800..=0xDBFF => {
					if chars.next() != Some('\\') {
						return None;
					}
					if chars.next() != Some('u') {
						return None;
					}
					let n2 = decode_unicode(&mut chars)?;
					if !matches!(n2, 0xDC00..=0xDFFF) {
						return None;
					}
					let n = (u32::from(n1 - 0xD800) << 10 | u32::from(n2 - 0xDC00)) + 0x1_0000;
					out.push(char::from_u32(n)?);
				}
				n => out.push(char::from_u32(u32::from(n))?),
			},
			'x' => {
				let c = IntoIterator::into_iter([chars.next()?, chars.next()?])
					.map(|c| c.to_digit(16))
					.try_fold(0u32, |acc, v| Some((acc << 8) | (v?)))?;
				out.push(char::from_u32(c)?);
			}
			_ => return None,
		}
	}
	Some(out)
}
