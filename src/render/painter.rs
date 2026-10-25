//! Rectangle, button and text rasterization into a 0RGB pixel buffer.
use super::{
	blend::{blend, blend_coverage_row},
	glyphs::{FontSet, Glyph},
};

/// Fills a clipped rectangle with a solid color.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fill_rect(
	px: &mut [u32],
	w: usize,
	h: usize,
	x: usize,
	y: usize,
	rw: usize,
	rh: usize,
	color: u32,
) {
	let x_end = x.saturating_add(rw).min(w);
	if x >= x_end {
		return;
	}
	let height_end = y.saturating_add(rh).min(h);
	for yy in y..height_end {
		let row_end = yy * w + x_end;
		if row_end > px.len() {
			break;
		}
		px[yy * w + x..row_end].fill(color);
	}
}

/// Colors for a bordered, rounded button.
#[derive(Clone, Copy)]
pub(crate) struct ButtonStyle {
	pub scale: usize,
	pub background: u32,
	pub border: u32,
	pub accent: u32,
}

/// A borrowed frame buffer with drawing helpers. Coordinates are physical
/// pixels; everything is clipped to the buffer.
pub(crate) struct Painter<'a> {
	pub pixels: &'a mut [u32],
	pub width: usize,
	pub height: usize,
}

impl<'a> Painter<'a> {
	pub(crate) fn new(pixels: &'a mut [u32], width: usize, height: usize) -> Self {
		debug_assert!(pixels.len() >= width * height);
		Self {
			pixels,
			width,
			height,
		}
	}

	pub(crate) fn fill(&mut self, color: u32) {
		self.pixels.fill(color);
	}

	pub(crate) fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
		fill_rect(self.pixels, self.width, self.height, x, y, w, h, color);
	}

	#[allow(clippy::too_many_arguments)]
	pub(crate) fn round_rect(
		&mut self,
		x: usize,
		y: usize,
		w: usize,
		h: usize,
		radius: usize,
		color: u32,
	) {
		let size = (self.width, self.height);
		super::shapes::round_rect(self.pixels, size, (x, y, w, h), radius, color);
	}

	pub(crate) fn button(
		&mut self,
		(x, y, w, h): (usize, usize, usize, usize),
		style: ButtonStyle,
		hovered: bool,
	) {
		let s = style.scale;
		let (outline, face) = if hovered {
			(style.accent, blend(style.background, style.accent, 52))
		} else {
			(
				blend(style.background, style.border, 90),
				blend(style.background, style.border, 38),
			)
		};
		self.round_rect(x, y, w, h, 7 * s, outline);
		self.round_rect(
			x + s,
			y + s,
			w.saturating_sub(2 * s),
			h.saturating_sub(2 * s),
			6 * s,
			face,
		);
	}

	/// Draws a line of text whose top edge is `y`; `ascent` is the baseline offset.
	#[allow(clippy::too_many_arguments)]
	pub(crate) fn text(
		&mut self,
		fonts: &mut FontSet,
		x: usize,
		y: usize,
		text: &str,
		px: f32,
		ascent: i32,
		color: u32,
	) {
		self.text_clipped(
			fonts,
			x,
			y,
			text,
			px,
			ascent,
			color,
			(0, 0, self.width, self.height),
		);
	}

	/// Draws a line of text clipped to the given pixel rectangle.
	#[allow(clippy::too_many_arguments)]
	pub(crate) fn text_clipped(
		&mut self,
		fonts: &mut FontSet,
		x: usize,
		y: usize,
		text: &str,
		px: f32,
		ascent: i32,
		color: u32,
		(clip_x, clip_y, clip_width, clip_height): (usize, usize, usize, usize),
	) {
		let clip_right = clip_x.saturating_add(clip_width).min(self.width) as i32;
		let clip_bottom = clip_y.saturating_add(clip_height).min(self.height) as i32;
		let clip = (clip_x as i32, clip_y as i32, clip_right, clip_bottom);
		let mut cursor_x = x as i32;
		for ch in text.chars() {
			let glyph = fonts.glyph(ch, px);
			let origin = (cursor_x + glyph.metrics.xmin, glyph_top(glyph, y, ascent));
			self.glyph(glyph, origin, clip, false, color);
			cursor_x += glyph.metrics.advance_width.ceil() as i32;
			if cursor_x >= clip_right {
				break;
			}
		}
	}

	/// Blends `glyph` with its top-left corner at `(x, y)`, clipped to
	/// `clip = (left, top, right, bottom)`, in `color` unless it has its own. `italic` shears rows to simulate
	/// slanted text.
	pub(crate) fn glyph(
		&mut self,
		glyph: &Glyph,
		origin: (i32, i32),
		clip: (i32, i32, i32, i32),
		italic: bool,
		color: u32,
	) {
		#[cfg(target_arch = "x86_64")]
		if crate::cpu::has_v3() {
			// SAFETY: the CPU supports the features.
			return unsafe { self.glyph_v3(glyph, origin, clip, italic, color) };
		}
		self.glyph_with(glyph, origin, clip, italic, color);
	}

	/// # Safety
	///
	/// The CPU must have the features [`crate::cpu::has_v3`] checks.
	#[cfg(target_arch = "x86_64")]
	#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,fma")]
	unsafe fn glyph_v3(
		&mut self,
		glyph: &Glyph,
		origin: (i32, i32),
		clip: (i32, i32, i32, i32),
		italic: bool,
		color: u32,
	) {
		self.glyph_with(glyph, origin, clip, italic, color);
	}

	/// The body of [`Painter::glyph`], compiled into each variant.
	#[inline(always)]
	fn glyph_with(
		&mut self,
		glyph: &Glyph,
		(x, y): (i32, i32),
		(clip_left, clip_top, clip_right, clip_bottom): (i32, i32, i32, i32),
		italic: bool,
		color: u32,
	) {
		let metrics = &glyph.metrics;
		let top = y.max(clip_top).max(0);
		let bottom = (y + metrics.height as i32)
			.min(clip_bottom)
			.min(self.height as i32);
		let clip_left = clip_left.max(0);
		let clip_right = clip_right.min(self.width as i32);
		for py in top..bottom {
			let gy = (py - y) as usize;
			// Emoji keep their shape in italic text.
			let shift = if italic && glyph.color.is_none() {
				(metrics.height.saturating_sub(gy) / 6) as i32
			} else {
				0
			};
			let left = x + shift;
			let draw_left = left.max(clip_left);
			let draw_right = (left + metrics.width as i32).min(clip_right);
			if draw_left >= draw_right {
				continue;
			}
			let len = (draw_right - draw_left) as usize;
			let src = gy * metrics.width + (draw_left - left) as usize;
			let dst = py as usize * self.width + draw_left as usize;
			let target = &mut self.pixels[dst..dst + len];
			match &glyph.color {
				Some(pixels) => {
					for (out, &pixel) in target.iter_mut().zip(&pixels[src..src + len]) {
						*out = blend(*out, pixel & 0xff_ffff, pixel >> 24);
					}
				}
				None => blend_coverage_row(target, &glyph.coverage[src..src + len], color),
			}
		}
	}
}

