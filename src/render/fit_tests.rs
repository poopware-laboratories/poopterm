//! Glyphs wider than their cells: spilling into blank cells, or shrinking.
use super::*;
use crate::{
	parser::Parser,
	render::{glyphs::FontSet, grid::GridView, painter::Painter},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
};

#[test]
fn glyphs_that_fit_are_left_alone() {
	assert_eq!(fit_scale((8, 15), (10, 20)), 1.0);
	assert_eq!(fit_left(1, 8, 10), 1);
}

#[test]
fn wide_or_tall_glyphs_shrink_with_room_for_rounding() {
	let scale = fit_scale((16, 13), (8, 20));
	assert!(16.0 * scale + 1.0 <= 8.0, "{scale}");
	let scale = fit_scale((8, 30), (10, 20));
	assert!(30.0 * scale + 1.0 <= 20.0, "{scale}");
}

#[test]
fn bitmaps_hanging_out_of_their_room_move_inside() {
	assert_eq!(fit_left(-2, 8, 10), 0);
	assert_eq!(fit_left(5, 8, 10), 2);
	assert_eq!(fit_left(-2, 12, 10), 0);
}

const BG: u32 = 0x000000;

/// Draws one row of 10×20 cells with a 32px font, so every glyph's ink is
/// wider than its cell, and returns the ink in each column of pixels.
fn ink_per_column(input: &str) -> Vec<u32> {
	let mut term = Term::new(4, 1, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(
		&mut term,
		format!("\x1b[38;2;255;255;255m\x1b[48;2;0;0;0m{input}").as_bytes(),
	);
	let mut frame = Frame::default();
	term.snapshot(&mut frame);
	frame.cursor = None;
	let (width, height) = (40, 20);
	let mut pixels = vec![BG; width * height];
	let mut painter = Painter::new(&mut pixels, width, height);
	let view = GridView {
		frame: &frame,
		origin: (0, 0),
		cell: (10, 20),
		px: 32.0,
		ascent: 16,
		scale: 1,
		default_bg: BG,
		link_color: 0,
		selection_colors: (0, 0),
		match_colors: (0, 0),
		selection: None,
		search_match: None,
		focused: true,
		padding: None,
		dim: None,
	};
	let font = crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap();
	let mut fonts = FontSet::new(font, Vec::new());
	crate::render::grid::draw_grid(&mut painter, &mut fonts, &view, None);
	(0..width)
		.map(|x| (0..height).map(|y| pixels[y * width + x] & 0xff).sum())
		.collect()
}

#[test]
fn a_wide_glyph_spills_into_a_blank_cell_to_its_right() {
	let ink = ink_per_column("W ");
	assert!(ink[..10].iter().any(|&ink| ink > 0));
	assert!(ink[10..20].iter().any(|&ink| ink > 0), "{ink:?}");
}

#[test]
fn a_wide_glyph_with_a_neighbor_shrinks_into_its_cell() {
	let whole: u32 = ink_per_column("W ").iter().sum();
	let ink = ink_per_column("W\u{2007}");
	let first: u32 = ink[..10].iter().sum();
	// Shrunk to a third of the width, the glyph keeps well under half its
	// ink, all of it inside its own cell.
	assert!(
		first > 0 && 2 * first < whole,
		"{first} of {whole}: {ink:?}"
	);
	assert!(ink[10..20].iter().all(|&ink| ink == 0), "{ink:?}");
}
