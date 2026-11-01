//! Images too large for the GPU, shrunk on the CPU: the size of the
//! texture to make of one, the box filter that makes it, and the part of
//! the texture a quad then shows.
use super::quads::{Fill, Quad, QuadPen};
use crate::graphics::{Bitmap, Rect};
use std::{ops::Range, sync::Arc};

/// The size of the texture for a `bitmap`-sized image: its own when it is
/// no wider or taller than the device's `dimension` and fits `room` bytes,
/// four a texel; else the size it is `shown` at whole, never larger than
/// itself or the device makes, halved until it fits. `None` when not even
/// a texel fits.
pub(crate) fn texture_size(
	bitmap: (usize, usize),
	shown: (usize, usize),
	dimension: usize,
	room: usize,
) -> Option<(usize, usize)> {
	let bytes = |(width, height): (usize, usize)| width.saturating_mul(height).saturating_mul(4);
	let (width, height) = bitmap;
	if width <= dimension && height <= dimension && bytes(bitmap) <= room {
		return Some(bitmap);
	}
	// Scales as fractions: the larger of the shown sides', then at most
	// one and what the device makes.
	let fraction = |part: usize, whole: usize| (part as u128, whole.max(1) as u128);
	let less = |a: (u128, u128), b: (u128, u128)| a.0 * b.1 < b.0 * a.1;
	let (across, down) = (fraction(shown.0, width), fraction(shown.1, height));
	let mut scale = if less(across, down) { down } else { across };
	for limit in [(1, 1), fraction(dimension, width.max(height))] {
		if less(limit, scale) {
			scale = limit;
		}
	}
	let most = dimension.max(1) as u128;
	let side = |n: usize| (n as u128 * scale.0).div_ceil(scale.1).clamp(1, most) as usize;
	let mut size = (side(width), side(height));
	while bytes(size) > room {
		if size == (1, 1) {
			return None;
		}
		size = (size.0.div_ceil(2), size.1.div_ceil(2));
	}
	Some(size)
}

/// The pixels of a side `from` long that texel `at` of the side `to` long
/// covers: never none.
fn span(at: usize, from: usize, to: usize) -> Range<usize> {
	let start = at * from / to;
	start..((at + 1) * from).div_ceil(to).max(start + 1)
}

/// `bitmap` shrunk to `size` with a box filter: each texel the average of
/// the pixels it covers, colors weighed by their alpha.
pub(crate) fn shrink(bitmap: &Bitmap, (width, height): (usize, usize)) -> Bitmap {
	let mut pixels = Vec::with_capacity(width * height);
	for y in 0..height {
		let rows = span(y, bitmap.height, height);
		for x in 0..width {
			let columns = span(x, bitmap.width, width);
			// Alpha, then red, green and blue each times its alpha.
			let (mut sums, mut count) = ([0u64; 4], 0);
			for row in rows.clone() {
				for &pixel in &bitmap.pixels[row * bitmap.width..][columns.clone()] {
					let alpha = u64::from(pixel >> 24);
					sums[0] += alpha;
					for (channel, sum) in sums[1..].iter_mut().enumerate() {
						*sum += alpha * u64::from(pixel >> (16 - 8 * channel) & 0xff);
					}
					count += 1;
				}
			}
			let alpha = sums[0];
			let mut texel = ((alpha + count / 2) / count) << 24;
			for (channel, &sum) in sums[1..].iter().enumerate() {
				let value = (sum + alpha / 2).checked_div(alpha).unwrap_or(0);
				texel |= value << (16 - 8 * channel);
			}
			pixels.push(texel as u32);
		}
	}
	Bitmap::new(width, height, pixels)
}

/// `source`, a part of a `bitmap`-sized image, as the same part of its
/// `texture`-sized texture: never empty, and inside it.
pub(crate) fn source_in(source: Rect, bitmap: (usize, usize), texture: (usize, usize)) -> Rect {
	if bitmap == texture {
		return source;
	}
	// The start and length of `at..at + length` of a side `from` long, in
	// one `to` long.
	let carry = |at: usize, length: usize, from: usize, to: usize| {
		let (from, to128) = (from.max(1) as u128, to as u128);
		let start = ((at as u128 * to128 / from) as usize).min(to.saturating_sub(1));
		let end = ((at as u128 + length as u128) * to128).div_ceil(from) as usize;
		(start, end.min(to).saturating_sub(start).max(1))
	};
	let (x, width) = carry(source.x, source.width, bitmap.0, texture.0);
	let (y, height) = carry(source.y, source.height, bitmap.1, texture.1);
	Rect {
		x,
		y,
		width,
		height,
	}
}

/// The size each of `pen`'s images is shown at whole: the largest any
/// placement of its bitmap is drawn at, so placements sharing a texture
/// agree on it.
pub(crate) fn shown_sizes(pen: &QuadPen) -> Vec<(usize, usize)> {
	let whole = |part: usize, shown: usize, side: usize| {
		(side as u128 * shown as u128).div_ceil(part.max(1) as u128) as usize
	};
	let sizes: Vec<_> = (pen.images.iter())
		.map(|image| {
			let placement = &image.placement;
			let (source, bitmap) = (placement.source, &placement.bitmap);
			let (width, height) = placement.size(image.cell);
			let across = whole(source.width, width, bitmap.width);
			(across, whole(source.height, height, bitmap.height))
		})
		.collect();
	(pen.images.iter())
		.map(|image| {
			let shares = (pen.images.iter().zip(&sizes))
				.filter(|(other, _)| Arc::ptr_eq(&other.placement.bitmap, &image.placement.bitmap));
			shares.fold((0, 0), |most, (_, &size)| {
				(most.0.max(size.0), most.1.max(size.1))
			})
		})
		.collect()
}

/// `quad`, showing an image of a `bitmap`-sized bitmap, carried into its
/// `texture`-sized texture; any other quad as it is.
pub(crate) fn in_texture(quad: Quad, bitmap: (usize, usize), texture: (usize, usize)) -> Quad {
	match quad.fill {
		Fill::Image(index, source) => Quad {
			fill: Fill::Image(index, source_in(source, bitmap, texture)),
			..quad
		},
		_ => quad,
	}
}

/// The larger size to make `texture`, a texture of a `bitmap`-sized image
/// now `shown` that large, again at, with `room` bytes and its own;
/// `None` when it would be no larger.
pub(crate) fn regrown(
	texture: (usize, usize),
	bitmap: (usize, usize),
	shown: (usize, usize),
	dimension: usize,
	room: usize,
) -> Option<(usize, usize)> {
	let room = room.saturating_add(texture.0 * texture.1 * 4);
	let size = texture_size(bitmap, shown, dimension, room)?;
	(size.0 > texture.0 || size.1 > texture.1).then_some(size)
}
