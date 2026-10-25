//! Tests of antialiased shapes: corner coverage against a supersampled
//! reference, and rounded rectangles drawn with it.
use super::{blend::blend, painter::Painter, shapes::corner_coverage};

/// The share of the pixel at `(x, y)` inside the quarter circle of a corner
/// square of `radius`, by sampling it 64 by 64 times, out of 255.
fn sampled(radius: usize, x: usize, y: usize) -> u32 {
	const SAMPLES: usize = 64;
	let r = radius as f64;
	let inside = (0..SAMPLES * SAMPLES)
		.filter(|index| {
			let sx = x as f64 + (index % SAMPLES) as f64 / SAMPLES as f64 + 0.5 / SAMPLES as f64;
			let sy = y as f64 + (index / SAMPLES) as f64 / SAMPLES as f64 + 0.5 / SAMPLES as f64;
			(sx - r).powi(2) + (sy - r).powi(2) <= r * r
		})
		.count();
	(inside as f64 / (SAMPLES * SAMPLES) as f64 * 255.0).round() as u32
}

#[test]
fn coverage_matches_a_supersampled_circle() {
	for radius in 1..=16 {
		for y in 0..radius {
			for x in 0..radius {
				let (got, want) = (corner_coverage(radius, x, y), sampled(radius, x, y));
				assert!(
					got.abs_diff(want) <= 4,
					"radius {radius} at ({x}, {y}): {got} against {want}"
				);
			}
		}
	}
}

#[test]
fn coverage_is_rounded_without_bias() {
	// Truncating would darken every edge pixel by half a step on average.
	let (mut error, mut edges) = (0i64, 0i64);
	for radius in 1..=16 {
		for y in 0..radius {
			for x in 0..radius {
				let want = sampled(radius, x, y);
				if want > 0 && want < 255 {
					error += i64::from(corner_coverage(radius, x, y)) - i64::from(want);
					edges += 1;
				}
			}
		}
	}
	assert!(error.abs() * 4 < edges, "{error} over {edges} edge pixels");
}

#[test]
fn the_corner_is_clear_and_the_pixel_by_the_center_full() {
	for radius in 4..=24 {
		assert_eq!(corner_coverage(radius, 0, 0), 0, "radius {radius}");
		assert_eq!(corner_coverage(radius, radius - 1, radius - 1), 255);
	}
}

#[test]
fn coverage_is_symmetric_about_the_diagonal() {
	for radius in 1..=12 {
		for y in 0..radius {
			for x in 0..radius {
				let (a, b) = (corner_coverage(radius, x, y), corner_coverage(radius, y, x));
				assert!(
					a.abs_diff(b) <= 2,
					"radius {radius} at ({x}, {y}): {a} and {b}"
				);
			}
		}
	}
}

#[test]
fn coverage_never_falls_toward_the_center() {
	for radius in 1..=12 {
		for y in 0..radius {
			for x in 1..radius {
				assert!(corner_coverage(radius, x, y) >= corner_coverage(radius, x - 1, y));
				assert!(corner_coverage(radius, y, x) >= corner_coverage(radius, y, x - 1));
			}
		}
	}
}

#[test]
fn the_covered_area_is_a_quarter_circle() {
	for radius in [3, 6, 12, 20] {
		let total: u32 = (0..radius * radius)
			.map(|index| corner_coverage(radius, index % radius, index / radius))
			.sum();
		let area = total as f64 / 255.0;
		let quarter = std::f64::consts::PI * (radius * radius) as f64 / 4.0;
		assert!(
			(area - quarter).abs() < 0.01 * quarter + 0.1,
			"radius {radius}: {area:.2} against {quarter:.2}"
		);
	}
}

const WIDTH: usize = 40;
const HEIGHT: usize = 30;

