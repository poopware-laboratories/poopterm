//! Tests of the wallpaper as the bitmap drawn inside the border: cropped
//! around its center to the shape asked for, blended over the background
//! at the strength asked for, opaque, and the same bitmap, so the same GPU
//! texture, until any of these change.
use super::Wallpaper;
use image::RgbaImage;
use std::sync::Arc;

const RED: [u8; 4] = [0xff, 0, 0, 0xff];
const CLEAR_GREEN: [u8; 4] = [0, 0xff, 0, 0];
const NAVY: u32 = 0x00_0080;

/// A wallpaper of these pixels, `width` wide.
fn wallpaper(width: u32, pixels: &[[u8; 4]]) -> Wallpaper {
	let height = pixels.len() as u32 / width;
	let raw = pixels.concat();
	Wallpaper::new(RgbaImage::from_raw(width, height, raw).expect("pixels fill the image"))
}

/// Four pixels told apart by their red channel.
fn four() -> [[u8; 4]; 4] {
	[10, 20, 30, 40].map(|red| [red, 0, 0, 0xff])
}

#[test]
fn a_wallpaper_at_full_strength_shows_its_opaque_pixels_and_the_background_through_the_rest() {
	let mut paper = wallpaper(2, &[RED, CLEAR_GREEN]);
	let bitmap = paper.bitmap(2, 1, NAVY, 100);
	assert_eq!((bitmap.width, bitmap.height), (2, 1));
	assert_eq!(bitmap.pixels, [0xff_ff0000, 0xff_000080]);
	assert!(bitmap.opaque, "drawn by copying rows, as before");
}

#[test]
fn a_weaker_wallpaper_is_blended_over_the_background_rounding_to_nearest() {
	// Alpha 255 at 50% is 128: red 255·128/255 = 128, blue 128·127/255 ≈ 63.75 → 64.
	let mut paper = wallpaper(2, &[RED, CLEAR_GREEN]);
	assert_eq!(
		paper.bitmap(2, 1, NAVY, 50).pixels,
		[0xff_800040, 0xff_000080]
	);
	assert_eq!(paper.bitmap(2, 1, NAVY, 0).pixels, [0xff_000080; 2]);
}

#[test]
fn a_strength_past_full_is_full_strength_and_the_same_bitmap() {
	let mut paper = wallpaper(2, &[RED, CLEAR_GREEN]);
	let full = paper.bitmap(2, 1, NAVY, 100).clone();
	assert!(Arc::ptr_eq(&full, paper.bitmap(2, 1, NAVY, 101)));
	assert!(Arc::ptr_eq(&full, paper.bitmap(2, 1, NAVY, usize::MAX)));
}

#[test]
fn a_wider_wallpaper_shows_its_middle_columns_and_a_taller_one_its_middle_rows() {
	let reds = |paper: &mut Wallpaper, size: (usize, usize)| -> Vec<u32> {
		let bitmap = paper.bitmap(size.0, size.1, 0, 100);
		bitmap
			.pixels
			.iter()
			.map(|pixel| pixel >> 16 & 0xff)
			.collect()
	};
	assert_eq!(reds(&mut wallpaper(4, &four()), (2, 1)), [20, 30]);
	assert_eq!(reds(&mut wallpaper(1, &four()), (1, 2)), [20, 30]);
	// The same shape keeps every pixel.
	assert_eq!(reds(&mut wallpaper(2, &four()), (2, 2)), [10, 20, 30, 40]);
}

#[test]
fn the_bitmap_is_kept_until_its_size_background_or_strength_changes() {
	let mut paper = wallpaper(2, &[RED, CLEAR_GREEN]);
	let first = paper.bitmap(2, 1, NAVY, 60).clone();
	assert!(Arc::ptr_eq(&first, paper.bitmap(2, 1, NAVY, 60)));
	let changes = [
		(3, 1, NAVY, 60),
		(3, 2, NAVY, 60),
		(3, 2, 0, 60),
		(3, 2, 0, 70),
	];
	let mut last = first;
	for (width, height, background, strength) in changes {
		let next = paper.bitmap(width, height, background, strength).clone();
		assert!(
			!Arc::ptr_eq(&last, &next),
			"{width}x{height} {background:x} {strength}"
		);
		assert_eq!((next.width, next.height), (width, height));
		last = next;
	}
}

#[test]
fn a_replaced_bitmap_is_let_go_so_its_texture_can_be_too() {
	let mut paper = wallpaper(2, &[RED, CLEAR_GREEN]);
	let old = Arc::downgrade(paper.bitmap(2, 1, NAVY, 100));
	paper.bitmap(4, 2, NAVY, 100);
	assert!(old.upgrade().is_none());
}

#[test]
fn an_empty_inside_still_gets_a_pixel() {
	let mut paper = wallpaper(2, &[RED, CLEAR_GREEN]);
	let bitmap = paper.bitmap(0, 0, NAVY, 100);
	assert_eq!(
		(bitmap.width, bitmap.height, bitmap.pixels.len()),
		(1, 1, 1)
	);
	assert!(Arc::ptr_eq(&bitmap.clone(), paper.bitmap(0, 1, NAVY, 100)));
}
