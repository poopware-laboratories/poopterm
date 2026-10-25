//! Turns transmitted image data (`f=24` RGB, `f=32` RGBA, `f=100` PNG,
//! optionally zlib-compressed with `o=z`), and image files, into a
//! [`Bitmap`].
use std::io::Cursor;

use super::{Command, Error, MAX_TRANSFER};
use crate::graphics::Bitmap;

/// Largest width or height accepted.
pub(crate) const MAX_SIDE: usize = 16384;
/// Largest image in pixels (256 MiB decoded).
pub(crate) const MAX_PIXELS: usize = 1 << 26;

pub(crate) fn decode(command: &Command, data: &[u8]) -> Result<Bitmap, Error> {
	let format = command.uint(b'f');
	let format = if format == 0 { 32 } else { format };
	let compressed = match command.letter(b'o', 0) {
		0 => false,
		b'z' => true,
		_ => return Err(Error::new("EINVAL", "unknown compression")),
	};
	match format {
		24 | 32 => {
			let (width, height) = (command.uint(b's') as usize, command.uint(b'v') as usize);
			if width == 0 || height == 0 {
				return Err(Error::new("EINVAL", "raw pixel data needs s and v"));
			}
			check_size(width, height)?;
			let depth = if format == 24 { 3 } else { 4 };
			let size = width * height * depth;
			let inflated;
			let data = if compressed {
				inflated = inflate(data, size, size)?;
				&inflated[..]
			} else {
				data
			};
			let data = data
				.get(..size)
				.ok_or_else(|| Error::new("ENODATA", "insufficient image data"))?;
			let pixels = if depth == 3 {
				from_rgb(data)
			} else {
				from_rgba(data)
			};
			Ok(Bitmap::new(width, height, pixels))
		}
		100 => {
			let inflated;
			let data = if compressed {
				// Most streams shrink their data by a few times at most.
				inflated = inflate(data, data.len().saturating_mul(4), MAX_TRANSFER)?;
				&inflated[..]
			} else {
				data
			};
			decode_png(data)
		}
		_ => Err(Error::new("EINVAL", "unknown image format")),
	}
}

fn check_size(width: usize, height: usize) -> Result<(), Error> {
	if width > MAX_SIDE || height > MAX_SIDE || width * height > MAX_PIXELS {
		return Err(Error::new("EFBIG", "image too large"));
	}
	Ok(())
}

/// Inflates zlib data of about `expected` bytes, refusing output larger than `limit`.
fn inflate(data: &[u8], expected: usize, limit: usize) -> Result<Vec<u8>, Error> {
	crate::inflate::zlib(data, expected, limit)
		.ok_or_else(|| Error::new("EINVAL", "bad or oversized zlib data"))
}

fn from_rgb(data: &[u8]) -> Vec<u32> {
	let (pixels, _) = data.as_chunks::<3>();
	pixels
		.iter()
		.map(|&[r, g, b]| u32::from_be_bytes([0xff, r, g, b]))
		.collect()
}

fn from_rgba(data: &[u8]) -> Vec<u32> {
	let (pixels, _) = data.as_chunks::<4>();
	pixels
		.iter()
		.map(|&pixel| {
			// Little-endian RGBA is 0xAABBGGRR: swap red and blue.
			let value = u32::from_le_bytes(pixel);
			value & 0xff00_ff00 | (value >> 16) & 0xff | (value & 0xff) << 16
		})
		.collect()
}

/// Decodes a PNG, JPEG or WebP file, as its first bytes say it is.
pub(crate) fn decode_file(data: &[u8]) -> Result<Bitmap, Error> {
	use image::ImageFormat::{Jpeg, Png, WebP};
	match image::guess_format(data) {
		Ok(format @ (Png | Jpeg | WebP)) => decode_as(data, format, "EBADFILE"),
		_ => Err(Error::new("EBADFILE", "unsupported image format")),
	}
}

