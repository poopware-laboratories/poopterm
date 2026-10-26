//! Tests of the grid renderer against the fake driver of `driver_tests`:
//! quads encoded as the shaders read them, the glyphs a frame lacks
//! uploaded to their pages once, every quad drawn, buffers grown as frames
//! need, and frames the GPU cannot draw yet refused.
use super::{
	buffer_tests::{INTEL, device},
	call::Error,
	driver_tests::{Driver, finish, with},
	image::Format,
	pipeline::QuadPipeline,
	renderer::{Renderer, encode},
	timeline::Timeline,
};
use crate::{
	fonts::Metrics,
	gpu::{
		atlas::Slot,
		quads::{Fill, Quad, QuadPen},
	},
	graphics::{Bitmap, Placement, Rect},
	render::{
		glyphs::Glyph,
		pen::{Ink, Pen},
	},
};
use std::{rc::Rc, sync::Arc};

/// The atlas: four pages of 64 pixels square.
pub(super) const ATLAS: (usize, usize) = (64, 4);

fn slot(page: usize, x: usize, y: usize, w: usize, h: usize) -> Slot {
	Slot { page, x, y, w, h }
}

fn quad(fill: Fill, italic: bool) -> Quad {
	Quad {
		at: (-3, 7),
		size: (5, 9),
		clip: (0, 2, 40, 30),
		fill,
		italic,
		color: 0x00ab_cdef,
	}
}

#[test]
fn quads_encode_as_the_shaders_read_them() {
	// Rectangle, clip, then atlas x | y << 16, page, kind | italic << 8 and
	// the color.
	let place = [-3i32 as u32, 7, 5, 9, 0, 2, 40, 30];
	let glyph = slot(3, 0x12, 0x345, 5, 9);
	let solid = encode(&quad(Fill::Solid, false)).unwrap();
	assert_eq!(solid[..8], place);
	assert_eq!(solid[8..], [0, 0, 0, 0x00ab_cdef]);
	let coverage = encode(&quad(Fill::Coverage(glyph), true)).unwrap();
	assert_eq!(coverage[8..], [0x0345_0012, 3, 0x101, 0x00ab_cdef]);
	let color = encode(&quad(Fill::Color(glyph), false)).unwrap();
	assert_eq!(color[8..], [0x0345_0012, 3, 2, 0x00ab_cdef]);
	// Only coverage leans: an italic emoji keeps its shape.
	assert_eq!(encode(&quad(Fill::Color(glyph), true)), Some(color));
	assert_eq!(encode(&quad(Fill::Solid, true)), Some(solid));
	// Images are not drawn by the quad shaders.
	assert_eq!(encode(&quad(Fill::Image(0), false)), None);
}

/// A renderer on a fake device, with the pipeline and timeline it draws
/// with, and a 40×30 target.
fn renderer() -> (Renderer, Rc<super::image::Image>) {
	// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT` and `_TRANSFER_SRC_BIT`.
	renderer_for(0x10 | 0x1)
}

/// A renderer, and a 40×30 target of `usage` to draw on.
pub(super) fn renderer_for(usage: u32) -> (Renderer, Rc<super::image::Image>) {
	let device = device(&INTEL, |_| {});
	let pipeline = Rc::new(QuadPipeline::new(&device, Format::Bgra).unwrap());
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let target = super::image::Image::new(&device, Format::Bgra, [40, 30, 1], usage).unwrap();
	(
		Renderer::new(&pipeline, &timeline, ATLAS).unwrap(),
		Rc::new(target),
	)
}

/// A glyph `w`×`h`, of coverage `0, 1, 2…`, or in those colors.
pub(super) fn glyph(w: usize, h: usize, color: bool) -> Glyph {
	let coverage: Vec<u8> = (0..w * h).map(|i| i as u8).collect();
	Glyph {
		metrics: Metrics {
			width: w,
			height: h,
			..Metrics::default()
		},
		color: color.then(|| {
			coverage
				.iter()
				.map(|&c| 0x8000_0000 | u32::from(c))
				.collect()
		}),
		coverage,
	}
}

