//! A PNG's chunks: the signature, `IHDR`, `PLTE`, `tRNS`, the `IDAT`s
//! joined, and `IEND`, each checked against its CRC.
use super::{Error, crc::crc32};

const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
/// Most compressed image data accepted: more than the largest image any
/// caller allows needs.
const MAX_DATA: usize = 1 << 30;

/// What `IHDR` says of the image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
	pub width: usize,
	pub height: usize,
	pub depth: u8,
	/// 0 gray, 2 RGB, 3 palette, 4 gray and alpha, 6 RGBA.
	pub color: u8,
	pub interlaced: bool,
}

impl Header {
	/// Samples per pixel.
	pub(crate) fn channels(self) -> usize {
		match self.color {
			2 => 3,
			4 => 2,
			6 => 4,
			_ => 1,
		}
	}

	/// Bytes the filters treat as one pixel: at least one.
	pub(crate) fn bytes_per_pixel(self) -> usize {
		(self.channels() * usize::from(self.depth)).div_ceil(8)
	}

	/// Bytes in a row of `width` pixels, without its filter byte; `None`
	/// for an empty pass.
	pub(crate) fn row_bytes(self, width: usize) -> Option<usize> {
		(width > 0).then(|| (width * self.channels() * usize::from(self.depth)).div_ceil(8))
	}
}

/// The chunks that matter for decoding.
pub(crate) struct Png {
	pub header: Header,
	/// `PLTE` as RGB triples.
	pub palette: Vec<[u8; 3]>,
	/// `tRNS`: alphas for palette entries, or the transparent gray or RGB.
	pub transparency: Option<Vec<u8>>,
	/// The `IDAT` chunks joined.
	pub data: Vec<u8>,
}

/// Reads and checks the chunks of `data`, up to `IEND`.
pub(crate) fn read(data: &[u8]) -> Result<Png, Error> {
	let mut rest = data.strip_prefix(SIGNATURE).ok_or("not a PNG")?;
	let mut header = None;
	let (mut palette, mut transparency, mut image) = (Vec::new(), None, Vec::new());
	loop {
		let (&length, after) = rest.split_first_chunk::<4>().ok_or("truncated")?;
		let length = u32::from_be_bytes(length) as usize;
		if length > i32::MAX as usize || after.len() < length + 8 {
			return Err("truncated");
		}
		let (kind, body) = (&after[..4], &after[4..4 + length]);
		let crc = u32::from_be_bytes(after[4 + length..8 + length].try_into().unwrap_or_default());
		if crc32(&[kind, body]) != crc {
			return Err("bad chunk CRC");
		}
		rest = &after[8 + length..];
		if header.is_none() && kind != b"IHDR" {
			return Err("IHDR is not first");
		}
		match kind {
			b"IHDR" if header.is_none() => header = Some(parse_header(body)?),
			b"PLTE" if image.is_empty() => {
				if body.is_empty() || body.len() % 3 != 0 || body.len() > 768 {
					return Err("bad palette");
				}
				palette = body.as_chunks::<3>().0.to_vec();
			}
			b"tRNS" if image.is_empty() => transparency = Some(body.to_vec()),
			b"IDAT" => {
				if image.len() + body.len() > MAX_DATA {
					return Err("too much image data");
				}
				image.extend_from_slice(body);
			}
			b"IEND" => break,
			// Ancillary chunks have a lower-case first letter.
			_ if kind[0].is_ascii_lowercase() => {}
			_ => return Err("unknown critical chunk"),
		}
	}
	let header = header.ok_or("no IHDR")?;
	if header.color == 3 && palette.is_empty() {
		return Err("no palette");
	}
	if image.is_empty() {
		return Err("no image data");
	}
	// More alphas than palette entries break the spec: ignored as a whole.
	if header.color == 3
		&& transparency
			.as_ref()
			.is_some_and(|t| t.len() > palette.len())
	{
		transparency = None;
	}
	Ok(Png {
		header,
		palette,
		transparency,
		data: image,
	})
}

fn parse_header(body: &[u8]) -> Result<Header, Error> {
	let &[
		w0,
		w1,
		w2,
		w3,
		h0,
		h1,
		h2,
		h3,
		depth,
		color,
		compression,
		filter,
		interlace,
	] = body
	else {
		return Err("bad IHDR");
	};
	let (width, height) = (
		u32::from_be_bytes([w0, w1, w2, w3]),
		u32::from_be_bytes([h0, h1, h2, h3]),
	);
	let depths: &[u8] = match color {
		0 => &[1, 2, 4, 8, 16],
		3 => &[1, 2, 4, 8],
		2 | 4 | 6 => &[8, 16],
		_ => return Err("bad color type"),
	};
	let limit = 1 << 31;
	if width == 0 || height == 0 || width >= limit || height >= limit {
		return Err("bad size");
	}
	if !depths.contains(&depth) || compression != 0 || filter != 0 || interlace > 1 {
		return Err("bad IHDR");
	}
	Ok(Header {
		width: width as usize,
		height: height as usize,
		depth,
		color,
		interlaced: interlace == 1,
	})
}
