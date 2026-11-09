//! Tests of dimmed grid views, which draw an inactive pane: every color
//! moves toward the default background by one weight, default backgrounds
//! stay as they are, and a view that is not dimmed draws as ever.
use super::{
	blend::blend,
	grid::{GridView, draw_grid},
	grid_case_tests::{CELL, ORIGIN, SIZE, fonts, frame, view},
	painter::Painter,
	pane_view::INACTIVE_PANE,
};
use crate::graphics::Rect;

/// The terminal's own default background, which a view treats as bare.
const BG: u32 = crate::theme::DEFAULT_BG;
const RED: u32 = 0xc8_0000;
const BLUE: u32 = 0x00_00c8;
/// Marker the whole surface starts with, so untouched pixels show.
const UNDER: u32 = 0x5a_5a5a;

/// The pixels of a grid showing `input`, dimmed by `dim`, with the padding
/// area the whole surface when `padding`.
fn draw(input: &str, dim: Option<u32>, padding: bool) -> Vec<u32> {
	let frame = frame(input);
	let mut pixels = vec![UNDER; SIZE.0 * SIZE.1];
	let mut painter = Painter::new(&mut pixels, SIZE.0, SIZE.1);
	let whole = Rect {
		x: 0,
		y: 0,
		width: SIZE.0,
		height: SIZE.1,
	};
	let view = GridView {
		dim,
		default_bg: BG,
		padding: padding.then_some(whole),
		..view(&frame)
	};
	draw_grid(&mut painter, &mut fonts(), &view, None);
	pixels
}

fn at(pixels: &[u32], col: usize, row: usize) -> u32 {
	// The middle of cell (col, row).
	pixels[(ORIGIN.1 + row * CELL.1 + CELL.1 / 2) * SIZE.0 + ORIGIN.0 + col * CELL.0 + CELL.0 / 2]
}

#[test]
fn an_inactive_panes_weight_is_sixty_two_percent() {
	assert_eq!(INACTIVE_PANE, 158);
	assert_eq!(INACTIVE_PANE * 100 / 255, 61);
}

#[test]
fn a_colored_background_moves_toward_the_default_by_the_weight() {
	let input = "\x1b[48;2;200;0;0m \x1b[0m";
	assert_eq!(at(&draw(input, None, false), 0, 0), RED);
	let dimmed = at(&draw(input, Some(INACTIVE_PANE), false), 0, 0);
	assert_eq!(dimmed, blend(BG, RED, INACTIVE_PANE));
	assert_ne!(dimmed, RED);
}

#[test]
fn text_moves_toward_the_default_background_too() {
	let input = "\x1b[38;2;0;0;200m\u{2588}";
	assert_eq!(at(&draw(input, None, false), 0, 0), BLUE);
	assert_eq!(
		at(&draw(input, Some(INACTIVE_PANE), false), 0, 0),
		blend(BG, BLUE, INACTIVE_PANE)
	);
}

#[test]
fn default_backgrounds_are_left_alone_so_the_pane_keeps_its_ground() {
	let dimmed = draw("plain text", Some(INACTIVE_PANE), false);
	// Beyond the text, a default cell draws nothing over what was there.
	assert_eq!(at(&dimmed, 20, 3), UNDER);
	assert_eq!(at(&draw("plain text", None, false), 20, 3), UNDER);
}

#[test]
fn a_full_weight_draws_exactly_what_no_dimming_draws() {
	let input = "\x1b[48;2;200;0;0m\x1b[38;2;0;0;200mhello\x1b[0m world\x1b[4munder";
	assert_eq!(draw(input, Some(255), false), draw(input, None, false));
}

#[test]
fn a_zero_weight_collapses_every_color_to_the_default_background() {
	let input = "\x1b[48;2;200;0;0m\x1b[38;2;0;0;200m\u{2588}\u{2588}\x1b[0m";
	let pixels = draw(input, Some(0), false);
	assert_eq!((at(&pixels, 0, 0), at(&pixels, 1, 0)), (BG, BG));
}

#[test]
fn every_channel_stays_between_the_background_and_the_colors_own_give_or_take_a_rounding_step() {
	let input = "\x1b[48;2;200;0;0m\x1b[38;2;0;0;200mabc\x1b[0m\x1b[48;2;10;200;10m \x1b[0m";
	let (plain, dimmed) = (
		draw(input, None, false),
		draw(input, Some(INACTIVE_PANE), false),
	);
	for (before, after) in plain.iter().zip(&dimmed) {
		if before == after {
			continue;
		}
		for shift in [16, 8, 0] {
			let (b, d, g) = (
				before >> shift & 0xff,
				after >> shift & 0xff,
				BG >> shift & 0xff,
			);
			assert!(
				// Blending truncates, so an antialiased edge may land one step out.
				d + 1 >= b.min(g) && d <= b.max(g) + 1,
				"channel {shift}: {b:#x} -> {d:#x} past {g:#x}"
			);
		}
	}
}

#[test]
fn a_heavier_weight_is_closer_to_the_original() {
	let input = "\x1b[48;2;200;0;0m ";
	let red = |weight| at(&draw(input, Some(weight), false), 0, 0) >> 16 & 0xff;
	assert!(red(60) < red(120) && red(120) < red(200) && red(200) < red(255));
}

#[test]
fn edge_backgrounds_in_the_padding_are_dimmed_like_the_cells_they_extend() {
	let input = "\x1b[48;2;200;0;0m\x1b[K";
	let pixels = draw(input, Some(INACTIVE_PANE), true);
	let beside = pixels[(ORIGIN.1 + 4) * SIZE.0 + 2];
	assert_eq!(beside, blend(BG, RED, INACTIVE_PANE));
	assert_eq!(
		at(&pixels, 3, 0),
		beside,
		"the padding matches the row it extends"
	);
}

#[test]
fn dimming_changes_nothing_outside_the_grid_and_its_padding() {
	let input = "\x1b[48;2;200;0;0mxy";
	let (plain, dimmed) = (
		draw(input, None, false),
		draw(input, Some(INACTIVE_PANE), false),
	);
	for x in 0..SIZE.0 {
		assert_eq!(plain[x], dimmed[x], "the top margin, column {x}");
	}
	for y in 0..ORIGIN.1 {
		assert_eq!(plain[y * SIZE.0 + 3], dimmed[y * SIZE.0 + 3], "row {y}");
	}
}

#[test]
fn an_underline_follows_its_text_color_into_the_dimmed_tone() {
	let input = "\x1b[38;2;0;0;200m\x1b[4m   ";
	let line = |pixels: &[u32]| pixels[(ORIGIN.1 + CELL.1 - 2) * SIZE.0 + ORIGIN.0 + 4];
	assert_eq!(line(&draw(input, None, false)), BLUE);
	assert_eq!(
		line(&draw(input, Some(INACTIVE_PANE), false)),
		blend(BG, BLUE, INACTIVE_PANE)
	);
}
