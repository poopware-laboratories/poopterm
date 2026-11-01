//! Tests of a window growing its glyph atlas, against the fake driver of
//! `driver_tests`: a frame lacking room in it makes the atlas larger and is
//! drawn again, unless the device cannot make one larger.
use super::{
	call::Error,
	driver_tests::with,
	renderer_tests::glyph,
	window::{Drawn, GpuWindow},
	window_tests::{ATLAS, SIZE, close, open, showing},
};
use crate::{
	gpu::quads::QuadPen,
	render::pen::{Ink, Pen},
};

/// Draws a frame of glyphs `w` × `h` pixels in `window`, one for each of
/// `letters`.
fn glyphs(
	window: &mut GpuWindow,
	pen: &mut QuadPen,
	(w, h): (usize, usize),
	letters: &str,
) -> Result<Drawn, Error> {
	pen.begin_frame((40, 30));
	for ch in letters.chars() {
		let ink = Ink::Drawn(ch, (w, h));
		pen.glyph(&glyph(w, h, false), ink, (0, 0), (0, 0, 40, 30), false, 0);
	}
	window.draw(pen, SIZE)
}

/// The size and layers of the atlas the window made last: the last image
/// larger than the pixel bound where no image is drawn.
fn made_atlas() -> ([u32; 3], u32) {
	with(|driver| {
		let image = driver
			.images
			.iter()
			.rfind(|image| image.extent != [1, 1, 1])
			.expect("an atlas");
		(image.extent, image.layers)
	})
}

/// The uploads recorded since the last call.
fn uploads() -> Vec<String> {
	let calls = with(|driver| std::mem::take(&mut driver.recorded));
	calls
		.into_iter()
		.filter(|call| call.starts_with("upload"))
		.collect()
}

#[test]
fn a_glyph_wider_than_a_page_grows_the_atlas_and_its_frame_is_drawn_again() {
	let mut window = open(showing(), false).unwrap();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	assert_eq!(made_atlas(), ([64, 64, 1], 2));
	let big = (100, 10);
	assert_eq!(glyphs(&mut window, &mut pen, big, "a"), Ok(Drawn::Again));
	// Pages of 128 pixels, as many as before, and a pen laid out for them.
	assert_eq!(made_atlas(), ([128, 128, 1], 2));
	assert!(pen.complete && pen.quads.is_empty());
	assert_eq!(uploads(), [""; 0], "nothing of the refused frame");
	assert_eq!(glyphs(&mut window, &mut pen, big, "a"), Ok(Drawn::Shown));
	let uploaded = uploads();
	assert_eq!(uploaded.len(), 1);
	assert!(uploaded[0].contains("size 100x10"), "{uploaded:?}");
	close(window);
}

#[test]
fn a_glyph_taller_than_a_page_grows_the_atlas_too() {
	let mut window = open(showing(), false).unwrap();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let tall = (10, 300);
	assert_eq!(glyphs(&mut window, &mut pen, tall, "a"), Ok(Drawn::Again));
	assert_eq!(made_atlas(), ([512, 512, 1], 2));
	assert_eq!(glyphs(&mut window, &mut pen, tall, "a"), Ok(Drawn::Shown));
	close(window);
}

#[test]
fn pages_run_out_within_a_frame_double_and_its_frame_is_drawn_again() {
	let mut window = open(showing(), false).unwrap();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	// A page of 64 pixels holds one glyph of 40: two pages hold two.
	let (size, letters) = ((40, 40), "abc");
	assert_eq!(
		glyphs(&mut window, &mut pen, size, letters),
		Ok(Drawn::Again)
	);
	assert_eq!(made_atlas(), ([64, 64, 1], 4));
	uploads();
	assert_eq!(
		glyphs(&mut window, &mut pen, size, letters),
		Ok(Drawn::Shown)
	);
	let uploaded = uploads();
	for layer in 0..3 {
		let upload = format!("layer {layer} at 0,0 size 40x40");
		assert!(
			uploaded.iter().any(|call| call.contains(&upload)),
			"{uploaded:?}"
		);
	}
	close(window);
}

#[test]
fn an_atlas_the_device_cannot_grow_refuses_the_frame() {
	let mut driver = showing();
	driver.image_limits = Some((64, 2048));
	let mut window = open(driver, false).unwrap();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let images = with(|driver| driver.images.len());
	let drawn = glyphs(&mut window, &mut pen, (100, 10), "a");
	assert_eq!(drawn, Err(Error::Draw));
	assert_eq!(with(|driver| driver.images.len()), images, "no atlas made");
	// The pen started over, and still fits the atlas the window has.
	assert!(pen.complete && pen.quads.is_empty());
	assert_eq!(glyphs(&mut window, &mut pen, (3, 2), "a"), Ok(Drawn::Shown));
	close(window);
}
