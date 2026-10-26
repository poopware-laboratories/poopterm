//! Tests of what a window drawn by the GPU puts in its swapchain images,
//! against the fake driver of `driver_tests`: the backdrop under the quads,
//! and the glyphs of a frame it refused uploaded by the next one.
use super::{
	call::Error,
	driver_tests::with,
	renderer::Backdrop,
	renderer_tests::glyph,
	window::{Drawn, GpuWindow},
	window_tests::{ATLAS, SIZE, close, open, pen, showing, uploads},
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
