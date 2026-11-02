//! Tests of the window button icons: their shapes, symmetry, stroke width
//! and scaling, read from the coverage they rasterize to.
use super::{
	grid_case_tests::fonts,
	painter::Painter,
	scene::{Command, Scene},
	window_icons::{WindowIcon, WindowIcon::*, rasterize},
};

/// The icon's coverage as rows, and its side in pixels.
fn icon(kind: WindowIcon, scale: usize) -> (Vec<Vec<u8>>, usize) {
	let glyph = rasterize(kind, scale);
	let side = glyph.metrics.width;
	assert_eq!(glyph.metrics.height, side);
	assert!(glyph.color.is_none());
	(
		glyph.coverage.chunks(side).map(<[u8]>::to_vec).collect(),
		side,
	)
}

/// How much ink a run of pixels holds, in pixels' worth.
fn ink(pixels: impl IntoIterator<Item = u8>) -> f32 {
	pixels.into_iter().map(|c| f32::from(c) / 255.0).sum()
}

fn column(rows: &[Vec<u8>], x: usize) -> Vec<u8> {
	rows.iter().map(|row| row[x]).collect()
}

#[test]
fn icons_are_twelve_pixels_square_times_the_scale() {
	for kind in [Minimize, Maximize, Restore, Close] {
		for scale in 1..=3 {
			let (rows, side) = icon(kind, scale);
			assert_eq!((side, rows.len()), (12 * scale, 12 * scale), "{kind:?}");
		}
	}
}

#[test]
fn icons_leave_a_clear_margin_inside_their_square() {
	// Strokes stay inside 1.25..10.75 units, so the outer pixel ring is empty.
	for kind in [Minimize, Maximize, Restore, Close] {
		let (rows, side) = icon(kind, 2);
		let edge = [0, 1, side - 2, side - 1];
		for &y in &edge {
			assert_eq!(ink(rows[y].iter().copied()), 0.0, "{kind:?} row {y}");
			assert_eq!(ink(column(&rows, y)), 0.0, "{kind:?} column {y}");
		}
	}
}

#[test]
fn minimize_is_a_bar_just_below_the_middle() {
	let (rows, _) = icon(Minimize, 2);
	// 6.5 units, 1.5 wide: rows 11.5 to 14.5 at twice the scale.
	assert_eq!(rows[11][12], 128);
	assert_eq!((rows[12][12], rows[13][12]), (255, 255));
	assert_eq!(rows[14][12], 128);
	for y in (0..11).chain(15..24) {
		assert_eq!(ink(rows[y].iter().copied()), 0.0, "row {y}");
	}
	// From 2 to 10 units with round caps: 1.25 to 10.75.
	assert_eq!(rows[13][1], 0);
	assert!((1..255).contains(&rows[13][2]), "the cap rounds off");
	assert_eq!((rows[13][3], rows[13][20]), (255, 255));
	assert!((1..255).contains(&rows[13][21]), "the cap rounds off");
	assert_eq!(rows[13][22], 0);
}

#[test]
fn maximize_is_a_hollow_square_with_rounded_corners() {
	let (rows, side) = icon(Maximize, 2);
	// The middle of each side is fully inked, the inside is empty.
	for (x, y) in [(12, 4), (12, 19), (4, 12), (19, 12)] {
		assert_eq!(rows[y][x], 255, "({x}, {y})");
	}
	for (y, row) in rows.iter().enumerate().take(18).skip(6) {
		assert!(row[6..18].iter().all(|&c| c == 0), "row {y}");
	}
	// The corner is rounded: its outermost pixel is not fully covered.
	assert!(rows[3][3] < 128, "{}", rows[3][3]);
	// The same from every side.
	for y in 0..side {
		for x in 0..side {
			assert_eq!(rows[y][x], rows[y][side - 1 - x], "({x}, {y})");
			assert_eq!(rows[y][x], rows[x][y], "({x}, {y})");
		}
	}
}

#[test]
fn close_is_a_cross_symmetric_across_both_diagonals() {
	let (rows, side) = icon(Close, 2);
	let middle = side / 2;
	assert_eq!(rows[middle][middle], 255);
	assert_eq!(rows[middle - 1][middle - 1], 255);
	// The arms end at the corners, not the edges' middles.
	assert_eq!(rows[3][middle], 0);
	assert_eq!(rows[middle][3], 0);
	for y in 0..side {
		for x in 0..side {
			assert_eq!(rows[y][x], rows[x][y], "({x}, {y})");
			assert_eq!(rows[y][x], rows[y][side - 1 - x], "({x}, {y})");
		}
	}
}