/// Y coordinate of a glyph bitmap's top row for a line whose top edge is `y`.
#[inline]
pub(crate) fn glyph_top(glyph: &Glyph, y: usize, ascent: i32) -> i32 {
	y as i32 + ascent - glyph.metrics.ymin - glyph.metrics.height as i32
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rectangles_are_clipped_to_the_buffer() {
		let mut pixels = vec![0u32; 4 * 3];
		let mut painter = Painter::new(&mut pixels, 4, 3);
		painter.rect(2, 1, 10, 10, 7);
		assert_eq!(pixels, [0, 0, 0, 0, 0, 0, 7, 7, 0, 0, 7, 7]);
	}

	#[test]
	fn glyphs_are_clipped_to_a_rectangle() {
		let glyph = Glyph {
			metrics: crate::fonts::Metrics {
				width: 2,
				height: 3,
				..Default::default()
			},
			coverage: vec![255; 6],
			color: None,
		};
		let mut pixels = vec![0u32; 3 * 4];
		let mut painter = Painter::new(&mut pixels, 3, 4);
		painter.glyph(&glyph, (0, 0), (1, 1, 3, 2), false, 0xffffff);
		assert_eq!(
			pixels,
			[0, 0, 0, 0, 0xffffff, 0, 0, 0, 0, 0, 0, 0],
			"only column 1 of row 1 is inside both the glyph and the clip"
		);
	}

	#[test]
	fn text_is_clipped_to_its_tab_label_area() {
		let font = crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap();
		let mut fonts = FontSet::new(font, Vec::new());
		let mut pixels = vec![0u32; 30 * 20];
		let mut painter = Painter::new(&mut pixels, 30, 20);
		painter.text_clipped(
			&mut fonts,
			0,
			2,
			"MMMMMMMM",
			14.0,
			12,
			0xffffff,
			(0, 0, 8, 20),
		);

		assert!(pixels.iter().any(|pixel| *pixel != 0), "text is visible");
		assert!(
			pixels
				.iter()
				.enumerate()
				.all(|(index, pixel)| index % 30 < 8 || *pixel == 0)
		);
	}

	#[test]
	fn rounded_rectangles_inset_their_corners() {
		let mut pixels = vec![0u32; 8 * 8];
		Painter::new(&mut pixels, 8, 8).round_rect(0, 0, 8, 8, 3, 1);
		assert_eq!(pixels[0], 0, "corner stays clear");
		assert_eq!(pixels[4 * 8], 1, "middle row is filled edge to edge");
	}

	fn color_glyph(width: usize, pixels: Vec<u32>) -> Glyph {
		Glyph {
			metrics: crate::fonts::Metrics {
				width,
				height: pixels.len() / width,
				..Default::default()
			},
			coverage: Vec::new(),
			color: Some(pixels),
		}
	}

	#[test]
	fn color_glyphs_draw_their_own_colors_not_the_text_color() {
		let glyph = color_glyph(2, vec![0xffff0000, 0xff00ff00, 0xff0000ff, 0x00ffffff]);
		let mut pixels = vec![0x101010u32; 3 * 2];
		Painter::new(&mut pixels, 3, 2).glyph(&glyph, (1, 0), (0, 0, 3, 2), false, 0xffffff);
		assert_eq!(
			pixels,
			[0x101010, 0xff0000, 0x00ff00, 0x101010, 0x0000ff, 0x101010]
		);
	}

	#[test]
	fn translucent_color_pixels_blend_with_what_is_below() {
		let glyph = color_glyph(1, vec![0x80ffffff]);
		let mut pixels = vec![0u32];
		Painter::new(&mut pixels, 1, 1).glyph(&glyph, (0, 0), (0, 0, 1, 1), false, 0);
		assert_eq!(pixels, [blend(0, 0xffffff, 0x80)]);
	}

	#[test]
	fn color_glyphs_are_clipped_like_others() {
		let glyph = color_glyph(3, vec![0xff00ff00; 9]);
		let mut pixels = vec![0u32; 4 * 4];
		Painter::new(&mut pixels, 4, 4).glyph(&glyph, (-1, 2), (0, 0, 1, 3), false, 0);
		let drawn: Vec<usize> = (0..16).filter(|&i| pixels[i] != 0).collect();
		assert_eq!(drawn, [8], "column 0 of row 2 only");
	}

	#[test]
	fn italic_text_shears_outlines_but_not_color_glyphs() {
		let mut upright = vec![0u32; 6 * 6];
		let mut italic = upright.clone();
		let glyph = color_glyph(2, vec![0xff00ff00; 12]);
		Painter::new(&mut upright, 6, 6).glyph(&glyph, (0, 0), (0, 0, 6, 6), false, 0);
		Painter::new(&mut italic, 6, 6).glyph(&glyph, (0, 0), (0, 0, 6, 6), true, 0);
		assert_eq!(upright, italic);
		let outline = Glyph {
			metrics: glyph.metrics,
			coverage: vec![255; 12],
			color: None,
		};
		let mut sheared = vec![0u32; 6 * 6];
		Painter::new(&mut sheared, 6, 6).glyph(&outline, (0, 0), (0, 0, 6, 6), true, 0xffffff);
		assert_eq!(sheared[0], 0, "the top row moved right");
		assert_eq!(sheared[1], 0xffffff);
	}

	/// Where a glyph is drawn, its clip, italic and text color.
	#[cfg(target_arch = "x86_64")]
	type Style = ((i32, i32), (i32, i32, i32, i32), bool, u32);

	/// Draws `glyph` over `background` (`width` pixels wide) with the
	/// baseline body and with the v3 variant; returns both results.
	#[cfg(target_arch = "x86_64")]
	fn both_variants(
		background: &[u32],
		width: usize,
		glyph: &Glyph,
		(origin, clip, italic, color): Style,
	) -> (Vec<u32>, Vec<u32>) {
		let height = background.len() / width;
		let (mut baseline, mut v3) = (background.to_vec(), background.to_vec());
		Painter::new(&mut baseline, width, height).glyph_with(glyph, origin, clip, italic, color);
		// SAFETY: the callers check `has_v3`.
		unsafe { Painter::new(&mut v3, width, height).glyph_v3(glyph, origin, clip, italic, color) };
		(baseline, v3)
	}

	/// Coverage and alpha values that reach each blending path: none, full,
	/// and in between.
	#[cfg(target_arch = "x86_64")]
	fn alpha(rng: &mut crate::bench::corpus::Rng) -> u8 {
		match rng.below(3) {
			0 => 0,
			1 => 255,
			_ => rng.next() as u8,
		}
	}

	#[cfg(target_arch = "x86_64")]
	#[test]
	fn v3_glyphs_match_the_baseline_on_random_glyphs_clips_and_backgrounds() {
		if !crate::cpu::has_v3() {
			return;
		}
		let mut rng = crate::bench::corpus::Rng::new(0x9e7);
		for case in 0..4_000 {
			let (width, height) = (1 + rng.below(40), 1 + rng.below(12));
			let (w, h) = (1 + rng.below(30), 1 + rng.below(24));
			let coverage: Vec<u8> = (0..w * h).map(|_| alpha(&mut rng)).collect();
			let glyph = if rng.below(4) == 0 {
				let pixels = coverage
					.iter()
					.map(|&a| u32::from(a) << 24 | rng.next() as u32 >> 8);
				color_glyph(w, pixels.collect())
			} else {
				Glyph {
					metrics: crate::fonts::Metrics {
						width: w,
						height: h,
						..Default::default()
					},
					coverage,
					color: None,
				}
			};
			let at = |rng: &mut crate::bench::corpus::Rng, size: usize| rng.below(size + 20) as i32 - 10;
			let origin = (at(&mut rng, width), at(&mut rng, height));
			let clip = (
				at(&mut rng, width),
				at(&mut rng, height),
				at(&mut rng, width),
				at(&mut rng, height),
			);
			let background: Vec<u32> = (0..width * height)
				.map(|_| rng.next() as u32 & 0xff_ffff)
				.collect();
			let style = (
				origin,
				clip,
				rng.below(2) == 0,
				rng.next() as u32 & 0xff_ffff,
			);
			let (baseline, v3) = both_variants(&background, width, &glyph, style);
			assert_eq!(
				v3, baseline,
				"case {case}: {width}x{height}, glyph {w}x{h}, {style:?}"
			);
		}
	}

	#[cfg(target_arch = "x86_64")]
	#[test]
	fn v3_glyphs_match_the_baseline_for_every_row_length_and_alpha_run() {
		// Rows of 1 to 33 pixels at each offset, so every chunk and remainder
		// length meets runs of no, full and partial coverage.
		if !crate::cpu::has_v3() {
			return;
		}
		for len in 1..=33 {
			for offset in 0..8 {
				for pattern in [[0, 0, 0, 0], [255; 4], [0, 255, 128, 7], [1, 254, 255, 0]] {
					let coverage: Vec<u8> = (0..len).map(|i| pattern[i / 4 % 4]).collect();
					let glyph = Glyph {
						metrics: crate::fonts::Metrics {
							width: len,
							height: 1,
							..Default::default()
						},
						coverage,
						color: None,
					};
					let background: Vec<u32> = (0..48).map(|i| 0x0001_0203 * i).collect();
					let style = ((offset, 0), (0, 0, 48, 1), false, 0xc0_ffee);
					let (baseline, v3) = both_variants(&background, 48, &glyph, style);
					assert_eq!(v3, baseline, "{len} pixels at {offset}, {pattern:?}");
				}
			}
		}
	}

	#[cfg(target_arch = "x86_64")]
	#[test]
	fn the_dispatched_glyph_draws_like_both_variants() {
		// Whichever variant `glyph` picks on this CPU, it matches the body.
		let glyph = color_glyph(
			3,
			vec![
				0x80ff_0000,
				0xff00_ff00,
				0x0000_00ff,
				0x40ff_ffff,
				0xff00_0000,
				0x7f12_3456,
			],
		);
		let mut dispatched = vec![0x33_3333; 5 * 3];
		let mut body = dispatched.clone();
		Painter::new(&mut dispatched, 5, 3).glyph(&glyph, (1, 0), (0, 0, 5, 3), false, 0);
		Painter::new(&mut body, 5, 3).glyph_with(&glyph, (1, 0), (0, 0, 5, 3), false, 0);
		assert_eq!(dispatched, body);
		assert_eq!(body[1], 0x99_1919, "half red over gray");
	}
}
