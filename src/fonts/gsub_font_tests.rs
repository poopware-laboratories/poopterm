//! Shaping through a whole font: the character map before the
//! substitutions, the glyph bound, `GDEF` read from the font, malformed or
//! missing `GSUB` tables, which sequence `Font::ligature` prefers, and
//! drawing glyphs by index.
use super::{CCMP, color_font_with, gsub, ligatures, lookup, multiple};
use crate::fonts::{
	Font,
	cbdt_tests::{Image, Strike, font_tables, png, sfnt},
	otl::tests::gdef,
};

/// A color font of `chars`, sorted, as glyphs 1 on, with eight more
/// glyphs after them, and the `extra` tables.
fn font_with(chars: &[char], extra: Vec<(&'static [u8; 4], Vec<u8>)>) -> Font {
	let images = (0..chars.len() + 8)
		.map(|_| {
			Some(Image {
				metrics: [2, 2, 0, 2, 2],
				png: png(2, 2, &[0xff00_00ff; 4]),
			})
		})
		.collect();
	let strike = Strike {
		ppem: 20,
		first: 1,
		index_format: 1,
		image_format: 17,
		images,
	};
	let mut tables = font_tables(chars, &[strike]);
	tables.extend(extra);
	tables.sort_by_key(|table| *table.0);
	Font::from_bytes(sfnt(&tables), 0).unwrap()
}

/// A ligature of glyphs 1 and 2 into 9.
fn one_and_two() -> Vec<u8> {
	gsub(
		&[(CCMP, &[0])],
		vec![lookup(4, 0, vec![ligatures(&[(1, &[(9, &[2])])])])],
	)
}

#[test]
fn a_font_without_substitutions_joins_nothing() {
	let font = font_with(&['a', 'b'], Vec::new());
	assert_eq!(font.ligature(&['a', 'b']), None);
	assert!(font.rasterize_color('a', 20.0).is_some(), "it still draws");
}

#[test]
fn a_gsub_the_font_cannot_read_is_ignored() {
	let mut newer = one_and_two();
	newer[1] = 2;
	let font = font_with(&['a', 'b'], vec![(b"GSUB", newer)]);
	assert_eq!(font.ligature(&['a', 'b']), None);
	assert!(font.rasterize_color('b', 20.0).is_some());
	let font = font_with(&['a', 'b'], vec![(b"GSUB", one_and_two())]);
	assert_eq!(
		font.ligature(&['a', 'b']),
		Some(9),
		"the same table, readable"
	);
}

#[test]
fn characters_the_font_lacks_break_a_sequence() {
	// 'z' maps to glyph 0, which no ligature lists.
	let font = font_with(&['a', 'b'], vec![(b"GSUB", one_and_two())]);
	assert_eq!(font.ligature(&['a', 'z', 'b']), None);
	assert_eq!(font.ligature(&['z', 'a', 'b']), None);
	assert_eq!(font.ligature(&['a', 'b', 'z']), None, "two glyphs left");
}

#[test]
fn clusters_are_shaped_from_their_first_sixty_four_characters() {
	// Every 'a' is deleted and b c join: in 64 characters they do, in 65
	// the c is past the bound and the b is left alone.
	let delete = gsub(
		&[(CCMP, &[0, 1])],
		vec![
			lookup(2, 0, vec![multiple(&[(1, &[])])]),
			lookup(4, 0, vec![ligatures(&[(2, &[(9, &[3])])])]),
		],
	);
	let font = font_with(&['a', 'b', 'c'], vec![(b"GSUB", delete)]);
	let mut chars = vec!['a'; 62];
	chars.extend(['b', 'c']);
	assert_eq!(font.ligature(&chars), Some(9));
	chars.insert(0, 'a');
	assert_eq!(font.ligature(&chars), Some(2), "the b's own glyph");
}

#[test]
fn a_fonts_gdef_lets_its_ligatures_reach_past_marks() {
	// Glyphs 1 '1', 2 '2', 3 'x', 4 'y', 5 U+0301: a mark in `gdef`. The
	// ligature 1 + 2 ignores marks; a later lookup deletes the mark.
	let chars = ['1', '2', 'x', 'y', '\u{301}'];
	let shaping = || {
		gsub(
			&[(CCMP, &[0, 1])],
			vec![
				lookup(4, 8, vec![ligatures(&[(1, &[(9, &[2])])])]),
				lookup(2, 0, vec![multiple(&[(5, &[])])]),
			],
		)
	};
	let with_classes = font_with(&chars, vec![(b"GSUB", shaping()), (b"GDEF", gdef(0))]);
	assert_eq!(with_classes.ligature(&['1', '\u{301}', '2']), Some(9));
	let without = font_with(&chars, vec![(b"GSUB", shaping())]);
	assert_eq!(without.ligature(&['1', '\u{301}', '2']), None);
	let mut broken = gdef(0);
	broken[1] = 9;
	let unreadable = font_with(&chars, vec![(b"GSUB", shaping()), (b"GDEF", broken)]);
	assert_eq!(unreadable.ligature(&['1', '\u{301}', '2']), None);
}

#[test]
fn the_sequence_as_given_is_preferred_to_one_without_selectors() {
	// Glyphs 1 '1', 2 U+20E3, 3 FE0F: 1 FE0F 20E3 → 9, and 1 20E3 → 10.
	let chars = ['1', '\u{20e3}', '\u{fe0f}'];
	let both = gsub(
		&[(CCMP, &[0])],
		vec![lookup(
			4,
			0,
			vec![ligatures(&[(1, &[(9, &[3, 2]), (10, &[2])])])],
		)],
	);
	let font = font_with(&chars, vec![(b"GSUB", both)]);
	assert_eq!(font.ligature(&['1', '\u{fe0f}', '\u{20e3}']), Some(9));
	assert_eq!(font.ligature(&['1', '\u{20e3}']), Some(10));
}

#[test]
fn every_selector_is_dropped_for_the_retry() {
	// A ZWJ sequence fully qualified on both sides, for a font whose
	// ligature has no selectors: glyphs 1 ZWJ, 2 '❤', 3 FE0F, 4 '🔥'.
	let chars = ['\u{200d}', '❤', '\u{fe0f}', '🔥'];
	let plain = gsub(
		&[(CCMP, &[0])],
		vec![lookup(4, 0, vec![ligatures(&[(2, &[(9, &[1, 4])])])])],
	);
	let font = font_with(&chars, vec![(b"GSUB", plain)]);
	assert_eq!(
		font.ligature(&['❤', '\u{fe0f}', '\u{200d}', '🔥', '\u{fe0f}']),
		Some(9)
	);
	assert_eq!(font.ligature(&['❤', '\u{200d}', '🔥']), Some(9));
	assert_eq!(
		font.ligature(&['❤', '\u{fe0e}', '\u{200d}', '🔥']),
		None,
		"text selectors stay"
	);
}

#[test]
fn drawing_a_glyph_by_index_matches_drawing_its_character() {
	let font = Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap();
	for ch in ['a', 'W', '@', '│', 'é'] {
		let glyph = font.lookup_glyph_index(ch);
		assert_ne!(glyph, 0, "{ch}");
		assert_eq!(
			font.rasterize_glyph(glyph, 17.5),
			font.rasterize(ch, 17.5),
			"{ch}"
		);
		assert_eq!(font.metrics(ch, 17.5), font.rasterize(ch, 17.5).0, "{ch}");
	}
	assert!(
		font.rasterize_color_glyph(1, 16.0).is_none(),
		"no color glyphs"
	);
}

#[test]
fn color_glyphs_by_index_stop_at_the_last_glyph() {
	let font = color_font_with(&['a'], one_and_two());
	assert!(font.rasterize_color_glyph(1, 20.0).is_some());
	assert!(
		font.rasterize_color_glyph(0, 20.0).is_none(),
		"glyph 0 has no image"
	);
	assert!(font.rasterize_color_glyph(u16::MAX, 20.0).is_none());
}
