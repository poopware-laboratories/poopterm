//! Our PNG decoder against the `image` crate's, on PNGs written here of
//! every color type, bit depth, filter and interlacing, random and by hand.
use super::{Image, crc::crc32, decode, filter};
use crate::bench::corpus::Rng;

const BIG: usize = 1 << 20;

/// What the `image` crate decodes `data` to, as `0xAARRGGBB` pixels.
pub(super) fn oracle(data: &[u8]) -> Option<Image> {
	let image = image::load_from_memory_with_format(data, image::ImageFormat::Png).ok()?;
	let rgba = image.into_rgba8();
	let pixels = rgba
		.pixels()
		.map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]]))
		.collect();
	Some(Image {
		width: rgba.width() as usize,
		height: rgba.height() as usize,
		pixels,
	})
}

/// A chunk of `kind` holding `body`, with its length and CRC.
pub(super) fn chunk(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
	let mut out = (body.len() as u32).to_be_bytes().to_vec();
	out.extend(kind);
	out.extend(body);
	out.extend(crc32(&[kind, body]).to_be_bytes());
	out
}

/// An image to write: its `IHDR` fields, palette, `tRNS`, and the raw
/// unfiltered rows of each pass, with the filter to write each row with.
#[derive(Clone, Debug)]
pub(super) struct Spec {
	pub width: u32,
	pub height: u32,
	pub depth: u8,
	pub color: u8,
	pub interlaced: bool,
	pub palette: Vec<[u8; 3]>,
	pub transparency: Option<Vec<u8>>,
	/// Per pass, its rows (each `row_bytes` long) and their filters.
	pub passes: Vec<Vec<(u8, Vec<u8>)>>,
}

fn header(spec: &Spec) -> super::chunks::Header {
	super::chunks::Header {
		width: spec.width as usize,
		height: spec.height as usize,
		depth: spec.depth,
		color: spec.color,
		interlaced: spec.interlaced,
	}
}

/// `raw` filtered with `kind` against the row `above`, as an encoder does.
fn filtered(kind: u8, raw: &[u8], above: &[u8], bpp: usize) -> Vec<u8> {
	let left = |i: usize| if i >= bpp { raw[i - bpp] } else { 0 };
	let up_left = |i: usize| if i >= bpp { above[i - bpp] } else { 0 };
	(0..raw.len())
		.map(|i| {
			let predicted = match kind {
				0 => 0,
				1 => left(i),
				2 => above[i],
				3 => ((u16::from(left(i)) + u16::from(above[i])) / 2) as u8,
				_ => {
					let (a, b, c) = (
						i16::from(left(i)),
						i16::from(above[i]),
						i16::from(up_left(i)),
					);
					let p = a + b - c;
					let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
					(if pa <= pb && pa <= pc {
						a
					} else if pb <= pc {
						b
					} else {
						c
					}) as u8
				}
			};
			raw[i].wrapping_sub(predicted)
		})
		.collect()
}

/// `spec` as a PNG file.
pub(super) fn write(spec: &Spec) -> Vec<u8> {
	let bpp = header(spec).bytes_per_pixel();
	let mut stream = Vec::new();
	for pass in &spec.passes {
		let mut above = vec![0; pass.first().map_or(0, |(_, row)| row.len())];
		for (kind, raw) in pass {
			stream.push(*kind);
			stream.extend(filtered(*kind, raw, &above, bpp));
			above.clone_from(raw);
		}
	}
	let mut ihdr = spec.width.to_be_bytes().to_vec();
	ihdr.extend(spec.height.to_be_bytes());
	ihdr.extend([spec.depth, spec.color, 0, 0, u8::from(spec.interlaced)]);
	let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
	out.extend(chunk(b"IHDR", &ihdr));
	if !spec.palette.is_empty() {
		out.extend(chunk(b"PLTE", &spec.palette.concat()));
	}
	if let Some(trns) = &spec.transparency {
		out.extend(chunk(b"tRNS", trns));
	}
	out.extend(chunk(
		b"IDAT",
		&miniz_oxide::deflate::compress_to_vec_zlib(&stream, 6),
	));
	out.extend(chunk(b"IEND", &[]));
	out
}

/// A random image of `color` and `depth`, `width` by `height`.
pub(super) fn random(
	rng: &mut Rng,
	(color, depth): (u8, u8),
	(width, height): (u32, u32),
	interlaced: bool,
) -> Spec {
	let mut spec = Spec {
		width,
		height,
		depth,
		color,
		interlaced,
		palette: Vec::new(),
		transparency: None,
		passes: Vec::new(),
	};
	let head = header(&spec);
	let entries = if color == 3 {
		1 + rng.below(1 << depth.min(8))
	} else {
		0
	};
	spec.palette = (0..entries)
		.map(|_| [0; 3].map(|_| rng.below(256) as u8))
		.collect();
	let byte = |rng: &mut Rng| rng.below(256) as u8;
	let passes: Vec<(usize, usize)> = super::passes(head).collect();
	for (w, h) in passes {
		let Some(row) = head.row_bytes(w) else {
			spec.passes.push(Vec::new());
			continue;
		};
		let rows = (0..h)
			.map(|_| {
				let raw: Vec<u8> = if color == 3 {
					// Indices within the palette, packed `depth` bits each.
					let mut raw = vec![0u8; row];
					for x in 0..w {
						let index = rng.below(entries) as u8;
						let bit = x * usize::from(depth);
						raw[bit / 8] |= index << (8 - usize::from(depth) - bit % 8);
					}
					raw
				} else {
					(0..row).map(|_| byte(rng)).collect()
				};
				(rng.below(5) as u8, raw)
			})
			.collect();
		spec.passes.push(rows);
	}
	spec.transparency = match (color, rng.below(3)) {
		(_, 0) => None,
		(3, _) => Some((0..1 + rng.below(entries)).map(|_| byte(rng)).collect()),
		(0, _) => Some(transparent_sample(&spec, 1)),
		(2, _) => Some(transparent_sample(&spec, 3)),
		_ => None,
	};
	spec
}

