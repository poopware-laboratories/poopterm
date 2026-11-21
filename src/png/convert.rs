//! Unfiltered rows to `0xAARRGGBB` pixels: samples of 1 to 16 bits scaled
//! to 8, palettes looked up, and `tRNS` turned into alpha. The common
//! 8-bit formats without transparency have rows of their own.
use super::chunks::Png;

/// Fills `out` (`w` pixels) from `line`, one unfiltered row.
pub(crate) fn row(png: &Png, line: &[u8], out: &mut [u32]) {
	let header = png.header;
	if header.depth == 8 && png.transparency.is_none() {
		let opaque = 0xff00_0000;
		match header.color {
			6 => {
				let pixels = line.as_chunks::<4>().0.iter();
				return pixels
					.zip(out)
					.for_each(|(&[r, g, b, a], o)| *o = u32::from_be_bytes([a, r, g, b]));
			}
			2 => {
				let pixels = line.as_chunks::<3>().0.iter();
				return pixels
					.zip(out)
					.for_each(|(&[r, g, b], o)| *o = u32::from_be_bytes([0xff, r, g, b]));
			}
			0 => {
				return line
					.iter()
					.zip(out)
					.for_each(|(&g, o)| *o = opaque | (u32::from(g) * 0x01_0101));
			}
			4 => {
				let pixels = line.as_chunks::<2>().0.iter();
				return pixels
					.zip(out)
					.for_each(|(&[g, a], o)| *o = u32::from(a) << 24 | (u32::from(g) * 0x01_0101));
			}
			_ => {}
		}
	}
	any(png, line, out);
}

/// Any format, a sample at a time.
fn any(png: &Png, line: &[u8], out: &mut [u32]) {
	let header = png.header;
	let depth = usize::from(header.depth);
	let trns = png.transparency.as_deref().unwrap_or_default();
	// The transparent sample values, when `tRNS` gives them.
	let key = |i: usize| {
		(trns.len() >= 2 * i + 2).then(|| u16::from_be_bytes([trns[2 * i], trns[2 * i + 1]]))
	};
	// The `n`th sample of pixel `x`.
	let sample = |x: usize, n: usize| -> u16 {
		let index = x * header.channels() + n;
		match depth {
			16 => u16::from_be_bytes([line[2 * index], line[2 * index + 1]]),
			8 => u16::from(line[index]),
			_ => {
				let bit = index * depth;
				u16::from(line[bit / 8] >> (8 - depth - bit % 8) & ((1 << depth) - 1) as u8)
			}
		}
	};
	// A sample scaled to eight bits.
	let eight = |value: u16| -> u32 {
		match depth {
			16 => (u32::from(value) + 128) / 257,
			8 => u32::from(value),
			_ => u32::from(value) * 255 / ((1 << depth) - 1),
		}
	};
	for (x, o) in out.iter_mut().enumerate() {
		*o = match header.color {
			0 => {
				let g = sample(x, 0);
				let alpha = if key(0) == Some(g) && trns.len() == 2 {
					0
				} else {
					0xff
				};
				let g = eight(g);
				alpha << 24 | g << 16 | g << 8 | g
			}
			2 => {
				let (r, g, b) = (sample(x, 0), sample(x, 1), sample(x, 2));
				let clear = trns.len() == 6 && [key(0), key(1), key(2)] == [Some(r), Some(g), Some(b)];
				let alpha = if clear { 0 } else { 0xff };
				alpha << 24 | eight(r) << 16 | eight(g) << 8 | eight(b)
			}
			3 => {
				let index = usize::from(sample(x, 0));
				let [r, g, b] = png.palette.get(index).copied().unwrap_or_default();
				let alpha = trns.get(index).copied().unwrap_or(0xff);
				u32::from_be_bytes([alpha, r, g, b])
			}
			4 => {
				let g = eight(sample(x, 0));
				eight(sample(x, 1)) << 24 | g << 16 | g << 8 | g
			}
			_ => {
				let [r, g, b, a] = [0, 1, 2, 3].map(|n| eight(sample(x, n)));
				a << 24 | r << 16 | g << 8 | b
			}
		};
	}
}
