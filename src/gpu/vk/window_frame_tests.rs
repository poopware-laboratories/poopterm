//! Tests of what a window drawn by the GPU puts in its swapchain images,
//! against the fake driver of `driver_tests`: the backdrop under the quads,
//! in the image of the swapchain the window has now, and the glyphs of a
//! frame it refused uploaded by the next one.
use super::{
	call::Error,
	driver_tests::with,
	present_driver_tests::OUT_OF_DATE,
	renderer::Backdrop,
	renderer_tests::glyph,
	window::{Drawn, GpuWindow},
	window_tests::{ATLAS, SIZE, close, open, pen, showing, swapchains, uploads},
};
use crate::{
	gpu::quads::QuadPen,
	graphics::{Bitmap, Placement, Rect},
	render::pen::{Ink, Pen},
};
use std::sync::Arc;

/// Draws a frame with the glyph `a` in `window`, with a picture when
/// `pictured`.
fn lettered(window: &mut GpuWindow, pen: &mut QuadPen, pictured: bool) -> Result<Drawn, Error> {
	pen.begin_frame((40, 30));
	let ink = Ink::Drawn('a', (3, 2));
	pen.glyph(&glyph(3, 2, false), ink, (1, 1), (0, 0, 40, 30), false, 0);
	if pictured {
		let bitmap = Arc::new(Bitmap {
			width: 2,
			height: 2,
			opaque: true,
			pixels: vec![0; 4],
		});
		let whole = Rect {
			x: 0,
			y: 0,
			width: 2,
			height: 2,
		};
		let placement = Placement::natural(bitmap, 0, 0);
		pen.placement(whole, &placement, (0, 0), (8, 16));
	}
	window.draw(pen, SIZE, None)
}

#[test]
fn the_glyphs_of_a_refused_frame_are_uploaded_by_the_next() {
	let mut window = open(showing(), false).unwrap();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	// Software draws the frame with a picture; its glyph never reached the
	// GPU, so the next frame the GPU draws must upload it.
	assert_eq!(lettered(&mut window, &mut pen, true), Err(Error::Draw));
	assert_eq!(lettered(&mut window, &mut pen, false), Ok(Drawn::Shown));
	assert_eq!(uploads(), 1);
	assert_eq!(lettered(&mut window, &mut pen, false), Ok(Drawn::Shown));
	assert_eq!(uploads(), 0, "the atlas keeps it");
	close(window);
}

#[test]
fn a_backdrop_is_copied_into_the_swapchain_image_before_the_quads() {
	let mut window = open(showing(), false).unwrap();
	let backdrop = vec![0x30_5070; 40 * 30];
	let under = Backdrop {
		pixels: &backdrop,
		width: 40,
	};
	assert_eq!(
		window.draw(&mut pen(SIZE), SIZE, Some(under)),
		Ok(Drawn::Shown)
	);
	let recorded = with(|driver| driver.recorded.clone());
	let copy = recorded.iter().position(|call| call.starts_with("upload"));
	let draw = recorded.iter().position(|call| call.starts_with("draw"));
	assert!(copy.is_some() && copy < draw, "{recorded:?}");
	assert!(
		recorded[copy.unwrap()].contains("size 40x30"),
		"{recorded:?}"
	);
	close(window);
}

#[test]
fn a_window_without_area_copies_no_backdrop_and_takes_no_image() {
	let mut window = open(showing(), false).unwrap();
	let backdrop = vec![0x30_5070; 40 * 30];
	for size in [[0, 30], [40, 0]] {
		let under = Backdrop {
			pixels: &backdrop,
			width: 40,
		};
		let drawn = window.draw(&mut pen(SIZE), size, Some(under));
		assert_eq!(drawn, Ok(Drawn::Skipped), "{size:?}");
	}
	assert!(with(|driver| driver.recorded.is_empty()));
	assert!(with(|driver| driver.acquire_timeouts.is_empty()));
	close(window);
}

/// Draws a frame over a whole backdrop, and says where its copies went.
fn copied(window: &mut GpuWindow, backdrop: bool) -> (Result<Drawn, Error>, Vec<String>) {
	let pixels = vec![0x30_5070; 40 * 30];
	let under = backdrop.then_some(Backdrop {
		pixels: &pixels,
		width: 40,
	});
	let drawn = window.draw(&mut pen(SIZE), SIZE, under);
	let calls = with(|driver| std::mem::take(&mut driver.recorded));
	let copies = calls.into_iter().filter(|call| call.starts_with("upload"));
	(
		drawn,
		copies
			.map(|call| call[call.find(" of ").unwrap()..].to_owned())
			.collect(),
	)
}

#[test]
fn a_frame_without_a_backdrop_copies_none_into_its_image() {
	let mut window = open(showing(), false).unwrap();
	assert_eq!(copied(&mut window, true).1.len(), 1);
	assert_eq!(copied(&mut window, false), (Ok(Drawn::Shown), vec![]));
	close(window);
}

#[test]
fn after_a_remake_the_backdrop_goes_into_the_new_swapchains_image() {
	let mut window = open(showing(), false).unwrap();
	let (_, before) = copied(&mut window, true);
	with(|driver| driver.present_answers = vec![OUT_OF_DATE]);
	assert_eq!(copied(&mut window, true).0, Ok(Drawn::Again));
	let (drawn, after) = copied(&mut window, true);
	assert_eq!(drawn, Ok(Drawn::Shown));
	assert_eq!(swapchains().len(), 2);
	assert_ne!(after, before, "an image of the new swapchain");
	close(window);
}