/// A `tRNS` naming the first pixel's samples, so some pixels match it.
fn transparent_sample(spec: &Spec, channels: usize) -> Vec<u8> {
	let row = spec
		.passes
		.iter()
		.find_map(|pass| pass.first())
		.map(|(_, raw)| raw.clone())
		.unwrap_or_default();
	let depth = usize::from(spec.depth);
	(0..channels)
		.flat_map(|n| {
			let value: u16 = match depth {
				16 => u16::from_be_bytes([row[2 * n], row[2 * n + 1]]),
				8 => u16::from(row[n]),
				_ => u16::from(row[0] >> (8 - depth) & ((1 << depth) - 1) as u8),
			};
			value.to_be_bytes()
		})
		.collect()
}

/// Every valid color type and bit depth.
pub(super) const FORMATS: [(u8, u8); 15] = [
	(0, 1),
	(0, 2),
	(0, 4),
	(0, 8),
	(0, 16),
	(2, 8),
	(2, 16),
	(3, 1),
	(3, 2),
	(3, 4),
	(3, 8),
	(4, 8),
	(4, 16),
	(6, 8),
	(6, 16),
];

#[test]
fn random_images_of_every_format_decode_as_the_image_crate_decodes_them() {
	let mut rng = Rng::new(2026);
	for round in 0..600 {
		let format = FORMATS[round % FORMATS.len()];
		let size = (1 + rng.below(37) as u32, 1 + rng.below(21) as u32);
		let interlaced = rng.below(2) == 0;
		let spec = random(&mut rng, format, size, interlaced);
		let png = write(&spec);
		let expected = oracle(&png).unwrap_or_else(|| panic!("the oracle refused {spec:?}"));
		assert_eq!(
			decode(&png, BIG, BIG),
			Ok(expected),
			"round {round}: {format:?} {size:?}"
		);
	}
}

#[test]
fn the_crc_is_the_standard_one() {
	// The check value of CRC-32/ISO-HDLC.
	assert_eq!(crc32(&[b"123456789"]), 0xcbf4_3926);
	assert_eq!(crc32(&[b"1234", b"56789"]), 0xcbf4_3926, "parts in order");
	assert_eq!(crc32(&[]), 0);
	assert_eq!(
		crc32(&[b"IEND"]),
		0xae42_6082,
		"the one every PNG ends with"
	);
}

#[test]
fn the_image_crates_own_files_decode_the_same() {
	use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
	let mut rng = Rng::new(7);
	for (color, bytes) in [
		(ExtendedColorType::L8, 1),
		(ExtendedColorType::La8, 2),
		(ExtendedColorType::Rgb8, 3),
		(ExtendedColorType::Rgba8, 4),
		(ExtendedColorType::L16, 2),
		(ExtendedColorType::Rgba16, 8),
	] {
		let (width, height) = (1 + rng.below(50) as u32, 1 + rng.below(50) as u32);
		let raw: Vec<u8> = (0..width * height * bytes)
			.map(|_| rng.below(256) as u8)
			.collect();
		let mut png = Vec::new();
		PngEncoder::new(&mut png)
			.write_image(&raw, width, height, color)
			.unwrap();
		assert_eq!(
			decode(&png, BIG, BIG),
			Ok(oracle(&png).unwrap()),
			"{color:?}"
		);
	}
}

#[test]
fn every_filter_undoes_what_an_encoder_did() {
	let mut rng = Rng::new(3);
	for bpp in [1, 2, 3, 4, 6, 8] {
		for kind in 0..5 {
			let row = bpp * (1 + rng.below(9));
			let raw: Vec<Vec<u8>> = (0..4)
				.map(|_| (0..row).map(|_| rng.below(256) as u8).collect())
				.collect();
			let mut data = Vec::new();
			let mut above = vec![0; row];
			for line in &raw {
				data.push(kind);
				data.extend(filtered(kind, line, &above, bpp));
				above.clone_from(line);
			}
			assert_eq!(
				filter::unfilter(&data, row, bpp),
				Ok(raw.concat()),
				"filter {kind}, {bpp} bytes a pixel"
			);
		}
	}
	assert_eq!(filter::unfilter(&[5, 0], 1, 1), Err("bad filter type"));
}

#[test]
fn the_crc_eight_bytes_a_step_matches_a_byte_at_a_time() {
	// The bitwise definition, written out, as the reference.
	let slow = |bytes: &[u8]| {
		let mut c = !0u32;
		for &b in bytes {
			c ^= u32::from(b);
			for _ in 0..8 {
				c = if c & 1 != 0 {
					0xedb8_8320 ^ (c >> 1)
				} else {
					c >> 1
				};
			}
		}
		!c
	};
	let mut rng = Rng::new(31);
	for len in (0..70).chain([255, 1000, 4097]) {
		let bytes: Vec<u8> = (0..len).map(|_| rng.below(256) as u8).collect();
		assert_eq!(crc32(&[&bytes]), slow(&bytes), "{len} bytes");
		let cut = rng.below(len + 1);
		assert_eq!(
			crc32(&[&bytes[..cut], &bytes[cut..]]),
			slow(&bytes),
			"{len} bytes cut at {cut}"
		);
	}
}
