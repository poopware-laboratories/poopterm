//! Undoing PNG's per-row filters: None, Sub, Up, Average and Paeth, each
//! predicting a byte from the one a pixel to the left, the one above, or
//! both.
//!
//! Sub, Average and Paeth make each pixel wait for the one to its left, a
//! chain as slow as the operations in it. So a pixel's bytes go through
//! SIMD lanes together, and runs of such rows are undone along a diagonal:
//! while one row works on pixel x, the row below works on pixel x - 1,
//! which needs nothing the row above has not finished, and the chains of
//! up to [`BAND`] rows overlap.
use std::simd::{
	Select, Simd,
	cmp::SimdPartialOrd,
	num::{SimdInt, SimdUint},
};

use super::Error;

/// Rows undone together along a diagonal.
const BAND: usize = 4;

/// The rows of `data` (each a filter byte and `row` bytes) unfiltered into
/// one buffer of `row`-byte rows; `bpp` (1 to 8) is bytes per pixel.
pub(crate) fn unfilter(data: &[u8], row: usize, bpp: usize) -> Result<Vec<u8>, Error> {
	match bpp {
		1 => rows::<1, 1>(data, row),
		2 => rows::<2, 2>(data, row),
		3 => rows::<3, 4>(data, row),
		4 => rows::<4, 4>(data, row),
		6 => rows::<6, 8>(data, row),
		_ => rows::<8, 8>(data, row),
	}
}

fn rows<const BPP: usize, const L: usize>(data: &[u8], row: usize) -> Result<Vec<u8>, Error> {
	let lines = data.chunks_exact(row + 1);
	let kinds: Vec<u8> = lines.clone().map(|line| line[0]).collect();
	if kinds.iter().any(|&kind| kind > 4) {
		return Err("bad filter type");
	}
	let mut out = Vec::with_capacity(kinds.len() * row);
	lines.for_each(|line| out.extend_from_slice(&line[1..]));
	let mut y = 0;
	while y < kinds.len() {
		match kinds[y] {
			0 => y += 1,
			2 => {
				if y > 0 {
					let (done, rest) = out.split_at_mut(y * row);
					let above = &done[(y - 1) * row..];
					rest[..row]
						.iter_mut()
						.zip(above)
						.for_each(|(c, &b)| *c = c.wrapping_add(b));
				}
				y += 1;
			}
			1 => {
				band::<BPP, L, 1>(&mut out, row, y, [1]);
				y += 1;
			}
			_ => {
				// Average and Paeth rows, up to four, undone together.
				let run = kinds[y..]
					.iter()
					.take(BAND)
					.take_while(|&&kind| matches!(kind, 3 | 4));
				let at = |j: usize| kinds[y + j];
				match run.count() {
					4 => band::<BPP, L, 4>(&mut out, row, y, [at(0), at(1), at(2), at(3)]),
					3 => band::<BPP, L, 3>(&mut out, row, y, [at(0), at(1), at(2)]),
					2 => band::<BPP, L, 2>(&mut out, row, y, [at(0), at(1)]),
					_ => band::<BPP, L, 1>(&mut out, row, y, [at(0)]),
				}
				y += BAND
					.min(
						kinds[y..]
							.iter()
							.take_while(|&&kind| matches!(kind, 3 | 4))
							.count(),
					)
					.max(1);
			}
		}
	}
	Ok(out)
}

/// Undoes the `N` rows from `first` of `out`, filtered with `kinds` (Sub,
/// Average or Paeth), along a diagonal, `BPP` bytes a pixel in `L` lanes.
/// The row above `first` is finished, or there is none.
fn band<const BPP: usize, const L: usize, const N: usize>(
	out: &mut [u8],
	row: usize,
	first: usize,
	kinds: [u8; N],
) {
	// A pixel that fills its lanes loads whole; 3 and 6 bytes are padded.
	let load = |bytes: &[u8]| match BPP == L {
		true => Simd::<u8, L>::from_slice(&bytes[..L]).cast::<i16>(),
		false => Simd::<u8, L>::load_or_default(&bytes[..BPP]).cast::<i16>(),
	};
	let zero = Simd::<i16, L>::splat(0);
	// Each row's pixel to the left and the one above that; none at first.
	let (mut left, mut up_left) = ([zero; N], [zero; N]);
	let mut pixel = |j: usize, x: usize| {
		let at = (first + j) * row + x * BPP;
		let above = if first + j == 0 {
			zero
		} else {
			load(&out[at - row..])
		};
		let (a, c) = (left[j], up_left[j]);
		let predicted = match kinds[j] {
			1 => a,
			3 => (a + above) >> 1,
			_ => paeth(a, above, c),
		};
		let value = (load(&out[at..]) + predicted) & Simd::splat(0xff);
		out[at..at + BPP].copy_from_slice(&value.cast::<u8>().to_array()[..BPP]);
		(left[j], up_left[j]) = (value, above);
	};
	let pixels = row / BPP;
	for step in 0..pixels + N - 1 {
		if step >= N - 1 && step < pixels {
			// Every row has a pixel at this step.
			(0..N).for_each(|j| pixel(j, step - j));
		} else {
			(0..N)
				.filter(|&j| step >= j && step - j < pixels)
				.for_each(|j| pixel(j, step - j));
		}
	}
}
/// Of left `a`, above `b` and upper left `c`, the one nearest
/// `a + b - c`, ties going to `a`, then `b`: with `d = a - c` and
/// `e = b - c` the distances are `|e|`, `|d|` and `|d + e|`.
#[inline(always)]
fn paeth<const L: usize>(a: Simd<i16, L>, b: Simd<i16, L>, c: Simd<i16, L>) -> Simd<i16, L> {
	let (d, e) = (a - c, b - c);
	let (pa, pb, pc) = (e.abs(), d.abs(), (d + e).abs());
	let near_b = pb.simd_le(pc).select(b, c);
	(pa.simd_le(pb) & pa.simd_le(pc)).select(a, near_b)
}
