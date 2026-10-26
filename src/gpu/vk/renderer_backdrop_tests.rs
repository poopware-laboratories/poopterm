//! Tests of drawing quads over a backdrop, pixels software painted, against
//! the fake driver of `driver_tests`: the backdrop copied into the target
//! before the quads are drawn, clipped to the target and packed to its
//! width, and nothing copied when there is nothing to copy.
use super::{
	call::Error,
	driver_tests::finish,
	image::Image,
	renderer::{Backdrop, Renderer},
	renderer_tests::{ATLAS, drawn, frame, renderer_for},
};
use crate::gpu::quads::QuadPen;
use std::rc::Rc;

/// A renderer, and a target it can draw on and copy into, as a swapchain
/// image is.
fn renderer() -> (Renderer, Rc<Image>) {
	// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT` and `_TRANSFER_DST_BIT`.
	renderer_for(0x10 | 0x2)
}

/// `width`×`height` pixels, each its own row and column.
fn pixels(width: usize, height: usize) -> Vec<u32> {
	(0..width * height)
		.map(|i| (((i / width) << 8) | (i % width)) as u32)
		.collect()
}

/// The backdrop as the renderer staged it: its words, in order.
fn staged(renderer: &Renderer, count: usize) -> Vec<u32> {
	let bytes = renderer.backdrop.as_ref().unwrap().bytes().unwrap();
	(bytes.as_chunks::<4>().0.iter().take(count))
		.map(|&chunk| u32::from_ne_bytes(chunk))
		.collect()
}

#[test]
fn a_backdrop_is_copied_into_the_target_before_the_quads_are_drawn() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	frame(&mut pen);
	let backdrop = pixels(40, 30);
	let under = Backdrop {
		pixels: &backdrop,
		width: 40,
	};
	let recording = renderer.frame_over(&mut pen, &target, Some(under)).unwrap();
	recording.submit().unwrap();
	assert_eq!(staged(&renderer, 1200), backdrop);
	drop((renderer, target, pen));
	let driver = finish();
	// The glyphs first, then the backdrop over the whole target, then the
	// quads over it.
	let expected = [
		"upload 0xb00004+0 layer 0 at 0,0 size 3x2 of 0xa00002",
		"upload 0xb00004+24 layer 0 at 4,0 size 2x2 of 0xa00002",
		"upload 0xb00005+0 layer 0 at 0,0 size 40x30 of 0xa00001",
		"draw 6 vertices of 3 instances from 0",
	];
	assert_eq!(drawn(&driver), expected);
}

#[test]
fn a_backdrop_larger_than_the_target_is_clipped_and_packed_to_its_width() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	frame(&mut pen);
	let backdrop = pixels(50, 35);
	let under = Backdrop {
		pixels: &backdrop,
		width: 50,
	};
	renderer
		.frame_over(&mut pen, &target, Some(under))
		.unwrap()
		.submit()
		.unwrap();
	let expected: Vec<u32> = (backdrop.chunks(50).take(30))
		.flat_map(|row| &row[..40])
		.copied()
		.collect();
	assert_eq!(staged(&renderer, 1200), expected);
	drop((renderer, target, pen));
	let driver = finish();
	assert!(drawn(&driver).contains(&"upload 0xb00005+0 layer 0 at 0,0 size 40x30 of 0xa00001"));
}

#[test]
fn a_backdrop_smaller_than_the_target_covers_its_own_corner() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	frame(&mut pen);
	// Twelve whole rows of 25, and a part row that does not count.
	let backdrop = pixels(25, 13)[..25 * 12 + 7].to_vec();
	let under = Backdrop {
		pixels: &backdrop,
		width: 25,
	};
	renderer
		.frame_over(&mut pen, &target, Some(under))
		.unwrap()
		.submit()
		.unwrap();
	assert_eq!(staged(&renderer, 300), backdrop[..300]);
	drop((renderer, target, pen));
	let driver = finish();
	assert!(drawn(&driver).contains(&"upload 0xb00005+0 layer 0 at 0,0 size 25x12 of 0xa00001"));
}

#[test]
fn an_empty_backdrop_copies_nothing() {
	let empty = [
		(vec![], 40),
		(pixels(40, 30), 0),
		(pixels(39, 1)[..39].to_vec(), 40),
	];
	for (backdrop, width) in empty {
		let (mut renderer, target) = renderer();
		let mut pen = QuadPen::new((40, 30), ATLAS);
		frame(&mut pen);
		let under = Backdrop {
			pixels: &backdrop,
			width,
		};
		renderer
			.frame_over(&mut pen, &target, Some(under))
			.unwrap()
			.submit()
			.unwrap();
		drop((renderer, target, pen));
		let driver = finish();
		let copied = drawn(&driver)
			.iter()
			.any(|call| call.contains("of 0xa00001"));
		assert!(!copied, "{width}: {:?}", drawn(&driver));
	}
}

#[test]
fn a_frame_the_gpu_cannot_draw_stages_no_backdrop() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	frame(&mut pen);
	pen.complete = false;
	let backdrop = pixels(40, 30);
	let under = Backdrop {
		pixels: &backdrop,
		width: 40,
	};
	let recorded = renderer.frame_over(&mut pen, &target, Some(under));
	assert_eq!(recorded.err(), Some(Error::Draw));
	assert!(renderer.backdrop.is_none(), "never made");
	drop((renderer, target, pen));
	let driver = finish();
	assert!(drawn(&driver).is_empty());
}

#[test]
fn the_backdrop_buffer_grows_for_a_larger_backdrop_and_is_kept_for_smaller() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let mut over = |renderer: &mut Renderer, (width, height): (usize, usize)| {
		frame(&mut pen);
		let backdrop = pixels(width, height);
		let under = Backdrop {
			pixels: &backdrop,
			width,
		};
		let recording = renderer.frame_over(&mut pen, &target, Some(under));
		recording.unwrap().submit().unwrap();
		assert_eq!(staged(renderer, width * height), backdrop);
		renderer.backdrop.as_ref().unwrap().size
	};
	// Made for the first, a corner of 100 pixels.
	assert_eq!(over(&mut renderer, (10, 10)), 400);
	// Grown to the next power of two pixels for the whole target.
	assert_eq!(over(&mut renderer, (40, 30)), 2048 * 4);
	// Kept for a smaller one, and again for the whole.
	assert_eq!(over(&mut renderer, (20, 5)), 2048 * 4);
	assert_eq!(over(&mut renderer, (40, 30)), 2048 * 4);
	drop((renderer, target, pen));
	finish();
}
