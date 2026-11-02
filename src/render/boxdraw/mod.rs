//! Box drawing, block elements, Braille, sextants and Powerline separators,
//! drawn in code at the exact cell size instead of taken from a font, so
//! lines meet their neighbors and blocks tile without seams at any size.
mod blocks;
mod compact;
mod lines;
mod powerline;

pub(crate) use compact::{Drawn, compact};

/// Subsamples per pixel along each axis for curves and diagonals.
const SAMPLES: usize = 8;

/// Whether `ch` is drawn here rather than by a font.
#[inline]
pub(crate) fn covers(ch: char) -> bool {
	matches!(
		ch as u32,
		0x2500..=0x259f | 0x2800..=0x28ff | 0x1fb00..=0x1fb3b | 0xe0b0..=0xe0bf
	)
}

/// The coverage bitmap of `ch` filling a `w`×`h` cell, row by row, or `None`
/// for characters left to fonts.
pub(crate) fn draw(ch: char, (w, h): (usize, usize)) -> Option<Vec<u8>> {
	if !covers(ch) || w == 0 || h == 0 {
		return None;
	}
	let mut canvas = Canvas {
		w,
		h,
		pixels: vec![0; w * h],
	};
	let code = ch as u32;
	match code {
		0x2500..=0x257f => lines::draw(&mut canvas, code),
		0x2580..=0x259f => blocks::block(&mut canvas, code),
		0x2800..=0x28ff => blocks::braille(&mut canvas, code as u8),
		0x1fb00..=0x1fb3b => blocks::sextant(&mut canvas, code - 0x1fb00),
		_ => powerline::draw(&mut canvas, code),
	}
	Some(canvas.pixels)
}

/// The thickness of a light line in a cell `w` pixels wide.
fn light_thickness(w: usize) -> usize {
	((w + 4) / 8).max(1)
}

/// A cell-sized coverage bitmap that shapes are painted onto.
struct Canvas {
	w: usize,
	h: usize,
	pixels: Vec<u8>,
}

impl Canvas {
	/// Fills the pixel rectangle `x0..x1` × `y0..y1` with `coverage`, clipped.
	fn fill(&mut self, (x0, x1): (usize, usize), (y0, y1): (usize, usize), coverage: u8) {
		let (x1, y1) = (x1.min(self.w), y1.min(self.h));
		if x0 >= x1 {
			return;
		}
		for y in y0..y1 {
			self.pixels[y * self.w + x0..y * self.w + x1].fill(coverage);
		}
	}

	/// Adds the antialiased area where `inside(x, y)` holds, with `x` and `y`
	/// in pixels from the top left corner.
	fn shape(&mut self, inside: impl Fn(f32, f32) -> bool) {
		let step = 1.0 / SAMPLES as f32;
		for y in 0..self.h {
			for x in 0..self.w {
				let mut hits = 0;
				for sy in 0..SAMPLES {
					let py = y as f32 + (sy as f32 + 0.5) * step;
					for sx in 0..SAMPLES {
						hits += usize::from(inside(x as f32 + (sx as f32 + 0.5) * step, py));
					}
				}
				let coverage = (hits * 255 / (SAMPLES * SAMPLES)) as u8;
				let pixel = &mut self.pixels[y * self.w + x];
				*pixel = (*pixel).max(coverage);
			}
		}
	}
}

/// The distance from `(x, y)` to the segment from `a` to `b`.
pub(crate) fn segment_distance((x, y): (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
	let (dx, dy) = (b.0 - a.0, b.1 - a.1);
	let length = dx * dx + dy * dy;
	let t = if length > 0.0 {
		(((x - a.0) * dx + (y - a.1) * dy) / length).clamp(0.0, 1.0)
	} else {
		0.0
	};
	let (px, py) = (a.0 + t * dx - x, a.1 + t * dy - y);
	(px * px + py * py).sqrt()
}

#[cfg(test)]
mod tests;
