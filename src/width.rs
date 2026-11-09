//! Terminal character widths, from a table `scripts/width_table.py` generates
//! from the Unicode Character Database.

/// Stage 1 gives each block of 128 code points a block number; stage 2 holds
/// each distinct block as 2-bit widths, four to a byte, lowest bits first.
const TABLE: &[u8] = include_bytes!("width.bin");
const STAGE1: usize = (char::MAX as usize + 1) >> 7;
const BLOCK: usize = 128 / 4;

const _: () = {
	assert!(TABLE.len() > STAGE1 && (TABLE.len() - STAGE1).is_multiple_of(BLOCK));
	let mut i = 0;
	while i < STAGE1 {
		assert!(STAGE1 + (TABLE[i] as usize + 1) * BLOCK <= TABLE.len());
		i += 1;
	}
};

/// The number of cells `c` takes: 0 for controls and characters that join the
/// one before, 2 for wide ones, and 3 for U+17D8 KHMER SIGN BEYYAL.
#[inline]
pub(crate) fn width(c: char) -> usize {
	let c = c as usize;
	if (0x20..0x7f).contains(&c) {
		return 1;
	}
	let block = STAGE1 + TABLE[c >> 7] as usize * BLOCK;
	(TABLE[block + (c & 127) / 4] as usize >> (c % 4 * 2)) & 3
}

#[cfg(test)]
mod tests {
	use super::*;
	use unicode_width::UnicodeWidthChar;

	#[test]
	fn every_scalar_agrees_with_unicode_width() {
		for c in (0..=0x10ffff).filter_map(char::from_u32) {
			assert_eq!(width(c), c.width().unwrap_or(0), "{c:?}");
		}
	}

	#[test]
	fn widths_follow_the_character_properties() {
		let cases = [
			('a', 1),
			('\0', 0),
			('\u{7f}', 0),
			('\u{9b}', 0),
			('\u{ad}', 0),     // soft hyphen, default ignorable
			('\u{301}', 0),    // combining acute accent
			('\u{5c8}', 0),    // Hebrew point, new in Unicode 18
			('\u{ff9e}', 1),   // halfwidth voiced mark, a spacing letter that extends
			('\u{3164}', 0),   // Hangul filler, an ignorable letter
			('\u{200d}', 0),   // zero width joiner
			('\u{1160}', 0),   // Hangul vowel filler
			('\u{1100}', 2),   // Hangul leading consonant
			('\u{115f}', 2),   // Hangul leading filler, though default ignorable
			('\u{ac00}', 2),   // Hangul syllable
			('\u{4e00}', 2),   // CJK ideograph
			('\u{ff21}', 2),   // fullwidth A
			('\u{1f600}', 2),  // emoji presentation
			('\u{2764}', 1),   // emoji with text presentation
			('\u{1f1e6}', 1),  // regional indicator
			('\u{600}', 1),    // Arabic number sign
			('\u{17d8}', 3),   // Khmer sign beyyal
			('\u{e0001}', 0),  // language tag
			('\u{10ffff}', 1), // noncharacter
			('\u{3fffd}', 2),  // unassigned in a wide plane
			('\u{20000}', 2),  // CJK extension B
		];
		for (c, expected) in cases {
			assert_eq!(width(c), expected, "{c:?}");
		}
	}
}