/// A frame of a background, a 3×2 coverage glyph and a 2×2 color one.
pub(super) fn frame(pen: &mut QuadPen) {
	pen.begin_frame((40, 30));
	pen.rect(0, 0, 40, 30, 0x10_1010);
	let whole = (0, 0, 40, 30);
	pen.glyph(
		&glyph(3, 2, false),
		Ink::Drawn('a', (3, 2)),
		(1, 1),
		whole,
		false,
		0xff_ffff,
	);
	pen.glyph(
		&glyph(2, 2, true),
		Ink::Drawn('b', (2, 2)),
		(5, 1),
		whole,
		false,
		0,
	);
}

/// The uploads and draws recorded.
pub(super) fn drawn(driver: &Driver) -> Vec<&str> {
	let recorded = driver.recorded.iter().map(String::as_str);
	recorded
		.filter(|call| call.starts_with("upload") || call.starts_with("draw"))
		.collect()
}

#[test]
fn a_frame_uploads_its_new_glyphs_to_their_pages_and_draws_every_quad() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	frame(&mut pen);
	let recording = renderer.frame(&mut pen, &target).unwrap();
	recording.submit().unwrap();
	// The quads as encoded, in order.
	let quads = renderer.quads.bytes().unwrap();
	let words: Vec<u32> = (quads.as_chunks::<4>().0.iter())
		.map(|&word| u32::from_ne_bytes(word))
		.collect();
	let encoded: Vec<u32> = pen
		.quads
		.iter()
		.flat_map(|quad| encode(quad).unwrap())
		.collect();
	assert_eq!(words[..encoded.len()], encoded);
	// The glyphs' texels one after the other, as their uploads list them.
	let staging = renderer.staging.bytes().unwrap();
	let texels: Vec<u32> = (staging.as_chunks::<4>().0.iter())
		.map(|&word| u32::from_ne_bytes(word))
		.collect();
	let uploaded: Vec<u32> = pen
		.uploads
		.iter()
		.flat_map(|upload| upload.pixels.clone())
		.collect();
	assert_eq!(texels[..10], uploaded);
	assert_eq!(texels[..3], [0x00ff_ffff, 0x01ff_ffff, 0x02ff_ffff]);
	// A second frame of the same glyphs uploads nothing.
	frame(&mut pen);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	drop((renderer, target, pen));
	let driver = finish();
	// From the fourth buffer, the staging one grown, to the atlas, made
	// after the target; side by side on the first page, a pixel apart.
	let expected = [
		"upload 0xb00004+0 layer 0 at 0,0 size 3x2 of 0xa00002",
		"upload 0xb00004+24 layer 0 at 4,0 size 2x2 of 0xa00002",
		"draw 6 vertices of 3 instances from 0",
		"draw 6 vertices of 3 instances from 0",
	];
	assert_eq!(drawn(&driver), expected);
}

#[test]
fn a_glyph_a_full_page_leaves_goes_to_the_next_page() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	pen.begin_frame((40, 30));
	let whole = (0, 0, 40, 30);
	pen.glyph(
		&glyph(64, 64, false),
		Ink::Drawn('e', (64, 64)),
		(0, 0),
		whole,
		false,
		0,
	);
	pen.glyph(
		&glyph(2, 3, true),
		Ink::Drawn('f', (2, 3)),
		(0, 0),
		whole,
		false,
		0,
	);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	drop((renderer, target, pen));
	let driver = finish();
	// The second glyph's texels follow the first's 64×64 in the staging.
	let expected = [
		"upload 0xb00004+0 layer 0 at 0,0 size 64x64 of 0xa00002",
		"upload 0xb00004+16384 layer 1 at 0,0 size 2x3 of 0xa00002",
		"draw 6 vertices of 2 instances from 0",
	];
	assert_eq!(drawn(&driver), expected);
}

#[test]
fn buffers_grow_to_the_next_power_of_two_quads_or_bytes_a_frame_needs() {
	let (mut renderer, target) = renderer();
	// Room for a quad and nothing staged until frames need more.
	assert_eq!((renderer.quads.size, renderer.staging.size), (48, 4));
	let mut pen = QuadPen::new((40, 30), ATLAS);
	pen.begin_frame((40, 30));
	for i in 0..5 {
		pen.rect(i, 0, 1, 1, 0);
	}
	pen.glyph(
		&glyph(5, 4, false),
		Ink::Drawn('c', (5, 4)),
		(0, 0),
		(0, 0, 40, 30),
		false,
		0,
	);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	// Six quads round up to eight; twenty texels to 128 bytes.
	assert_eq!((renderer.quads.size, renderer.staging.size), (8 * 48, 128));
	// Fewer the next frame: the buffers stay.
	pen.begin_frame((40, 30));
	pen.rect(0, 0, 1, 1, 0);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	assert_eq!((renderer.quads.size, renderer.staging.size), (8 * 48, 128));
	drop((renderer, target, pen));
	finish();
}

