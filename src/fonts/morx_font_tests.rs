//! `morx` against the real Apple Color Emoji, where the system has it: each
//! sequence becomes one glyph with a color bitmap, which is what the
//! terminal draws. Skipped elsewhere.
use super::{data::read_file, face::Face};
use std::path::Path;

fn apple() -> Option<Face> {
	let data = read_file(Path::new("/System/Library/Fonts/Apple Color Emoji.ttc")).ok()?;
	Face::parse(data, 0).ok()
}

/// Sequences that must join into one glyph.
const JOINED: &[(&str, &str)] = &[
	("flag of Japan", "\u{1F1EF}\u{1F1F5}"),
	("flag of Brazil", "\u{1F1E7}\u{1F1F7}"),
	("thumbs up, medium skin", "\u{1F44D}\u{1F3FD}"),
	("astronaut", "\u{1F9D1}\u{200D}\u{1F680}"),
	(
		"woman teacher, dark skin",
		"\u{1F469}\u{1F3FF}\u{200D}\u{1F3EB}",
	),
	(
		"family",
		"\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
	),
	("keycap 1", "1\u{FE0F}\u{20E3}"),
	("red heart", "\u{2764}\u{FE0F}"),
	("pirate flag", "\u{1F3F4}\u{200D}\u{2620}\u{FE0F}"),
	(
		"flag of Scotland",
		"\u{1F3F4}\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}",
	),
];

#[test]
fn apple_color_emoji_joins_sequences_into_one_colored_glyph() {
	let Some(face) = apple() else { return };
	for (name, text) in JOINED {
		let chars: Vec<char> = text.chars().collect();
		let glyphs = face.shape(&chars);
		assert_eq!(glyphs.len(), 1, "{name}: {glyphs:?}");
		assert!(face.color_bitmap(glyphs[0]).is_some(), "{name}: no bitmap");
	}
}

#[test]
fn a_joined_sequence_is_not_the_picture_of_its_first_character() {
	let Some(face) = apple() else { return };
	// A heart with a variation selector is the heart's own glyph.
	for (name, text) in JOINED.iter().filter(|(name, _)| *name != "red heart") {
		let chars: Vec<char> = text.chars().collect();
		let first = face.glyph_index(chars[0]);
		let joined = face.shape(&chars)[0];
		assert_ne!(joined, first, "{name} drew only its first character");
	}
}

#[test]
fn a_couple_of_the_same_kind_without_a_joining_picture_stays_apart() {
	let Some(face) = apple() else { return };
	// Man, joiner, man is not a sequence: two men, the joiner gone.
	let men = face.shape(&['\u{1F468}', '\u{200D}', '\u{1F468}']);
	let man = face.glyph_index('\u{1F468}');
	assert_eq!(men, [man, man]);
}

#[test]
fn a_single_emoji_shapes_to_its_own_glyph() {
	let Some(face) = apple() else { return };
	for ch in ['\u{1F980}', '\u{1F44D}', '\u{1F4A9}', '\u{2728}'] {
		assert_eq!(face.shape(&[ch]), [face.glyph_index(ch)], "{ch:?}");
	}
}

#[test]
fn text_without_emoji_is_left_alone() {
	let Some(face) = apple() else { return };
	let chars = ['a', 'b', ' ', '1', '#'];
	let glyphs: Vec<u16> = chars.iter().map(|&ch| face.glyph_index(ch)).collect();
	assert_eq!(face.shape(&chars), glyphs);
}
