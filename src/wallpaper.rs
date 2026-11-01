//! Static terminal background image loading and resize caching.

use crate::graphics::Bitmap;
use std::{path::Path, sync::Arc};

use image::{RgbaImage, imageops::FilterType};

/// What a wallpaper bitmap is made for: its width and height, the
/// background and the strength.
type Made = (usize, usize, u32, usize);

pub(crate) struct Wallpaper {
	image: RgbaImage,
	/// The last bitmap made, with what it was made for.
	cache: Option<(Made, Arc<Bitmap>)>,
}

impl Wallpaper {
	pub(crate) fn load(path: &Path) -> Result<Self, String> {
		let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
		if metadata.len() > 128 * 1024 * 1024 {
			return Err("Wallpaper image must be smaller than 128 MiB.".into());
		}
		let mut reader = image::ImageReader::open(path)
			.map_err(|error| error.to_string())?
			.with_guessed_format()
			.map_err(|error| error.to_string())?;
		let mut limits = image::Limits::default();
		limits.max_image_width = Some(16_384);
		limits.max_image_height = Some(16_384);
		limits.max_alloc = Some(256 * 1024 * 1024);
		reader.limits(limits);
		let image = reader.decode().map_err(|error| error.to_string())?;
		let (width, height) = (image.width(), image.height());
		if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
			return Err("Wallpaper dimensions must be between 1 and 16,384 pixels.".into());
		}
		if (width as u64) * (height as u64) > 50_000_000 {
			return Err("Wallpaper image exceeds the 50-megapixel limit.".into());
		}
		Ok(Self::new(image.into_rgba8()))
	}

	pub(crate) fn new(image: RgbaImage) -> Self {
		Self { image, cache: None }
	}

	/// The wallpaper `width` by `height` (at least one pixel each), cropped
	/// around its center to that shape and blended over `background` at
	/// `strength` percent: opaque, and the same bitmap until one of these
	/// changes, so a GPU keeps its texture.
	pub(crate) fn bitmap(
		&mut self,
		width: usize,
		height: usize,
		background: u32,
		strength: usize,
	) -> &Arc<Bitmap> {
		let made = (width.max(1), height.max(1), background, strength.min(100));
		if self.cache.as_ref().is_none_or(|(old, _)| *old != made) {
			// Dropped first, so the old and new bitmaps are never both held.
			self.cache = None;
			let (width, height, strength) = (made.0 as u32, made.1 as u32, made.3);
			let (source_width, source_height) = self.image.dimensions();
			let (crop_width, crop_height) =
				if source_width as u64 * height as u64 > source_height as u64 * width as u64 {
					(
						(source_height as u64 * width as u64 / height as u64) as u32,
						source_height,
					)
				} else {
					(
						source_width,
						(source_width as u64 * height as u64 / width as u64) as u32,
					)
				};
			let crop_x = source_width.saturating_sub(crop_width) / 2;
			let crop_y = source_height.saturating_sub(crop_height) / 2;
			let cropped = image::imageops::crop_imm(
				&self.image,
				crop_x,
				crop_y,
				crop_width.max(1),
				crop_height.max(1),
			)
			.to_image();
			let scaled = image::imageops::resize(&cropped, width, height, FilterType::Triangle);
			let pixels = scaled
				.pixels()
				.map(|pixel| blend_over(pixel.0, background, strength));
			let bitmap = Bitmap::new(made.0, made.1, pixels.collect());
			self.cache = Some((made, Arc::new(bitmap)));
		}
		&self.cache.as_ref().expect("made above").1
	}
}

fn blend_over(pixel: [u8; 4], background: u32, strength: usize) -> u32 {
	let alpha = (pixel[3] as usize * strength + 50) / 100;
	let bg = [
		((background >> 16) & 0xff) as usize,
		((background >> 8) & 0xff) as usize,
		(background & 0xff) as usize,
	];
	let channel = |src: u8, dst: usize| (src as usize * alpha + dst * (255 - alpha) + 127) / 255;
	0xff00_0000
		| (channel(pixel[0], bg[0]) as u32) << 16
		| (channel(pixel[1], bg[1]) as u32) << 8
		| channel(pixel[2], bg[2]) as u32
}

#[cfg(test)]
#[path = "wallpaper_tests.rs"]
mod tests;
