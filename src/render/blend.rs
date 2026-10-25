//! Color blending, vectorized with portable SIMD.
use std::simd::u32x4;

#[inline(always)]
pub(crate) fn blend(bg: u32, fg: u32, a: u32) -> u32 {
	if a == 0 {
		return bg;
	}
	if a == 255 {
		return fg;
	}
	let inverse = 255 - a;
	let red = (((fg >> 16 & 0xff) * a + (bg >> 16 & 0xff) * inverse) / 255) << 16;
	let green = (((fg >> 8 & 0xff) * a + (bg >> 8 & 0xff) * inverse) / 255) << 8;
	let blue = ((fg & 0xff) * a + (bg & 0xff) * inverse) / 255;
	red | green | blue
}

#[inline(always)]
pub(crate) fn blend_coverage_row(pixels: &mut [u32], coverage: &[u8], fg: u32) {
	let (pixel_chunks, pixel_remainder) = pixels.as_chunks_mut::<4>();
	let (coverage_chunks, coverage_remainder) = coverage.as_chunks::<4>();
	for (background, alpha) in pixel_chunks.iter_mut().zip(coverage_chunks) {
		if alpha.iter().all(|&value| value == 0) {
			continue;
		}
		if alpha.iter().all(|&value| value == u8::MAX) {
			*background = [fg; 4];
		} else {
			let background_vector = u32x4::from_array(*background);
			let alpha = u32x4::from_array(alpha.map(u32::from));
			*background = blend_chunk(background_vector, u32x4::splat(fg), alpha).to_array();
		}
	}
	for (pixel, alpha) in pixel_remainder.iter_mut().zip(coverage_remainder) {
		*pixel = blend(*pixel, fg, *alpha as u32);
	}
}

#[inline(always)]
pub(crate) fn blend_chunk(bg: u32x4, fg: u32x4, a: u32x4) -> u32x4 {
	let inv_a_v = u32x4::splat(255) - a;
	let max_a = u32x4::splat(255);
	let fg_r = (fg >> u32x4::splat(16)) & u32x4::splat(0xff);
	let bg_r = (bg >> u32x4::splat(16)) & u32x4::splat(0xff);
	let r = ((fg_r * a + bg_r * inv_a_v) / max_a) << u32x4::splat(16);
	let fg_g = (fg >> u32x4::splat(8)) & u32x4::splat(0xff);
	let bg_g = (bg >> u32x4::splat(8)) & u32x4::splat(0xff);
	let g = ((fg_g * a + bg_g * inv_a_v) / max_a) << u32x4::splat(8);
	let fg_b = fg & u32x4::splat(0xff);
	let bg_b = bg & u32x4::splat(0xff);
	let b = (fg_b * a + bg_b * inv_a_v) / max_a;
	r | g | b
}