#[test]
fn restore_is_a_front_square_with_another_behind_it() {
	let (rows, side) = icon(Restore, 2);
	// The front square, lower left: hollow, its sides inked.
	assert_eq!(rows[13][3], 255, "left side");
	assert_eq!(rows[20][9], 255, "bottom side");
	assert_eq!(rows[14][9], 0, "inside");
	// The back square shows only above and to the right of the front one.
	assert_eq!(rows[4][12], 255, "top of the back square");
	assert_eq!(rows[10][19], 255, "right of the back square");
	assert_eq!(rows[18][19], 0, "the back square ends at 8 units");
	assert_eq!(rows[4][5], 0, "the back square starts at 4 units");
	// Unlike the others, it has no mirror symmetry.
	let mirrored = (0..side).all(|y| (0..side).all(|x| rows[y][x] == rows[y][side - 1 - x]));
	assert!(!mirrored);
}

#[test]
fn strokes_are_one_and_a_half_pixels_wide_at_every_scale() {
	for scale in 1..=3 {
		let (rows, side) = icon(Minimize, scale);
		let width = ink(column(&rows, side / 2));
		assert!((width - 1.5 * scale as f32).abs() < 0.1, "{scale}: {width}");
		let (rows, side) = icon(Maximize, scale);
		let width = ink(rows[side / 2][..side / 2].iter().copied());
		assert!((width - 1.5 * scale as f32).abs() < 0.1, "{scale}: {width}");
	}
}

#[test]
fn ink_grows_with_the_square_of_the_scale_less_the_edges() {
	for kind in [Minimize, Maximize, Restore, Close] {
		let total = |scale| {
			let (rows, _) = icon(kind, scale);
			ink(rows.into_iter().flatten())
		};
		let (one, two) = (total(1), total(2));
		// Strokes are lines: twice the scale draws them twice as long and
		// twice as wide.
		assert!((two / one - 4.0).abs() < 0.3, "{kind:?}: {one} then {two}");
	}
}

#[test]
fn edges_are_antialiased_not_stepped() {
	for kind in [Maximize, Restore, Close] {
		let (rows, _) = icon(kind, 1);
		let partial = rows.iter().flatten().filter(|&&c| c > 0 && c < 255).count();
		assert!(partial > 8, "{kind:?}: {partial} partly covered pixels");
	}
}

#[test]
fn a_scene_records_an_icon_and_paints_its_coverage_in_its_color() {
	let mut scene = Scene::new(40, 40);
	scene.icon((4, 6), Close, 2, 0x00ff00);
	let recorded = Command::Icon {
		at: (4, 6),
		icon: Close,
		scale: 2,
		color: 0x00ff00,
	};
	assert_eq!(scene.commands, [recorded]);
	let mut pixels = vec![0; 40 * 40];
	scene.replay(&mut Painter::new(&mut pixels, 40, 40), &mut fonts());
	let at = |x: usize, y: usize| pixels[y * 40 + x];
	assert_eq!(at(4 + 12, 6 + 12), 0x00ff00, "full coverage in the middle");
	assert_eq!(at(4, 6), 0, "nothing in the corner");
	assert!(
		pixels.iter().any(|&p| p != 0 && p != 0x00ff00),
		"edges blend"
	);
	// Nothing outside its 24 pixel square.
	for y in 0..40 {
		for x in 0..40 {
			if !(4..28).contains(&x) || !(6..30).contains(&y) {
				assert_eq!(at(x, y), 0, "({x}, {y})");
			}
		}
	}
}

#[test]
fn the_font_set_rasterizes_each_icon_once_per_scale() {
	let mut fonts = fonts();
	let first = fonts.window_icon(Maximize, 2).coverage.clone();
	assert_eq!(first, rasterize(Maximize, 2).coverage);
	assert_eq!(fonts.window_icon(Maximize, 2).coverage, first);
	assert_eq!(fonts.window_icon(Maximize, 1).metrics.width, 12);
	assert_ne!(fonts.window_icon(Restore, 2).coverage, first);
}
