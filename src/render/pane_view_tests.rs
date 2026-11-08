//! Tests of grid views moved into split panes: each pane draws its own
//! screen in its own place, only the focused one keeps highlights and a
//! solid cursor, and edge backgrounds stop at the pane's padding.
use super::{
	grid::{GridView, draw_grid},
	grid_case_tests::{BG, CELL, fonts, frame, view},
	painter::Painter,
};
use crate::{
	graphics::Rect,
	ui::selection::{CellPoint, Selection},
};

/// A window holding two 30 by 5 grids side by side, 9 pixels apart, with
/// 4 pixels of padding around both.
const WINDOW: (usize, usize) = (4 + 240 + 9 + 240 + 4, 4 + 80 + 4);
const LEFT: Rect = Rect {
	x: 4,
	y: 4,
	width: 240,
	height: 80,
};
const RIGHT: Rect = Rect {
	x: 4 + 240 + 9,
	y: 4,
	width: 240,
	height: 80,
};

fn selection() -> Option<Selection> {
	Some(Selection {
		start: CellPoint { row: 0, col: 0 },
		end: CellPoint { row: 1, col: 4 },
		block: false,
	})
}

fn base(frame: &crate::term::Frame) -> GridView<'_> {
	GridView {
		selection: selection(),
		search_match: selection(),
		focused: true,
		..view(frame)
	}
}

fn paint(views: &[GridView]) -> Vec<u32> {
	let mut pixels = vec![BG; WINDOW.0 * WINDOW.1];
	let mut painter = Painter::new(&mut pixels, WINDOW.0, WINDOW.1);
	for view in views {
		draw_grid(&mut painter, &mut fonts(), view, None);
	}
	pixels
}

/// The pixels of `rect` in a window's `pixels`.
fn cut(pixels: &[u32], rect: Rect) -> Vec<u32> {
	(rect.y..rect.y + rect.height)
		.flat_map(|y| pixels[y * WINDOW.0 + rect.x..y * WINDOW.0 + rect.x + rect.width].to_vec())
		.collect()
}

#[test]
fn the_focused_pane_keeps_highlights_and_focus_and_the_others_lose_them() {
	let (shown, other) = (frame("focused"), frame("other"));
	let focused = base(&shown).in_pane(&shown, RIGHT, RIGHT, true);
	assert_eq!(focused.origin, (RIGHT.x, RIGHT.y));
	assert_eq!(focused.selection, selection());
	assert_eq!(focused.search_match, selection());
	assert!(focused.focused);
	assert_eq!(focused.padding, Some(RIGHT));
	let unfocused = base(&shown).in_pane(&other, LEFT, LEFT, false);
	assert!(std::ptr::eq(unfocused.frame, &other));
	assert_eq!(unfocused.origin, (LEFT.x, LEFT.y));
	assert_eq!(unfocused.selection, None);
	assert_eq!(unfocused.search_match, None);
	assert!(!unfocused.focused);
	// The rest of the window's view carries over.
	assert_eq!(unfocused.cell, CELL);
	assert_eq!(unfocused.default_bg, BG);
	assert_eq!(unfocused.scale, 1);
}

#[test]
fn an_unfocused_window_leaves_every_pane_unfocused() {
	let shown = frame("x");
	let window = GridView {
		focused: false,
		..base(&shown)
	};
	assert!(!window.in_pane(&shown, LEFT, LEFT, true).focused);
	assert_eq!(
		window.in_pane(&shown, LEFT, LEFT, true).selection,
		selection(),
		"highlights do not need window focus"
	);
}

#[test]
fn each_pane_draws_its_own_screen_in_its_own_place() {
	let (left, right) = (frame("left pane\r\n$ ls"), frame("right pane\r\n$ top"));
	let views = [
		base(&right).in_pane(&left, LEFT, LEFT, false),
		base(&right).in_pane(&right, RIGHT, RIGHT, true),
	];
	let both = paint(&views);
	// Each pane alone, drawn at its place, paints the same pixels there.
	for view in views {
		let alone = paint(&[view]);
		let rect = Rect {
			x: view.origin.0,
			y: view.origin.1,
			width: 240,
			height: 80,
		};
		assert_eq!(cut(&both, rect), cut(&alone, rect));
		assert!(cut(&alone, rect).iter().any(|&pixel| pixel != BG), "drawn");
	}
	// The gap between them stays untouched.
	let gap = Rect {
		x: 244,
		y: 0,
		width: 9,
		height: WINDOW.1,
	};
	assert!(cut(&both, gap).iter().all(|&pixel| pixel == BG));
}

#[test]
fn the_same_screen_in_two_panes_differs_only_by_focus() {
	let shown = frame("same\r\n");
	let views = [
		base(&shown).in_pane(&shown, LEFT, LEFT, false),
		base(&shown).in_pane(&shown, RIGHT, RIGHT, true),
	];
	let pixels = paint(&views);
	// The focused pane shows the selection and a solid cursor.
	assert_ne!(cut(&pixels, LEFT), cut(&pixels, RIGHT));
	let plain = GridView {
		selection: None,
		search_match: None,
		focused: false,
		..base(&shown)
	};
	let unfocused_again = paint(&[plain.in_pane(&shown, RIGHT, RIGHT, true)]);
	assert_eq!(cut(&pixels, LEFT), cut(&unfocused_again, RIGHT));
}

#[test]
fn edge_backgrounds_fill_the_pane_padding_and_stop_there() {
	// A red screen extends into its padding: here the window's padding on
	// the left, top and bottom, and its own rect on the divider's side.
	let red = frame("\x1b[?25l\x1b[41m\x1b[2J");
	let padding = Rect {
		x: 0,
		y: 0,
		width: 244,
		height: WINDOW.1,
	};
	let view = base(&red).in_pane(&red, LEFT, padding, false);
	let pixels = paint(&[view]);
	let color = pixels[LEFT.y * WINDOW.0 + LEFT.x];
	assert_ne!(color, BG);
	assert!(cut(&pixels, padding).iter().all(|&pixel| pixel == color));
	let rest = Rect {
		x: 244,
		y: 0,
		width: WINDOW.0 - 244,
		height: WINDOW.1,
	};
	assert!(cut(&pixels, rest).iter().all(|&pixel| pixel == BG));
}
