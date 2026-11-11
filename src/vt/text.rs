//! Text the tokenizer checked as UTF-8, kept as bytes so printing decodes it
//! without checking it again.
//!
//! Only [`Text::new`], after validation, and `From<&str>` make one, so a
//! `Text` holds UTF-8. Nothing relies on that for soundness: decoding checks
//! each code point and turns anything malformed into U+FFFD.
use super::utf8;

/// Valid UTF-8, as bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Text<'a>(&'a [u8]);

impl<'a> Text<'a> {
	/// U+FFFD, printed for malformed input.
	pub(crate) const REPLACEMENT: Text<'static> = Text("\u{fffd}".as_bytes());

	/// `bytes`, if they are valid UTF-8.
	#[inline]
	pub(crate) fn new(bytes: &'a [u8]) -> Option<Self> {
		utf8::is_valid(bytes).then_some(Self(bytes))
	}

	#[inline]
	pub(crate) fn as_bytes(self) -> &'a [u8] {
		self.0
	}

	#[inline]
	pub(crate) fn len(self) -> usize {
		self.0.len()
	}

	#[inline]
	pub(crate) fn is_empty(self) -> bool {
		self.0.is_empty()
	}

	#[inline]
	pub(crate) fn chars(self) -> Chars<'a> {
		Chars(self.0)
	}

	/// The text before and after byte `at`, which must start a character.
	#[inline]
	pub(crate) fn split_at(self, at: usize) -> (Self, Self) {
		debug_assert!(self.0.get(at).is_none_or(|&byte| !is_continuation(byte)));
		let (before, after) = self.0.split_at(at);
		(Self(before), Self(after))
	}

	/// The text as a `str`. Checks it again, so it is for cold paths.
	pub(crate) fn to_str(self) -> &'a str {
		std::str::from_utf8(self.0).unwrap_or("\u{fffd}")
	}
}

impl<'a> From<&'a str> for Text<'a> {
	#[inline]
	fn from(text: &'a str) -> Self {
		Self(text.as_bytes())
	}
}

fn is_continuation(byte: u8) -> bool {
	byte & 0xc0 == 0x80
}

/// The characters of a [`Text`], with the bytes not yet decoded.
#[derive(Clone, Debug)]
pub(crate) struct Chars<'a>(&'a [u8]);

impl<'a> Chars<'a> {
	/// The bytes not yet decoded.
	#[inline]
	pub(crate) fn as_bytes(&self) -> &'a [u8] {
		self.0
	}

	/// The text not yet decoded.
	#[inline]
	pub(crate) fn as_text(&self) -> Text<'a> {
		Text(self.0)
	}

	/// Skips `len` bytes, which must end on a character boundary.
	#[inline]
	pub(crate) fn advance(&mut self, len: usize) {
		self.0 = self.as_text().split_at(len).1.0;
	}

	/// Decodes the character that `lead`, a byte `0x80..`, starts.
	#[inline(always)]
	fn multibyte(&mut self, lead: u8) -> char {
		// Away from the end, one length check covers every byte read.
		let (code, len) = match *self.0 {
			[_, b, c, d, ..] => decode(lead, [b, c, d]),
			[_, b, c] => decode(lead, [b, c, 0]),
			[_, b] => decode(lead, [b, 0, 0]),
			_ => (0xfffd, 1),
		};
		let Some(rest) = self.0.get(len..) else {
			// Cut off; a `Text` never ends like this.
			self.0 = &[];
			return '\u{fffd}';
		};
		self.0 = rest;
		char::from_u32(code).unwrap_or('\u{fffd}')
	}
}

/// The code point and length of the character that `lead` starts, with the
/// bytes after it. A stray continuation byte reads as a two-byte lead.
#[inline(always)]
fn decode(lead: u8, [b, c, d]: [u8; 3]) -> (u32, usize) {
	if lead < 0xe0 {
		(u32::from(lead & 0x1f) << 6 | low(b), 2)
	} else if lead < 0xf0 {
		(u32::from(lead & 0x0f) << 12 | low(b) << 6 | low(c), 3)
	} else {
		(
			u32::from(lead & 0x07) << 18 | low(b) << 12 | low(c) << 6 | low(d),
			4,
		)
	}
}

impl Iterator for Chars<'_> {
	type Item = char;

	#[inline]
	fn next(&mut self) -> Option<char> {
		let (&lead, rest) = self.0.split_first()?;
		if lead < 0x80 {
			self.0 = rest;
			return Some(char::from(lead));
		}
		Some(self.multibyte(lead))
	}
}

/// The six payload bits of a continuation byte.
#[inline(always)]
fn low(byte: u8) -> u32 {
	u32::from(byte & 0x3f)
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
