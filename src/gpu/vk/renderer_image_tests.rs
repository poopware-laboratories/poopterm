//! Tests of the renderer's image textures against the fake driver: each
//! bitmap uploaded once and kept while it lives, runs of quads bound to
//! their image in order, and frames refused for software forgetting the
//! textures they made.
use super::{
	call::Error,
	driver_tests::{Driver, finish},
	renderer_tests::{ATLAS, frame, renderer_for},
};
use crate::{
	gpu::quads::QuadPen,
	graphics::{Bitmap, Placement, Rect},
	render::pen::Pen,
};
use std::sync::Arc;

/// A bitmap `width`×`height`, its pixels `0, 1, 2…`.
pub(super) fn bitmap(width: usize, height: usize) -> Arc<Bitmap> {
	Arc::new(Bitmap {
		width,
		height,
		opaque: true,
		pixels: (0..width * height)
			.map(|i| 0xff00_0000 | i as u32)
			.collect(),
	})
}

/// One texel wider than the fake device's 16384.
pub(super) fn too_wide() -> Arc<Bitmap> {
	bitmap(16385, 1)
}

/// Places the whole of `bitmap` at the top left, cells 8×16.
pub(super) fn place(pen: &mut QuadPen, bitmap: &Arc<Bitmap>) {
	let whole = Rect {
		x: 0,
		y: 0,
		width: 40,
		height: 30,
	};
	let placement = Placement::natural(bitmap.clone(), 0, 0);
	pen.placement(whole, &placement, (0, 0), (8, 16));
}

/// A renderer on a fake device, and a 40×30 target.
fn renderer() -> (super::renderer::Renderer, std::rc::Rc<super::image::Image>) {
	// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT` and `_TRANSFER_SRC_BIT`.
	renderer_for(0x10 | 0x1)
}

/// The uploads, image set bindings and draws recorded.
fn drawn(driver: &Driver) -> Vec<&str> {
	let recorded = driver.recorded.iter().map(String::as_str);
	let kept = ["upload", "bind image set", "draw"];
	recorded
		.filter(|call| kept.iter().any(|kind| call.starts_with(kind)))
		.collect()
}

#[test]
fn an_image_is_uploaded_once_and_drawn_from_its_texture_while_it_lives() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let image = bitmap(3, 2);
	for _ in 0..2 {
		pen.begin_frame((40, 30));
		place(&mut pen, &image);
		renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	}
	assert_eq!(renderer.textures.len(), 1);
	// Gone from the frame and dropped: its texture goes with the next frame.
	drop(image);
	pen.begin_frame((40, 30));
	pen.rect(0, 0, 1, 1, 0);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	assert_eq!(renderer.textures.len(), 0);
	drop((renderer, target, pen));
	let driver = finish();
	// Its texels from the start of a staging buffer grown to hold them, then its set bound for its quad;
	// the last frame binds the blank texture.
	let expected = [
		"upload 0xb00003+0 layer 0 at 0,0 size 3x2 of 0xa00004",
		"bind image set 0x660003",
		"draw 6 vertices of 1 instances from 0",
		"bind image set 0x660003",
		"draw 6 vertices of 1 instances from 0",
		"bind image set 0x660002",
		"draw 6 vertices of 1 instances from 0",
	];
	assert_eq!(drawn(&driver), expected);
}

#[test]
fn quads_draw_in_runs_bound_to_their_image_in_order() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let (first, second) = (bitmap(1, 1), bitmap(2, 1));
	// Glyphs, then the first image, a rectangle, the second, the first.
	frame(&mut pen);
	place(&mut pen, &first);
	pen.rect(0, 0, 1, 1, 0);
	place(&mut pen, &second);
	place(&mut pen, &first);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	assert_eq!(renderer.textures.len(), 2);
	drop((renderer, target, pen));
	let driver = finish();
	// The glyphs' texels, then each image's after them; the rectangle joins
	// the first image's run, and the first image is bound again for its
	// last quad.
	let expected = [
		"upload 0xb00004+0 layer 0 at 0,0 size 3x2 of 0xa00002",
		"upload 0xb00004+24 layer 0 at 4,0 size 2x2 of 0xa00002",
		"upload 0xb00004+40 layer 0 at 0,0 size 1x1 of 0xa00004",
		"upload 0xb00004+44 layer 0 at 0,0 size 2x1 of 0xa00005",
		"bind image set 0x660003",
		"draw 6 vertices of 5 instances from 0",
		"bind image set 0x660004",
		"draw 6 vertices of 1 instances from 5",
		"bind image set 0x660003",
		"draw 6 vertices of 1 instances from 6",
	];
	assert_eq!(drawn(&driver), expected);
}

#[test]
fn a_refused_frame_forgets_the_textures_it_made_and_keeps_the_others() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let (kept, fresh) = (bitmap(1, 1), bitmap(2, 2));
	pen.begin_frame((40, 30));
	place(&mut pen, &kept);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	// A new image before one too wide: the frame is software's.
	pen.begin_frame((40, 30));
	place(&mut pen, &kept);
	place(&mut pen, &fresh);
	place(&mut pen, &too_wide());
	assert_eq!(renderer.frame(&mut pen, &target).err(), Some(Error::Draw));
	assert_eq!(renderer.textures.len(), 1);
	// Drawn later, the new image is uploaded then, the kept one not again.
	pen.begin_frame((40, 30));
	place(&mut pen, &kept);
	place(&mut pen, &fresh);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	assert_eq!(renderer.textures.len(), 2);
	drop((renderer, target, pen));
	let driver = finish();
	let uploads = drawn(&driver)
		.into_iter()
		.filter(|call| call.starts_with("upload"));
	let expected = [
		"upload 0xb00002+0 layer 0 at 0,0 size 1x1 of 0xa00004",
		"upload 0xb00004+0 layer 0 at 0,0 size 2x2 of 0xa00006",
	];
	assert_eq!(uploads.collect::<Vec<_>>(), expected);
}

#[test]
fn a_restart_forgets_every_texture_so_the_next_frame_uploads_its_images() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let image = bitmap(1, 1);
	pen.begin_frame((40, 30));
	place(&mut pen, &image);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	renderer.restart(&mut pen);
	assert_eq!(renderer.textures.len(), 0);
	pen.begin_frame((40, 30));
	place(&mut pen, &image);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	drop((renderer, target, pen));
	let driver = finish();
	let uploads = drawn(&driver)
		.into_iter()
		.filter(|call| call.starts_with("upload"));
	let expected = [
		"upload 0xb00002+0 layer 0 at 0,0 size 1x1 of 0xa00004",
		"upload 0xb00002+0 layer 0 at 0,0 size 1x1 of 0xa00005",
	];
	assert_eq!(uploads.collect::<Vec<_>>(), expected);
}

#[test]
fn an_image_the_staging_buffer_fails_to_hold_is_not_kept() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	pen.begin_frame((40, 30));
	place(&mut pen, &bitmap(4, 4));
	// Growing the staging buffer fails: `VK_ERROR_OUT_OF_DEVICE_MEMORY`.
	super::driver_tests::with(|driver| driver.fails.push((c"vkCreateBuffer", -2)));
	assert_eq!(
		renderer.frame(&mut pen, &target).err(),
		Some(Error::Failed(-2))
	);
	assert_eq!(renderer.textures.len(), 0);
	drop((renderer, target, pen));
	finish();
}