/// A `WIDTH` by `HEIGHT` buffer filled with `background`, after `draw`.
fn drawn(background: u32, draw: impl FnOnce(&mut Painter)) -> Vec<u32> {
	let mut pixels = vec![background; WIDTH * HEIGHT];
	draw(&mut Painter::new(&mut pixels, WIDTH, HEIGHT));
	pixels
}

#[test]
fn a_zero_radius_draws_a_plain_rectangle() {
	let round = drawn(0, |p| p.round_rect(3, 4, 20, 10, 0, 0xffffff));
	let plain = drawn(0, |p| p.rect(3, 4, 20, 10, 0xffffff));
	assert!(round == plain);
}

#[test]
fn edges_blend_by_coverage_with_what_is_beneath() {
	let background = 0x404040;
	let pixels = drawn(background, |p| p.round_rect(2, 3, 24, 16, 6, 0xff8000));
	for y in 0..6 {
		for x in 0..6 {
			let want = blend(background, 0xff8000, corner_coverage(6, x, y));
			// All four corners, mirrored.
			for (px, py) in [
				(2 + x, 3 + y),
				(25 - x, 3 + y),
				(2 + x, 18 - y),
				(25 - x, 18 - y),
			] {
				assert_eq!(pixels[py * WIDTH + px], want, "({px}, {py})");
			}
		}
	}
}

#[test]
fn edge_pixels_are_partly_covered() {
	let pixels = drawn(0, |p| p.round_rect(0, 0, 12, 12, 6, 0xffffff));
	let partial = pixels
		.iter()
		.filter(|&&pixel| pixel != 0 && pixel != 0xffffff);
	assert!(
		partial.count() >= 4 * 4,
		"a smooth circle has soft edge pixels"
	);
}

#[test]
fn between_the_corners_rows_and_columns_are_solid() {
	let pixels = drawn(0, |p| p.round_rect(2, 3, 24, 16, 6, 0xffffff));
	let at = |x: usize, y: usize| pixels[y * WIDTH + x];
	for x in 8..20 {
		assert_eq!((at(x, 3), at(x, 18)), (0xffffff, 0xffffff), "column {x}");
	}
	for y in 9..13 {
		assert_eq!((at(2, y), at(25, y)), (0xffffff, 0xffffff), "row {y}");
	}
	assert_eq!((at(1, 10), at(26, 10), at(10, 2), at(10, 19)), (0, 0, 0, 0));
}

#[test]
fn an_odd_sized_circle_keeps_its_middle_row_and_column() {
	let pixels = drawn(0, |p| p.round_rect(0, 0, 7, 7, 3, 0xffffff));
	for index in 0..7 {
		assert_eq!(pixels[3 * WIDTH + index], 0xffffff, "middle row at {index}");
		assert_eq!(
			pixels[index * WIDTH + 3],
			0xffffff,
			"middle column at {index}"
		);
	}
}

#[test]
fn a_radius_past_half_the_size_makes_a_pill() {
	let huge = drawn(0, |p| p.round_rect(2, 2, 30, 12, 100, 0xffffff));
	let half = drawn(0, |p| p.round_rect(2, 2, 30, 12, 6, 0xffffff));
	assert!(huge == half);
}

#[test]
fn shapes_past_the_buffer_are_clipped_like_the_same_shape_drawn_whole() {
	let mut whole = vec![0; 80 * 60];
	Painter::new(&mut whole, 80, 60).round_rect(30, 20, 30, 20, 8, 0xffffff);
	let clipped = drawn(0, |p| p.round_rect(30, 20, 30, 20, 8, 0xffffff));
	for y in 0..HEIGHT {
		for x in 0..WIDTH {
			assert_eq!(clipped[y * WIDTH + x], whole[y * 80 + x], "({x}, {y})");
		}
	}
}

#[test]
fn empty_shapes_draw_nothing() {
	for (w, h) in [(0, 10), (10, 0), (0, 0)] {
		assert!(drawn(0, |p| p.round_rect(4, 4, w, h, 3, 0xffffff)) == vec![0; WIDTH * HEIGHT]);
	}
}
