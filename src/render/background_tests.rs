//! Tests of the window's background: every pixel of the rows asked for,
//! from the column asked for, takes the edge color within the border and
//! the background color or the wallpaper's pixel inside it, and nothing
//! else changes.
use super::{fill, wallpaper};
use crate::{
	gpu::quads::{Fill, QuadPen},
	graphics::{Bitmap, Rect},
	render::painter::Painter,
};
use std::sync::Arc;

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

/// A wallpaper for the inside of a `size` window with this `border`, no
/// two pixels alike, as `Wallpaper::bitmap` makes it: opaque, and at least
/// a pixel.
fn paper((width, height): (usize, usize), border: usize) -> Arc<Bitmap> {
	let inside = |size: usize| size.saturating_sub(2 * border).max(1);
	let (width, height) = (inside(width), inside(height));
	let pixels = (0..width * height)
		.map(|i| 0xff10_0000 + i as u32)
		.collect();
	Arc::new(Bitmap::new(width, height, pixels))
}

/// Fills on a `size` window with a color, then with a wallpaper, and checks
/// every pixel.
fn check(size: (usize, usize), rows: (usize, usize), left: usize, border: usize) {
	let (width, height) = size;
	let image = paper(size, border);
	for papered in [false, true] {
		let mut pixels = vec![BEFORE; width * height];
		let painter = &mut Painter::new(&mut pixels, width, height);
		match papered {
			false => fill(painter, rows, left, border, (EDGE, INSIDE)),
			true => wallpaper(painter, rows, left, border, EDGE, &image),
		}
		for y in 0..height {
			for x in 0..width {
				let want = match expected((x, y), size, rows, left, border) {
					INSIDE if papered => image.pixels[(y - border) * image.width + x - border] & 0xff_ffff,
					want => want,
				};
				assert_eq!(
					pixels[y * width + x],
					want,
					"({x}, {y}) of {size:?}, rows {rows:?}, from {left}, border {border}, \
					 wallpaper {papered}"
				);
			}
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
		let mut pen = QuadPen::new(size, (64, 1));
		fill(&mut pen, (0, usize::MAX), left, border, (EDGE, INSIDE));
		wallpaper(
			&mut pen,
			(0, usize::MAX),
			left,
			border,
			EDGE,
			&paper(size, border),
		);
		assert_eq!(pen.quads.is_empty(), left >= size.0, "{size:?} from {left}");
		for quad in &pen.quads {
			let (x, y) = (quad.at.0 as usize, quad.at.1 as usize);
			let inside = x + quad.size.0 <= size.0 && y + quad.size.1 <= size.1;
			let clip = (quad.clip.2 as usize, quad.clip.3 as usize);
			assert!(inside, "{quad:?} of {size:?}, border {border}");
			assert!(clip.0 <= size.0 && clip.1 <= size.1, "{quad:?} of {size:?}");
		}
	}
}

#[test]
fn the_gpu_gets_the_wallpaper_as_one_image_over_the_inside_clipped_to_the_rows() {
	let (size, border) = ((20, 12), 2);
	let image = paper(size, border);
	let mut pen = QuadPen::new(size, (64, 1));
	wallpaper(&mut pen, (3, 4), 5, border, EDGE, &image);
	let images: Vec<_> = pen
		.quads
		.iter()
		.filter(|quad| matches!(quad.fill, Fill::Image(..)))
		.collect();
	assert_eq!(images.len(), 1, "{:?}", pen.quads);
	let quad = images[0];
	// The whole wallpaper at the inside's corner, shown from column 5 of
	// rows 3 to 6.
	let all = Rect {
		x: 0,
		y: 0,
		width: 16,
		height: 8,
	};
	assert_eq!(quad.fill, Fill::Image(0, all));
	assert_eq!(
		(quad.at, quad.size, quad.clip),
		((2, 2), (16, 8), (5, 3, 18, 7))
	);
	let drawn = &pen.images[0];
	assert!(
		Arc::ptr_eq(&drawn.placement.bitmap, &image),
		"the same bitmap, so the same texture"
	);
	assert_eq!(drawn.placement.source, all);
}

#[test]
fn rows_in_the_border_alone_draw_no_wallpaper() {
	let (size, border) = ((20, 12), 2);
	for rows in [(0, 2), (10, 2), (12, 3), (4, 0)] {
		let mut pen = QuadPen::new(size, (64, 1));
		wallpaper(&mut pen, rows, 0, border, EDGE, &paper(size, border));
		assert!(pen.images.is_empty(), "rows {rows:?}");
	}
	// Nor columns past the inside.
	let mut pen = QuadPen::new(size, (64, 1));
	wallpaper(&mut pen, (0, 12), 18, border, EDGE, &paper(size, border));
	assert!(pen.images.is_empty());
	assert!(!pen.quads.is_empty(), "the right edge is still drawn");
}

#[test]
fn a_wallpaper_rect_paints_what_the_wallpaper_rows_do_there_and_nothing_else() {
	let (size, border) = ((20, 12), 2);
	let image = paper(size, border);
	let mut rows = vec![BEFORE; size.0 * size.1];
	let painter = &mut Painter::new(&mut rows, size.0, size.1);
	wallpaper(painter, (0, size.1), 0, border, EDGE, &image);
	for rect in [
		Rect {
			x: 2,
			y: 2,
			width: 16,
			height: 8,
		},
		Rect {
			x: 5,
			y: 3,
			width: 4,
			height: 6,
		},
		Rect {
			x: 17,
			y: 9,
			width: 1,
			height: 1,
		},
		Rect {
			x: 9,
			y: 4,
			width: 0,
			height: 3,
		},
	] {
		let mut pixels = vec![BEFORE; size.0 * size.1];
		let painter = &mut Painter::new(&mut pixels, size.0, size.1);
		super::fill_inside(painter, rect, border, (INSIDE, Some(&image)));
		for y in 0..size.1 {
			for x in 0..size.0 {
				let inside =
					(rect.x..rect.x + rect.width).contains(&x) && (rect.y..rect.y + rect.height).contains(&y);
				let want = if inside { rows[y * size.0 + x] } else { BEFORE };
				assert_eq!(pixels[y * size.0 + x], want, "({x}, {y}) in {rect:?}");
			}
		}
	}
}

#[test]
fn fill_inside_without_a_wallpaper_paints_the_color_in_the_rect_only() {
	let size = (20, 12);
	let mut pixels = vec![BEFORE; size.0 * size.1];
	let painter = &mut Painter::new(&mut pixels, size.0, size.1);
	let rect = Rect {
		x: 3,
		y: 4,
		width: 5,
		height: 2,
	};
	super::fill_inside(painter, rect, 2, (INSIDE, None));
	for y in 0..size.1 {
		for x in 0..size.0 {
			let inside = (3..8).contains(&x) && (4..6).contains(&y);
			let want = if inside { INSIDE } else { BEFORE };
			assert_eq!(pixels[y * size.0 + x], want, "({x}, {y})");
		}
	}
}
