//! Tests of the window's plain background: every pixel of the rows asked
//! for, from the column asked for, takes the edge color within the border
//! and the background inside it, and nothing else changes.
use super::fill;
use crate::render::painter::Painter;

const EDGE: u32 = 0x112233;
const INSIDE: u32 = 0x445566;
const BEFORE: u32 = 0xdead00;

/// What `fill` should leave at `(x, y)` of a `width` by `height` window.
fn expected(
	(x, y): (usize, usize),
	(width, height): (usize, usize),
	(top, rows): (usize, usize),
	left: usize,
	border: usize,
) -> u32 {
	if y < top || y >= top.saturating_add(rows) || x < left {
		return BEFORE;
	}
	let near = |at: usize, size: usize| at < border || at >= size.saturating_sub(border);
	if near(x, width) || near(y, height) {
		EDGE
	} else {
		INSIDE
	}
}

/// Fills on a `size` window and checks every pixel.
fn check(size: (usize, usize), rows: (usize, usize), left: usize, border: usize) {
	let (width, height) = size;
	let mut pixels = vec![BEFORE; width * height];
	fill(
		&mut Painter::new(&mut pixels, width, height),
		rows,
		left,
		border,
		(EDGE, INSIDE),
	);
	for y in 0..height {
		for x in 0..width {
			let want = expected((x, y), size, rows, left, border);
			assert_eq!(
				pixels[y * width + x],
				want,
				"({x}, {y}) of {size:?}, rows {rows:?}, from {left}, border {border}"
			);
		}
	}
}

#[test]
fn the_whole_window_is_its_border_around_the_background() {
	for border in [0, 1, 3] {
		check((20, 12), (0, 12), 0, border);
	}
}

#[test]
fn only_the_rows_asked_for_are_filled() {
	for rows in [
		(0, 1),
		(0, 2),
		(1, 3),
		(5, 2),
		(10, 2),
		(11, 5),
		(12, 4),
		(30, 2),
	] {
		check((20, 12), rows, 0, 2);
	}
}

#[test]
fn rows_reaching_past_the_bottom_or_overflowing_stop_at_it() {
	check((20, 12), (4, usize::MAX), 0, 2);
	check((20, 12), (usize::MAX, 3), 0, 2);
}

#[test]
fn only_the_columns_from_left_are_filled() {
	for left in [1, 2, 7, 17, 18, 19, 20, 40] {
		check((20, 12), (0, 12), left, 2);
	}
}

#[test]
fn a_border_wider_than_half_the_window_leaves_no_inside() {
	for border in [6, 7, 10, 25] {
		check((20, 12), (0, 12), 0, border);
		check((13, 9), (0, 9), 3, border);
	}
}

#[test]
fn an_empty_window_or_row_range_fills_nothing() {
	check((0, 0), (0, 4), 0, 2);
	check((20, 12), (3, 0), 0, 2);
	check((20, 1), (0, 1), 0, 0);
}

#[test]
fn every_quad_stays_inside_the_window_however_wide_the_border() {
	// A tall narrow window has rows inside its border but no columns.
	let cases = [
		((20, 12), 25, 0),
		((3, 20), 5, 0),
		((13, 9), 7, 3),
		((20, 12), 2, 40),
	];
	for (size, border, left) in cases {
		let mut pen = crate::gpu::quads::QuadPen::new(size, (64, 1));
		fill(&mut pen, (0, usize::MAX), left, border, (EDGE, INSIDE));
		assert_eq!(pen.quads.is_empty(), left >= size.0, "{size:?} from {left}");
		for quad in &pen.quads {
			let (x, y) = (quad.at.0 as usize, quad.at.1 as usize);
			let inside = x + quad.size.0 <= size.0 && y + quad.size.1 <= size.1;
			assert!(inside, "{quad:?} of {size:?}, border {border}");
		}
	}
}