#[test]
fn an_empty_frame_draws_no_quads() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	pen.begin_frame((40, 30));
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	drop((renderer, target, pen));
	let driver = finish();
	assert_eq!(drawn(&driver), ["draw 6 vertices of 0 instances from 0"]);
}

#[test]
fn frames_with_images_or_missing_glyphs_are_refused_for_software_to_draw() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	pen.begin_frame((40, 30));
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
	pen.placement(whole, &Placement::natural(bitmap, 0, 0), (0, 0), (8, 16));
	assert_eq!(renderer.frame(&mut pen, &target).err(), Some(Error::Draw));
	// A glyph larger than a page leaves the frame incomplete.
	pen.begin_frame((40, 30));
	pen.glyph(
		&glyph(65, 1, false),
		Ink::Drawn('d', (65, 1)),
		(0, 0),
		(0, 0, 40, 30),
		false,
		0,
	);
	assert!(!pen.complete);
	assert_eq!(renderer.frame(&mut pen, &target).err(), Some(Error::Draw));
	drop((renderer, target, pen));
	let driver = finish();
	assert_eq!(drawn(&driver), [""; 0]);
	assert!(
		!driver
			.recorded
			.iter()
			.any(|call| call.starts_with("begin 0x"))
	);
}

#[test]
fn a_frame_that_fails_uploads_its_glyphs_again_on_the_next() {
	let (mut renderer, target) = renderer();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	frame(&mut pen);
	// Growing the staging buffer fails: `VK_ERROR_OUT_OF_DEVICE_MEMORY`.
	with(|driver| driver.fails.push((c"vkCreateBuffer", -2)));
	assert_eq!(
		renderer.frame(&mut pen, &target).err(),
		Some(Error::Failed(-2))
	);
	with(|driver| driver.fails.clear());
	// The buffers it had stay, and the pen forgot what it meant to upload.
	assert_eq!(renderer.staging.size, 4);
	assert!(pen.uploads.is_empty() && pen.quads.is_empty());
	frame(&mut pen);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	// Software draws a frame with an image; its new glyph is uploaded later.
	frame(&mut pen);
	let bitmap = Arc::new(Bitmap {
		width: 1,
		height: 1,
		opaque: true,
		pixels: vec![0],
	});
	let one = Rect {
		x: 0,
		y: 0,
		width: 1,
		height: 1,
	};
	pen.placement(one, &Placement::natural(bitmap, 0, 0), (0, 0), (8, 16));
	pen.glyph(
		&glyph(1, 1, false),
		Ink::Drawn('g', (1, 1)),
		(9, 9),
		(0, 0, 40, 30),
		false,
		0,
	);
	assert_eq!(renderer.frame(&mut pen, &target).err(), Some(Error::Draw));
	frame(&mut pen);
	pen.glyph(
		&glyph(1, 1, false),
		Ink::Drawn('g', (1, 1)),
		(9, 9),
		(0, 0, 40, 30),
		false,
		0,
	);
	renderer.frame(&mut pen, &target).unwrap().submit().unwrap();
	drop((renderer, target, pen));
	let driver = finish();
	// Each frame drawn uploads every glyph it draws, from the staging start.
	let expected = [
		"upload 0xb00004+0 layer 0 at 0,0 size 3x2 of 0xa00002",
		"upload 0xb00004+24 layer 0 at 4,0 size 2x2 of 0xa00002",
		"draw 6 vertices of 3 instances from 0",
		"upload 0xb00004+0 layer 0 at 0,0 size 3x2 of 0xa00002",
		"upload 0xb00004+24 layer 0 at 4,0 size 2x2 of 0xa00002",
		"upload 0xb00004+40 layer 0 at 7,0 size 1x1 of 0xa00002",
		"draw 6 vertices of 4 instances from 0",
	];
	assert_eq!(drawn(&driver), expected);
}
