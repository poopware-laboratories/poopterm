//! Color glyphs from bitmap fonts: the PNG decoded and resampled from its
//! strike's size to the font size.
use super::{Metrics, cbdt::ColorBitmap};

/// Largest side of a resampled glyph, as for outlines.
const MAX_SIDE: usize = 4096;

/// `bitmap` at `px` pixels per em, as `0xAARRGGBB` pixels with straight
/// alpha; `None` when its PNG does not decode.
pub(crate) fn render(bitmap: &ColorBitmap, px: f32) -> Option<(Metrics, Vec<u32>)> {
	let image = crate::graphics::kitty::decode::png(bitmap.png)?;
	let scale = px / f32::from(bitmap.ppem.max(1));
	let side = |pixels: usize| ((pixels as f32 * scale).round() as usize).clamp(1, MAX_SIDE);
	let (width, height) = (side(image.width), side(image.height));
	let metrics = Metrics {
		xmin: (f32::from(bitmap.left) * scale).round() as i32,
		ymin: (f32::from(bitmap.top) * scale).round() as i32 - height as i32,
		width,
		height,
		advance_width: f32::from(bitmap.advance) * scale,
	};
	let source = (image.width, image.height);
	Some((metrics, resample(&image.pixels, source, (width, height))))
}

/// Box-filters `pixels` (rows of `width`) to `to`, weighting colors by
/// alpha so transparent pixels do not darken the edges.
pub(crate) fn resample(
	pixels: &[u32],
	(width, height): (usize, usize),
	to: (usize, usize),
) -> Vec<u32> {
	// The source pixels behind output pixel `i` of `n`: at least one.
	let span =
		|i: usize, from: usize, n: usize| (i * from / n, ((i + 1) * from / n).max(i * from / n + 1));
	let mut out = Vec::with_capacity(to.0 * to.1);
	for y in 0..to.1 {
		let (top, bottom) = span(y, height, to.1);
		for x in 0..to.0 {
			let (left, right) = span(x, width, to.0);
			let (mut sum, mut alpha) = ([0u64; 3], 0u64);
			for row in top..bottom {
				for &pixel in &pixels[row * width + left..row * width + right] {
					let a = u64::from(pixel >> 24);
					alpha += a;
					for (channel, shift) in sum.iter_mut().zip([16, 8, 0]) {
						*channel += u64::from(pixel >> shift & 0xff) * a;
					}
				}
			}
			let count = ((bottom - top) * (right - left)) as u64;
			let rgb = match alpha {
				0 => 0,
				_ => sum
					.iter()
					.fold(0, |rgb, &c| rgb << 8 | ((c + alpha / 2) / alpha) as u32),
			};
			out.push((((alpha + count / 2) / count) as u32) << 24 | rgb);
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	const RED: u32 = 0xffff_0000;
	const BLUE: u32 = 0xff00_00ff;
	const CLEAR: u32 = 0x0000_0000;

	#[test]
	fn the_same_size_keeps_every_pixel() {
		let pixels = [RED, BLUE, 0x80_123456, CLEAR, 0xff_00ff00, 0x01_ffffff];
		assert_eq!(resample(&pixels, (3, 2), (3, 2)), pixels);
	}

	#[test]
	fn shrinking_averages_the_pixels_behind_each_one() {
		let pixels = [RED, BLUE, BLUE, RED];
		assert_eq!(resample(&pixels, (2, 2), (1, 1)), [0xff80_0080]);
		// Rows and columns shrink independently.
		assert_eq!(
			resample(&[RED, RED, BLUE, BLUE], (2, 2), (2, 1)),
			[0xff80_0080; 2]
		);
		assert_eq!(
			resample(&[RED, BLUE, RED, BLUE], (2, 2), (2, 1)),
			[RED, BLUE]
		);
	}

	#[test]
	fn transparent_pixels_do_not_darken_the_colors_they_border() {
		// Averaging straight colors would give a dark half-red.
		let edge = resample(&[RED, CLEAR], (2, 1), (1, 1));
		assert_eq!(edge, [0x80ff_0000]);
		assert_eq!(resample(&[CLEAR; 4], (2, 2), (1, 1)), [CLEAR]);
		// A faint pixel tints the result by its weight only.
		let tinted = resample(&[RED, 0x3300_00ff], (2, 1), (1, 1));
		assert_eq!(tinted, [0x99d5_002b]);
	}

	#[test]
	fn growing_repeats_pixels() {
		assert_eq!(
			resample(&[RED, BLUE], (2, 1), (4, 2)),
			[RED, RED, BLUE, BLUE, RED, RED, BLUE, BLUE]
		);
	}

	#[test]
	fn uneven_shrinks_still_use_every_source_pixel() {
		// Three columns into two: the boxes are one and two pixels wide.
		let out = resample(&[RED, BLUE, BLUE], (3, 1), (2, 1));
		assert_eq!(out, [RED, BLUE]);
		let out = resample(&[RED, RED, BLUE, BLUE, BLUE], (5, 1), (2, 1));
		assert_eq!(out, [RED, BLUE]);
		// Seven into three.
		let out = resample(&[RED; 7], (7, 1), (3, 1));
		assert_eq!(out, [RED; 3]);
	}
}
