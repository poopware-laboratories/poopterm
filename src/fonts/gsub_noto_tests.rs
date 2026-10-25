//! Shaping against the installed Noto Color Emoji, when there is one: the
//! kinds of recommended emoji sequences each join into their own color
//! glyph, in fully and minimally qualified forms, and sequences that are
//! not emoji stay apart.
use super::noto;
use crate::fonts::Font;
use std::collections::HashMap;

/// Recommended sequences of each kind: keycaps, flags, subdivision flags,
/// ZWJ sequences of people, professions, hair, skin tones, animals,
/// objects and flags.
const SEQUENCES: &[&str] = &[
	"#\u{fe0f}\u{20e3}",
	"*\u{fe0f}\u{20e3}",
	"0\u{fe0f}\u{20e3}",
	"9\u{fe0f}\u{20e3}",
	"🇺🇸",
	"🇯🇵",
	"🇧🇷",
	"🇺🇳",
	"🇪🇺",
	"🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
	"🏴\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}",
	"🏴\u{e0067}\u{e0062}\u{e0077}\u{e006c}\u{e0073}\u{e007f}",
	"👩\u{200d}👩\u{200d}👦",
	"👨\u{200d}👧\u{200d}👧",
	"👩\u{200d}❤\u{fe0f}\u{200d}👨",
	"👩\u{200d}❤\u{fe0f}\u{200d}💋\u{200d}👩",
	"🧑\u{200d}⚕\u{fe0f}",
	"👩\u{200d}🚀",
	"👨\u{200d}🍳",
	"🏃\u{200d}♀\u{fe0f}",
	"🤷\u{200d}♂\u{fe0f}",
	"👩\u{200d}🦰",
	"🧑\u{200d}🦳",
	"👩🏽\u{200d}🚀",
	"🧑🏿\u{200d}🦯",
	"🫱🏻\u{200d}🫲🏿",
	"🐻\u{200d}❄\u{fe0f}",
	"🐈\u{200d}⬛",
	"🐦\u{200d}🔥",
	"❤\u{fe0f}\u{200d}🩹",
	"😮\u{200d}💨",
	"👁\u{fe0f}\u{200d}🗨\u{fe0f}",
	"🏳\u{fe0f}\u{200d}🌈",
	"🏳\u{fe0f}\u{200d}⚧\u{fe0f}",
	"🏴\u{200d}☠\u{fe0f}",
];

fn chars(sequence: &str) -> Vec<char> {
	sequence.chars().collect()
}

/// The color pixels of `glyph`.
fn pixels(font: &Font, glyph: u16) -> Vec<u32> {
	font
		.rasterize_color_glyph(glyph, 32.0)
		.map(|(_, pixels)| pixels)
		.unwrap_or_default()
}

#[test]
fn each_kind_of_sequence_joins_into_its_own_color_glyph() {
	let Some(font) = noto() else {
		return;
	};
	let mut seen: HashMap<u16, &str> = HashMap::new();
	for sequence in SEQUENCES {
		let chars = chars(sequence);
		let glyph = font
			.ligature(&chars)
			.unwrap_or_else(|| panic!("{sequence} is not joined"));
		if let Some(other) = seen.insert(glyph, sequence) {
			panic!("{sequence} and {other} share glyph {glyph}");
		}
		let joined = pixels(&font, glyph);
		assert!(!joined.is_empty(), "{sequence} has no color glyph");
		let first = font
			.rasterize_color(chars[0], 32.0)
			.map(|(_, pixels)| pixels);
		assert_ne!(
			Some(joined),
			first,
			"{sequence} draws only its first character"
		);
	}
}

#[test]
fn minimally_qualified_sequences_join_like_fully_qualified_ones() {
	let Some(font) = noto() else {
		return;
	};
	for sequence in SEQUENCES
		.iter()
		.filter(|sequence| sequence.contains('\u{fe0f}'))
	{
		let full = chars(sequence);
		let minimal: Vec<char> = full.iter().copied().filter(|&c| c != '\u{fe0f}').collect();
		assert_eq!(font.ligature(&minimal), font.ligature(&full), "{sequence}");
	}
}

#[test]
fn sequences_that_are_not_emoji_stay_apart() {
	let Some(font) = noto() else {
		return;
	};
	for sequence in [
		"🔥\u{200d}🍕",     // no such ZWJ sequence
		"1\u{20e3}x",       // a keycap and a letter
		"🇺🇸🇯",              // a flag and a lone indicator
		"a\u{200d}b",       // letters
		"\u{200d}\u{200d}", // joiners alone
	] {
		assert_eq!(font.ligature(&chars(sequence)), None, "{sequence:?}");
	}
}

#[test]
fn an_unknown_subdivision_flag_draws_a_flag_of_its_own() {
	// Noto keeps a glyph for subdivisions it does not draw: neither the
	// black flag nor any real subdivision's.
	let Some(font) = noto() else {
		return;
	};
	let unknown = chars("🏴\u{e0075}\u{e0073}\u{e0074}\u{e0078}\u{e007f}");
	let glyph = font.ligature(&unknown).expect("joined");
	let flag = font.rasterize_color('🏴', 32.0).map(|(_, pixels)| pixels);
	assert!(Some(pixels(&font, glyph)) != flag, "not the black flag");
	for real in &SEQUENCES[9..12] {
		assert_ne!(font.ligature(&chars(real)), Some(glyph), "{real}");
	}
	let other = chars("🏴\u{e0066}\u{e0072}\u{e0069}\u{e0064}\u{e0066}\u{e007f}");
	assert_eq!(
		font.ligature(&other),
		Some(glyph),
		"one glyph for every unknown region"
	);
}

#[test]
fn skin_tones_apply_inside_zwj_sequences() {
	let Some(font) = noto() else {
		return;
	};
	let tones = ['🏻', '🏼', '🏽', '🏾', '🏿'];
	let mut glyphs: Vec<u16> = tones
		.iter()
		.map(|&tone| {
			font
				.ligature(&['👩', tone, '\u{200d}', '🚀'])
				.expect("joined")
		})
		.collect();
	glyphs.push(font.ligature(&chars("👩\u{200d}🚀")).expect("joined"));
	let count = glyphs.len();
	glyphs.sort_unstable();
	glyphs.dedup();
	assert_eq!(glyphs.len(), count, "each tone its own glyph");
}

#[test]
fn indicator_pairs_that_name_no_flag_share_one_placeholder() {
	// Noto's contextual lookup turns any other pair into one glyph.
	let Some(font) = noto() else {
		return;
	};
	let placeholder = font.ligature(&chars("🇦🇦")).expect("joined");
	for pair in ["🇿🇿", "🇽🇽", "🇶🇶"] {
		assert_eq!(font.ligature(&chars(pair)), Some(placeholder), "{pair}");
	}
	for flag in &SEQUENCES[4..9] {
		assert_ne!(font.ligature(&chars(flag)), Some(placeholder), "{flag}");
	}
}
