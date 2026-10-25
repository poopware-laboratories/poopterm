//! Antialiased shapes: rounded rectangles whose corners blend each edge
//! pixel by how much of it the curve covers.
use super::blend::blend;

/// Slices each pixel is measured in, across; within a slice the covered
/// height is exact.
const SLICES: usize = 16;

/// How much of the pixel at `(x, y)` in a corner square `radius` wide lies
/// inside the quarter circle around the square's far corner, out of 255.
pub(crate) fn corner_coverage(radius: usize, x: usize, y: usize) -> u32 {
	let r = radius as f32;
	let covered: f32 = (0..SLICES)
		.map(|slice| {
			let dx = r - (x as f32 + (slice as f32 + 0.5) / SLICES as f32);
			let top = r - (r * r - dx * dx).max(0.0).sqrt();
			(y as f32 + 1.0 - top).clamp(0.0, 1.0)
		})
		.sum();
	(covered / SLICES as f32 * 255.0).round() as u32
}

/// Fills `(x, y, w, h)` with `color`, its corners rounded to `radius` and
/// antialiased, clipped to the `width` by `height` buffer.
pub(crate) fn round_rect(
	pixels: &mut [u32],
	(width, height): (usize, usize),
	(x, y, w, h): (usize, usize, usize, usize),
	radius: usize,
	color: u32,
) {
	let radius = radius.min(h / 2).min(w / 2);
	let mut fill = |x, y, w, h| super::painter::fill_rect(pixels, width, height, x, y, w, h, color);
	fill(x, y + radius, w, h - 2 * radius);
	fill(x + radius, y, w - 2 * radius, radius);
	fill(x + radius, y + h - radius, w - 2 * radius, radius);
	for row in 0..radius {
		for column in 0..radius {
			let coverage = corner_coverage(radius, column, row);
			let (left, right) = (x + column, x + w - 1 - column);
			let (top, bottom) = (y + row, y + h - 1 - row);
			for (px, py) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
				if px < width && py < height {
					let pixel = &mut pixels[py * width + px];
					*pixel = blend(*pixel, color, coverage);
				}
			}
		}
	}
}
