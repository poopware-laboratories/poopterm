//! Tests of the renderer's image textures at their edges, against the fake
//! driver: the device's largest image drawn as it is and larger ones shrunk,
//! a driver failing to make a texture failing the frame without keeping
//! it, and images uploaded before the quads that show them.
use super::{
	buffer_tests::{INTEL, device},
	call::Error,
	driver_tests::{Driver, finish, with},
	image::{Format, Image},
	pipeline::QuadPipeline,
	renderer::Renderer,
	renderer_image_tests::{bitmap, place},
	renderer_tests::ATLAS,
	timeline::Timeline,
};
use crate::gpu::quads::QuadPen;
use std::rc::Rc;

/// A renderer on a fake device `change` sets up, and a 40×30 target it
/// can draw on and copy into.
fn renderer_on(change: impl FnOnce(&mut Driver)) -> (Renderer, Rc<Image>) {
	let device = device(&INTEL, change);
	let pipeline = Rc::new(QuadPipeline::new(&device, Format::Bgra).unwrap());
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT` and `_TRANSFER_DST_BIT`.
	let target = Image::new(&device, Format::Bgra, [40, 30, 1], 0x10 | 0x2).unwrap();
	(
		Renderer::new(&pipeline, &timeline, ATLAS).unwrap(),
		Rc::new(target),
	)
}

#[test]
fn the_largest_image_the_device_makes_is_drawn_as_it_is_and_larger_ones_shrunk() {
	// As large as the atlas's 64-pixel pages, and no larger.
	let (mut renderer, target) = renderer_on(|driver| driver.image_limits = Some((64, 2048)));
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let sizes = [
		((64, 1), "64x1"),
		((65, 1), "64x1"),
		((1, 65), "1x64"),
		((130, 2), "64x1"),
		((1, 64), "1x64"),
	];
	for (size, _) in sizes {
		let image = bitmap(size.0, size.1);
		pen.begin_frame((40, 30));
		place(&mut pen, &image);
		renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
		assert!(renderer.textures.get(&image).is_some(), "{size:?}");
	}
	drop((renderer, target, pen));
	let driver = finish();
	let uploads: Vec<_> = (driver.recorded.iter())
		.filter(|call| call.starts_with("upload"))
		.collect();
	assert_eq!(uploads.len(), sizes.len(), "{uploads:?}");
	for (upload, (size, texture)) in uploads.iter().zip(sizes) {
		assert!(
			upload.contains(&format!("size {texture} of")),
			"{size:?}: {upload}"
		);
	}
}

#[test]
fn a_driver_failing_to_make_a_texture_fails_the_frame_and_keeps_the_rest() {
	let (mut renderer, target) = renderer_on(|_| {});
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let (kept, new) = (bitmap(1, 1), bitmap(2, 2));
	pen.begin_frame((40, 30));
	place(&mut pen, &kept);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	let live = || with(|driver| (driver.live_images.len(), driver.live_views.len()));
	let before = live();
	// The image, then its set's pool: each failure is the driver's own.
	for (call, code) in [(c"vkCreateImage", -2), (c"vkCreateDescriptorPool", -1)] {
		with(|driver| driver.fails.push((call, code)));
		pen.begin_frame((40, 30));
		place(&mut pen, &kept);
		place(&mut pen, &new);
		let failed = renderer.frame(&mut pen, &target).err();
		assert_eq!(failed, Some(Error::Failed(code)), "{call:?}");
		with(|driver| driver.fails.clear());
		assert_eq!(renderer.textures.len(), 1, "{call:?}");
		assert!(renderer.textures.get(&kept).is_some(), "{call:?}");
		// Nothing made for the new image outlives the frame.
		assert_eq!(live(), before, "{call:?}");
	}
	drop((renderer, target, pen));
	finish();
}

#[test]
fn images_are_uploaded_before_the_quads_that_show_them() {
	let (mut renderer, target) = renderer_on(|_| {});
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let image = bitmap(2, 1);
	pen.begin_frame((40, 30));
	place(&mut pen, &image);
	let recording = renderer.frame(&mut pen, &target).unwrap();
	recording.submit().unwrap();
	drop((renderer, target, pen));
	let driver = finish();
	let kept = ["upload", "bind image set", "draw"];
	let recorded = (driver.recorded.iter())
		.filter(|call| kept.iter().any(|kind| call.starts_with(kind)))
		.map(String::as_str)
		.collect::<Vec<_>>();
	// The image's texels to its texture, then its quad, with nothing
	// copied into the target.
	let expected = [
		"upload 0xb00003+0 layer 0 at 0,0 size 2x1 of 0xa00004",
		"bind image set 0x660003",
		"draw 6 vertices of 1 instances from 0",
	];
	assert_eq!(recorded, expected);
}
