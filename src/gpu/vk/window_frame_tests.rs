//! Tests of what a window drawn by the GPU puts in its swapchain images,
//! against the fake driver of `driver_tests`: the quads alone, in the
//! image of the swapchain the window has now, pictures as textures, and
//! the glyphs of a frame it refused uploaded by the next one.
use super::{
	call::Error,
	driver_tests::with,
	present_driver_tests::OUT_OF_DATE,
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

/// The widest image the fake device makes.
const LIMIT: usize = 16384;

/// Draws a frame with the glyph `a` in `window`, with a picture one pixel
/// tall and this wide, if any.
fn lettered(
	window: &mut GpuWindow,
	pen: &mut QuadPen,
	picture: Option<usize>,
) -> Result<Drawn, Error> {
	pen.begin_frame((40, 30));
	let ink = Ink::Drawn('a', (3, 2));
	pen.glyph(&glyph(3, 2, false), ink, (1, 1), (0, 0, 40, 30), false, 0);
	if let Some(width) = picture {
		let bitmap = Arc::new(Bitmap::new(width, 1, vec![0xff00_0000; width]));
		let whole = Rect {
			x: 0,
			y: 0,
			width: 40,
			height: 30,
		};
		let placement = Placement::natural(bitmap, 0, 0);
		pen.placement(whole, &placement, (0, 0), (8, 16));
	}
	window.draw(pen, SIZE)
}

#[test]
fn a_frame_with_a_picture_is_drawn_with_its_texture_and_shown() {
	let mut window = open(showing(), false).unwrap();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	for width in [2, LIMIT] {
		assert_eq!(
			lettered(&mut window, &mut pen, Some(width)),
			Ok(Drawn::Shown)
		);
		let recorded = with(|driver| std::mem::take(&mut driver.recorded));
		let size = format!("size {width}x1");
		assert!(
			recorded.iter().any(|call| call.contains(&size)),
			"{recorded:?}"
		);
		assert!(
			recorded
				.iter()
				.any(|call| call.starts_with("bind image set")),
			"{recorded:?}"
		);
	}
	assert_eq!(swapchains().len(), 1, "every frame shown in the first");
	close(window);
}

#[test]
fn the_glyphs_of_a_refused_frame_are_uploaded_by_the_next() {
	let mut window = open(showing(), false).unwrap();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	// A picture wider than the device makes is left to software, though a
	// swapchain image was taken for it; its glyph never reached the GPU,
	// so the next frame the GPU draws must upload it.
	assert_eq!(
		lettered(&mut window, &mut pen, Some(LIMIT + 1)),
		Err(Error::Draw)
	);
	assert_eq!(uploads(), 0);
	assert!(pen.quads.is_empty(), "the pen started over");
	assert_eq!(lettered(&mut window, &mut pen, None), Ok(Drawn::Shown));
	assert_eq!(uploads(), 1);
	// The image taken but not shown made the swapchain anew.
	assert_eq!(swapchains(), [(SIZE, false), (SIZE, true)]);
	assert_eq!(lettered(&mut window, &mut pen, None), Ok(Drawn::Shown));
	assert_eq!(uploads(), 0, "the atlas keeps it");
	close(window);
}

#[test]
fn a_window_without_area_takes_no_image() {
	let mut window = open(showing(), false).unwrap();
	// Opening writes the blank texture's set; the draws record nothing.
	let opened = with(|driver| std::mem::take(&mut driver.recorded));
	assert_eq!(opened.len(), 1);
	assert!(opened[0].starts_with("write"), "{opened:?}");
	for size in [[0, 30], [40, 0]] {
		let drawn = window.draw(&mut pen(SIZE), size);
		assert_eq!(drawn, Ok(Drawn::Skipped), "{size:?}");
	}
	assert_eq!(with(|driver| driver.recorded.clone()), [""; 0]);
	assert!(with(|driver| driver.acquire_timeouts.is_empty()));
	close(window);
}

/// Draws a frame of one rectangle, and says what it uploaded.
fn copied(window: &mut GpuWindow) -> (Result<Drawn, Error>, Vec<String>) {
	let drawn = window.draw(&mut pen(SIZE), SIZE);
	let calls = with(|driver| std::mem::take(&mut driver.recorded));
	(
		drawn,
		calls
			.into_iter()
			.filter(|call| call.starts_with("upload"))
			.collect(),
	)
}

#[test]
fn a_frame_of_quads_copies_nothing_into_its_image() {
	// The quads cover the whole frame: nothing software painted goes under.
	let mut window = open(showing(), false).unwrap();
	for _ in 0..2 {
		assert_eq!(copied(&mut window), (Ok(Drawn::Shown), vec![]));
	}
	close(window);
}

#[test]
fn after_a_remake_frames_are_shown_on_the_new_swapchain() {
	let mut window = open(showing(), false).unwrap();
	assert_eq!(copied(&mut window).0, Ok(Drawn::Shown));
	with(|driver| driver.present_answers = vec![OUT_OF_DATE]);
	assert_eq!(copied(&mut window), (Ok(Drawn::Again), vec![]));
	assert_eq!(copied(&mut window), (Ok(Drawn::Shown), vec![]));
	assert_eq!(swapchains().len(), 2);
	close(window);
}
