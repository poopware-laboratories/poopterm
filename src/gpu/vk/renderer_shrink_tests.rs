//! Tests of images shrunk to fit, against the fake driver: a bitmap past
//! the budget uploaded at the size it is shown, one texture for a bitmap
//! shown at two sizes, a shrunk texture made again larger when shown
//! larger, and kept when its limits still hold.
use super::{
	driver_tests::finish,
	image::Image,
	renderer::Renderer,
	renderer_image_tests::bitmap,
	renderer_tests::{ATLAS, renderer_for},
};
use crate::{
	gpu::{quads::QuadPen, textures::Textures},
	graphics::{Bitmap, Extent, Placement, Rect},
	render::pen::Pen,
};
use std::{rc::Rc, sync::Arc};

/// A renderer whose textures may take `budget` bytes, and its target.
fn renderer(budget: usize) -> (Renderer, Rc<Image>) {
	// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT` and `_TRANSFER_SRC_BIT`.
	let (mut renderer, target) = renderer_for(0x10 | 0x1);
	renderer.textures = Textures::new(budget);
	(renderer, target)
}

/// Places the whole of `bitmap`, `width`×`height` large.
fn place(pen: &mut QuadPen, bitmap: &Arc<Bitmap>, (width, height): (usize, usize)) {
	let mut placement = Placement::natural(bitmap.clone(), 0, 0);
	placement.extent = Extent::Pixels { width, height };
	let clip = Rect {
		x: 0,
		y: 0,
		width: 40,
		height: 30,
	};
	pen.placement(clip, &placement, (0, 0), (8, 16));
}

/// Draws a frame of `bitmap` at each of `sizes`.
fn draw(
	renderer: &mut Renderer,
	target: &Rc<Image>,
	bitmap: &Arc<Bitmap>,
	sizes: &[(usize, usize)],
) {
	let mut pen = QuadPen::new((40, 30), ATLAS);
	pen.begin_frame((40, 30));
	for &size in sizes {
		place(&mut pen, bitmap, size);
	}
	renderer.frame(&mut pen, target).unwrap().submit().unwrap();
}

/// The sizes of the textures uploaded.
fn uploaded() -> Vec<String> {
	let driver = finish();
	let uploads = (driver.recorded.iter()).filter(|call| call.starts_with("upload"));
	let size = |call: &String| {
		call
			.split(" size ")
			.nth(1)
			.unwrap_or("")
			.split(' ')
			.next()
			.unwrap_or("")
			.to_owned()
	};
	uploads.map(size).collect()
}

#[test]
fn a_bitmap_past_the_budget_is_uploaded_at_the_size_it_is_shown() {
	let (mut renderer, target) = renderer(10 * 5 * 4);
	let photo = bitmap(100, 50);
	draw(&mut renderer, &target, &photo, &[(10, 5)]);
	assert_eq!(renderer.textures.bytes(), 10 * 5 * 4);
	drop((renderer, target));
	assert_eq!(uploaded(), ["10x5"]);
}

#[test]
fn a_bitmap_shown_at_two_sizes_takes_one_texture_at_the_larger() {
	let (mut renderer, target) = renderer(20 * 10 * 4);
	let photo = bitmap(100, 50);
	draw(&mut renderer, &target, &photo, &[(10, 5), (20, 10), (4, 2)]);
	assert_eq!(renderer.textures.len(), 1);
	drop((renderer, target));
	assert_eq!(uploaded(), ["20x10"]);
}

#[test]
fn a_shrunk_texture_shown_larger_is_made_again_and_kept_after() {
	let (mut renderer, target) = renderer(20 * 10 * 4);
	let photo = bitmap(100, 50);
	draw(&mut renderer, &target, &photo, &[(10, 5)]);
	draw(&mut renderer, &target, &photo, &[(20, 10)]);
	// Only the larger texture is kept; shown smaller again, it is enough.
	assert_eq!(renderer.textures.bytes(), 20 * 10 * 4);
	draw(&mut renderer, &target, &photo, &[(10, 5)]);
	drop((renderer, target));
	assert_eq!(uploaded(), ["10x5", "20x10"]);
}

#[test]
fn a_texture_the_budget_shrank_is_not_made_again_each_frame() {
	// Shown at 20×10, but the budget holds half that each way.
	let (mut renderer, target) = renderer(10 * 5 * 4);
	let photo = bitmap(100, 50);
	for _ in 0..3 {
		draw(&mut renderer, &target, &photo, &[(20, 10)]);
	}
	drop((renderer, target));
	assert_eq!(uploaded(), ["10x5"]);
}