/// A PNG image within the size limits, as color glyphs store them.
pub(crate) fn png(data: &[u8]) -> Option<Bitmap> {
	decode_png(data).ok()
}

fn decode_png(data: &[u8]) -> Result<Bitmap, Error> {
	decode_as(data, image::ImageFormat::Png, "EBADPNG")
}

/// Decodes `data` as `format`, within the size limits; failures report `code`.
fn decode_as(data: &[u8], format: image::ImageFormat, code: &'static str) -> Result<Bitmap, Error> {
	let bad = |error: image::ImageError| Error::new(code, error.to_string());
	let mut reader = image::ImageReader::with_format(Cursor::new(data), format);
	let mut limits = image::Limits::default();
	limits.max_image_width = Some(MAX_SIDE as u32);
	limits.max_image_height = Some(MAX_SIDE as u32);
	// Room for one 16-bit RGBA image of the largest size accepted.
	limits.max_alloc = Some(MAX_PIXELS as u64 * 8);
	reader.limits(limits);
	let image = reader.decode().map_err(bad)?.into_rgba8();
	let (width, height) = (image.width() as usize, image.height() as usize);
	check_size(width, height)?;
	Ok(Bitmap::new(width, height, from_rgba(image.as_raw())))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn command(control: &str) -> Command {
		Command::parse(control.as_bytes()).unwrap()
	}

	fn code(result: Result<Bitmap, Error>) -> &'static str {
		result.unwrap_err().code
	}

	#[test]
	fn raw_rgb_and_rgba_become_argb() {
		let rgb = decode(&command("f=24,s=2,v=1"), &[1, 2, 3, 4, 5, 6]).unwrap();
		assert_eq!(rgb.pixels, [0xff010203, 0xff040506]);
		assert!(rgb.opaque);
		let rgba = decode(&command("s=1,v=2"), &[1, 2, 3, 0x80, 4, 5, 6, 255]).unwrap();
		assert_eq!((rgba.width, rgba.height), (1, 2));
		assert_eq!(rgba.pixels, [0x80010203, 0xff040506]);
		assert!(!rgba.opaque);
	}

	#[test]
	fn raw_data_must_match_the_size() {
		assert_eq!(
			code(decode(&command("f=24,s=2,v=1"), &[1, 2, 3])),
			"ENODATA"
		);
		assert_eq!(code(decode(&command("f=32,v=1"), &[1, 2, 3, 4])), "EINVAL");
		assert_eq!(code(decode(&command("f=7,s=1,v=1"), &[0; 4])), "EINVAL");
		assert_eq!(code(decode(&command("f=32,s=100000,v=1"), &[])), "EFBIG");
		assert_eq!(code(decode(&command("f=32,s=10000,v=10000"), &[])), "EFBIG");
	}

	#[test]
	fn zlib_compressed_rgb_is_inflated_within_the_expected_size() {
		let rgb: Vec<u8> = (0..4 * 3 * 3).map(|i| i as u8).collect();
		let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
		let bitmap = decode(&command("f=24,o=z,s=4,v=3"), &compressed).unwrap();
		assert_eq!(bitmap.pixels[1], 0xff030405);
		// Inflating to more than the image needs is refused, not buffered.
		let bomb = miniz_oxide::deflate::compress_to_vec_zlib(&[0; 1 << 20], 6);
		assert_eq!(code(decode(&command("f=24,o=z,s=4,v=3"), &bomb)), "EINVAL");
		assert_eq!(
			code(decode(&command("f=24,o=z,s=4,v=3"), b"junk")),
			"EINVAL"
		);
	}

	#[test]
	fn zlib_compressed_pixels_must_inflate_to_the_image_size() {
		let rgba: Vec<u8> = (0..2 * 2 * 4).map(|i| i as u8 * 7).collect();
		let zlib = |data: &[u8]| miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
		let bitmap = decode(&command("f=32,o=z,s=2,v=2"), &zlib(&rgba)).unwrap();
		assert_eq!(bitmap, decode(&command("f=32,s=2,v=2"), &rgba).unwrap());
		// Too little data once inflated, as for raw pixels.
		assert_eq!(
			code(decode(&command("f=32,o=z,s=2,v=2"), &zlib(&rgba[..15]))),
			"ENODATA"
		);
		// One byte too many is refused before it is kept.
		let mut more = rgba.clone();
		more.push(0);
		assert_eq!(
			code(decode(&command("f=32,o=z,s=2,v=2"), &zlib(&more))),
			"EINVAL"
		);
		// A damaged checksum fails the image.
		let mut damaged = zlib(&rgba);
		*damaged.last_mut().unwrap() ^= 1;
		assert_eq!(
			code(decode(&command("f=32,o=z,s=2,v=2"), &damaged)),
			"EINVAL"
		);
		assert_eq!(code(decode(&command("f=32,o=x,s=2,v=2"), &rgba)), "EINVAL");
	}

	#[test]
	fn compressed_png_may_inflate_past_four_times_its_size() {
		use image::ImageEncoder;
		// An uncompressed flat image shrinks far more than four times.
		use image::codecs::png::{CompressionType, FilterType, PngEncoder};
		let mut png = Vec::new();
		PngEncoder::new_with_quality(
			&mut png,
			CompressionType::Uncompressed,
			FilterType::NoFilter,
		)
		.write_image(&[0; 64 * 64 * 4], 64, 64, image::ExtendedColorType::Rgba8)
		.unwrap();
		let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&png, 9);
		assert!(png.len() > 4 * compressed.len());
		let bitmap = decode(&command("f=100,o=z"), &compressed).unwrap();
		assert_eq!((bitmap.width, bitmap.height), (64, 64));
		assert!(bitmap.pixels.iter().all(|&pixel| pixel == 0));
	}

	#[test]
	fn png_is_decoded_with_its_own_size() {
		use image::ImageEncoder;
		let mut png = Vec::new();
		image::codecs::png::PngEncoder::new(&mut png)
			.write_image(
				&[9, 8, 7, 255, 1, 2, 3, 0],
				2,
				1,
				image::ExtendedColorType::Rgba8,
			)
			.unwrap();
		let bitmap = decode(&command("f=100"), &png).unwrap();
		assert_eq!(bitmap.pixels, [0xff090807, 0x00010203]);
		let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&png, 6);
		assert_eq!(decode(&command("f=100,o=z"), &compressed).unwrap(), bitmap);
		assert_eq!(code(decode(&command("f=100"), b"not a png")), "EBADPNG");
	}

	#[test]
	fn files_are_decoded_by_their_format() {
		use image::ImageEncoder;
		let pixels = [200u8, 100, 50, 255].repeat(6);
		let mut png = Vec::new();
		image::codecs::png::PngEncoder::new(&mut png)
			.write_image(&pixels, 3, 2, image::ExtendedColorType::Rgba8)
			.unwrap();
		let mut jpeg = Vec::new();
		image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
			.write_image(
				&[200, 100, 50].repeat(6),
				3,
				2,
				image::ExtendedColorType::Rgb8,
			)
			.unwrap();
		let mut webp = Vec::new();
		image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
			.write_image(&pixels, 3, 2, image::ExtendedColorType::Rgba8)
			.unwrap();
		for file in [png, jpeg, webp] {
			let bitmap = decode_file(&file).unwrap();
			assert_eq!((bitmap.width, bitmap.height), (3, 2));
			// JPEG is lossy: close is enough.
			let red = (bitmap.pixels[0] >> 16) & 0xff;
			assert!(red.abs_diff(200) < 8, "{red}");
		}
		assert_eq!(code(decode_file(b"GIF89a\x01\x00")), "EBADFILE");
		assert_eq!(code(decode_file(b"")), "EBADFILE");
	}
}
