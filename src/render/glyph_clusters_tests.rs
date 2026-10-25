//! Tests for clusters drawn as one glyph: which marks join a cluster, which
//! font joins it, caching, and drawing joined clusters on the grid.
use super::*;
use crate::{
	fonts::{
		cbdt_tests::{Image, Strike, font_tables, png, sfnt},
		gsub_tests::{gsub, ligatures, lookup},
	},
	parser::Parser,
	render::{grid::GridView, painter::Painter},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

pub(crate) const BLUE: u32 = 0xff00_00ff;
pub(crate) const GREEN: u32 = 0xff00_ff00;
pub(crate) const RED: u32 = 0xffff_0000;

/// The characters of [`emoji_font`], sorted, as glyphs 1 to 6.
const CHARS: [char; 6] = ['1', '\u{200d}', '\u{20e3}', '❤', '\u{fe0f}', '🔥'];
const KEYCAP: &[char] = &['1', '\u{fe0f}', '\u{20e3}'];
const HEART_ON_FIRE: &[char] = &['❤', '\u{fe0f}', '\u{200d}', '🔥'];

/// A color font whose characters are blue 10-pixel squares at 16 pixels
/// per em, with a green keycap 1 (glyph 7) and a red heart on fire
/// (glyph 8) as ligatures.
pub(crate) fn emoji_font() -> Font {
	emoji_font_with(GREEN)
}

/// [`emoji_font`] with a `keycap` colored keycap.
fn emoji_font_with(keycap: u32) -> Font {
	let square = |color| {
		Some(Image {
			metrics: [10, 10, 0, 14, 12],
			png: png(10, 10, &[color; 100]),
		})
	};
	let mut images: Vec<_> = CHARS.iter().map(|_| square(BLUE)).collect();
	images.extend([square(keycap), square(RED)]);
	let strike = Strike {
		ppem: 16,
		first: 1,
		index_format: 1,
		image_format: 17,
		images,
	};
	let joins = ligatures(&[(1, &[(7, &[5, 3][..])]), (4, &[(8, &[5, 2, 6][..])])]);
	let mut tables = font_tables(&CHARS, &[strike]);
	tables.push((
		b"GSUB",
		gsub(&[(b"ccmp", &[0])], vec![lookup(4, 0, vec![joins])]),
	));
	tables.sort_by_key(|table| *table.0);
	Font::from_bytes(sfnt(&tables), 0).unwrap()
}

fn bundled() -> Font {
	Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap()
}

/// The color of the middle pixel of a color glyph.
fn middle(glyph: &Glyph) -> Option<u32> {
	let m = glyph.metrics;
	glyph
		.color
		.as_ref()
		.map(|pixels| pixels[m.height / 2 * m.width + m.width / 2])
}

#[test]
fn joiners_keycaps_and_tags_join_a_cluster_and_other_marks_do_not() {
	for marks in [
		&['\u{200d}', '🔥'][..],
		&['\u{fe0f}', '\u{20e3}'],
		&['\u{e0067}', '\u{e007f}'],
		&['\u{e0020}'],
	] {
		assert!(joins(marks), "{marks:?}");
	}
	for marks in [
		&[][..],
		&['\u{fe0f}'],
		&['\u{301}', '\u{302}'],
		&['\u{e001f}', '\u{e0080}'],
	] {
		assert!(!joins(marks), "{marks:?}");
	}
}

#[test]
fn a_cluster_a_fallback_joins_is_its_ligature_in_color() {
	let mut fonts = FontSet::new(bundled(), vec![emoji_font()]);
	assert_eq!(fonts.cluster(KEYCAP, 16.0).and_then(middle), Some(GREEN));
	assert_eq!(
		fonts.cluster(HEART_ON_FIRE, 16.0).and_then(middle),
		Some(RED)
	);
	assert!(
		fonts.cluster(&['❤', '\u{200d}', '1'], 16.0).is_none(),
		"no ligature"
	);
	assert!(
		fonts.cluster(&['x', '\u{200d}', '🔥'], 16.0).is_none(),
		"no font has x in color"
	);
}

#[test]
fn joined_and_unjoined_clusters_are_cached_per_size() {
	let mut fonts = FontSet::new(bundled(), vec![emoji_font()]);
	fonts.cluster(KEYCAP, 16.0);
	fonts.cluster(KEYCAP, 16.0);
	fonts.cluster(&['❤', '\u{200d}', '1'], 16.0);
	assert_eq!(fonts.clusters.len(), 2);
	let half = fonts.cluster(KEYCAP, 8.0).map(|glyph| glyph.metrics.width);
	assert_eq!(half, Some(5), "drawn again at the other size");
	assert_eq!(fonts.clusters.len(), 3);
	fonts.clear();
	assert!(fonts.clusters.is_empty());
	assert_eq!(fonts.cluster(KEYCAP, 16.0).and_then(middle), Some(GREEN));
}

#[test]
fn trimming_resets_an_oversized_cluster_cache() {
	let mut fonts = FontSet::new(bundled(), vec![emoji_font()]);
	for size in 0..=super::super::MAX_CACHED_GLYPHS {
		fonts.cluster(&['❤', '\u{200d}', '1'], 1.0 + size as f32);
	}
	fonts.trim();
	assert!(fonts.clusters.is_empty());
	fonts.cluster(KEYCAP, 16.0);
	fonts.trim();
	assert_eq!(fonts.clusters.len(), 1, "a small cache is kept");
}

#[test]
fn the_primary_font_joins_when_no_fallback_does() {
	let mut fonts = FontSet::new(emoji_font(), vec![bundled()]);
	assert_eq!(fonts.cluster(KEYCAP, 16.0).and_then(middle), Some(GREEN));
}

#[test]
fn a_color_fallback_joins_before_the_primary_font() {
	let mut fonts = FontSet::new(emoji_font_with(RED), vec![emoji_font()]);
	assert_eq!(fonts.cluster(KEYCAP, 16.0).and_then(middle), Some(GREEN));
	let mut text_first = FontSet::new(bundled(), vec![bundled(), emoji_font()]);
	assert_eq!(
		text_first.cluster(KEYCAP, 16.0).and_then(middle),
		Some(GREEN)
	);
}

#[test]
fn a_cluster_no_font_joins_draws_its_first_character_as_an_emoji() {
	let mut fonts = FontSet::new(bundled(), vec![emoji_font()]);
	let unjoined = ['❤', '\u{200d}', '1'];
	assert_eq!(
		middle(fonts.shaped(Shape::Cluster(&unjoined), 16.0)),
		Some(BLUE)
	);
	assert_eq!(
		middle(fonts.shaped(Shape::Cluster(KEYCAP), 16.0)),
		Some(GREEN)
	);
	let text = fonts.shaped(Shape::Char('1', Presentation::Default), 16.0);
	assert!(text.color.is_none(), "a plain 1 stays text");
	let empty = fonts.shaped(Shape::Cluster(&[]), 16.0);
	assert!(empty.color.is_none());
}

const WIDTH: usize = 40;

/// Draws `input` in four 10×20 cells and returns the pixels.
fn render(input: &str) -> Vec<u32> {
	let mut fonts = FontSet::new(bundled(), vec![emoji_font()]);
	let mut term = Term::new(4, 1, Theme::from_kind(ThemeKind::Poop), "/".into());
	let input = format!("\x1b[38;2;255;255;255m\x1b[48;2;0;0;0m{input}");
	Parser::new().advance(&mut term, input.as_bytes());
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	frame.cursor = None;
	let mut pixels = vec![0; WIDTH * 20];
	let mut painter = Painter::new(&mut pixels, WIDTH, 20);
	let view = GridView {
		frame: &frame,
		origin: (0, 0),
		cell: (10, 20),
		px: 16.0,
		ascent: 14,
		scale: 1,
		default_bg: 0,
		link_color: 0,
		selection_colors: (0, 0),
		match_colors: (0, 0),
		selection: None,
		search_match: None,
		focused: true,
		padding: None,
	};
	crate::render::grid::draw_grid(&mut painter, &mut fonts, &view, None);
	pixels
}

/// Whether any pixel of the first cell is `color` (without alpha).
fn first_cell_has(pixels: &[u32], color: u32) -> bool {
	(0..20).any(|y| pixels[y * WIDTH..y * WIDTH + 10].contains(&(color & 0xff_ffff)))
}

#[test]
fn a_keycap_sequence_draws_its_ligature_instead_of_the_digit() {
	let pixels = render("1\u{fe0f}\u{20e3}");
	assert!(first_cell_has(&pixels, GREEN));
	assert!(
		!first_cell_has(&pixels, BLUE),
		"no digit or keycap mark drawn apart"
	);
	assert!(!first_cell_has(&pixels, 0xffffff), "nor in text");
}

#[test]
fn a_zwj_sequence_draws_its_ligature_and_none_of_its_parts() {
	let pixels = render("❤\u{fe0f}\u{200d}🔥");
	assert!(first_cell_has(&pixels, RED));
	assert!(!first_cell_has(&pixels, BLUE));
}

#[test]
fn a_sequence_no_font_joins_draws_as_before() {
	// The ❤ draws (as an emoji, VS16) and the joined 1 is drawn as a mark.
	let pixels = render("❤\u{fe0f}\u{200d}1");
	assert!(first_cell_has(&pixels, BLUE));
	assert!(first_cell_has(&pixels, 0xffffff), "the 1 drawn over it");
	assert!(!first_cell_has(&pixels, RED) && !first_cell_has(&pixels, GREEN));
}

#[test]
fn a_text_presentation_keeps_a_sequence_apart() {
	let pixels = render("1\u{fe0e}\u{20e3}");
	assert!(!first_cell_has(&pixels, GREEN));
	assert!(first_cell_has(&pixels, 0xffffff), "the digit in text");
}

#[path = "glyph_clusters_grid_tests.rs"]
pub(crate) mod grid;
