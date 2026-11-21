//! Our own PNG decoder (PNG specification, third edition): chunks with
//! their CRCs, the zlib stream through `crate::inflate`, the five filters,
//! Adam7 interlacing, and every color type and bit depth, into
//! `0xAARRGGBB` pixels with straight alpha. Ancillary chunks other than
//! `tRNS` (gamma, color profiles, text) are skipped, as the `image` crate
//! it replaces for kitty's `f=100` does.
mod chunks;
mod convert;
mod crc;
mod filter;
#[cfg(test)]
mod format_tests;
#[cfg(test)]
mod hostile_tests;
#[cfg(test)]
mod interlace_tests;
#[cfg(test)]
mod png_bench;
#[cfg(test)]
mod tests;

use chunks::Header;

/// A decoded image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Image {
	pub width: usize,
	pub height: usize,
	pub pixels: Vec<u32>,
}

/// Why a PNG could not be decoded.
pub(crate) type Error = &'static str;

/// Decodes `data` when its sides are at most `max_side` and its pixels at
/// most `max_pixels`; larger images are refused before anything is
/// allocated for them.
pub(crate) fn decode(data: &[u8], max_side: usize, max_pixels: usize) -> Result<Image, Error> {
	let png = chunks::read(data)?;
	let header = png.header;
	let (width, height) = (header.width, header.height);
	if width > max_side || height > max_side || width * height > max_pixels {
		return Err("image too large");
	}
	let size: usize = passes(header)
		.map(|(w, h)| header.row_bytes(w).map_or(0, |row| (row + 1) * h))
		.sum();
	// Streams may hold more than the image needs; the rest is ignored.
	let raw = crate::inflate::zlib(&png.data, size, size + (1 << 16)).ok_or("bad image data")?;
	if raw.len() < size {
		return Err("too little image data");
	}
	let mut pixels = vec![0; width * height];
	let mut at = 0;
	for (pass, (w, h)) in passes(header).enumerate() {
		let Some(row) = header.row_bytes(w) else {
			continue;
		};
		let rows = filter::unfilter(&raw[at..at + (row + 1) * h], row, header.bytes_per_pixel())?;
		at += (row + 1) * h;
		if !header.interlaced {
			let lines = rows.chunks_exact(row).zip(pixels.chunks_exact_mut(width));
			lines.for_each(|(line, out)| convert::row(&png, line, out));
			continue;
		}
		let (x0, y0, dx, dy) = ADAM7[pass];
		let mut out = vec![0; w];
		for (y, line) in rows.chunks_exact(row).enumerate() {
			convert::row(&png, line, &mut out);
			let start = (y0 + y * dy) * width + x0;
			out
				.iter()
				.enumerate()
				.for_each(|(x, &pixel)| pixels[start + x * dx] = pixel);
		}
	}
	Ok(Image {
		width,
		height,
		pixels,
	})
}

/// The CRC a chunk of `kind_and_body` (its type then its data) carries.
#[cfg(test)]
pub(crate) fn crc_of_chunk(kind_and_body: &[u8]) -> u32 {
	crc::crc32(&[kind_and_body])
}

/// Adam7's passes: the first column and row each starts at, and the
/// steps between them.
const ADAM7: [(usize, usize, usize, usize); 7] = [
	(0, 0, 8, 8),
	(4, 0, 8, 8),
	(0, 4, 4, 8),
	(2, 0, 4, 4),
	(0, 2, 2, 4),
	(1, 0, 2, 2),
	(0, 1, 1, 2),
];

/// The width and height of each pass: the whole image when not interlaced.
fn passes(header: Header) -> impl Iterator<Item = (usize, usize)> {
	let (width, height) = (header.width, header.height);
	let whole = std::iter::once((width, height)).filter(move |_| !header.interlaced);
	let count = |size: usize, start: usize, step: usize| size.saturating_sub(start).div_ceil(step);
	let adam7 = ADAM7.iter().filter(move |_| header.interlaced);
	whole.chain(adam7.map(move |&(x0, y0, dx, dy)| (count(width, x0, dx), count(height, y0, dy))))
}
