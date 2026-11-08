//! Tests of the grid renderer on the real device: frames drawn and read
//! back, the quad buffer grown as frames need, glyphs and images uploaded
//! once and kept, the atlas grown when a frame lacks room, and frames the
//! GPU cannot draw refused, starting the pen over.
use super::Renderer;
use crate::{
	fonts::Metrics,
	gpu::{
		atlas::Wanted,
		mtl::{
			commands::tests::synchronize,
			device::{Device, Error, tests::device},
			texture::{Texture, Usage, tests::download},
		},
		quads::QuadPen,
	},
	graphics::{Bitmap, Placement, Rect},
	render::{
		glyphs::Glyph,
		pen::{Ink, Pen},
	},
};
use std::sync::Arc;

/// The atlas: four pages of 64 pixels square; and the target's size.
const ATLAS: (usize, usize) = (64, 4);
const SIZE: (usize, usize) = (40, 30);

fn target(device: &Device) -> Texture {
	let size = [SIZE.0 as u32, SIZE.1 as u32, 1];
	Texture::new(device, size, false, Usage::Target).unwrap()
}

/// Draws `pen`'s frame over `target` and reads it back.
fn drawn(renderer: &mut Renderer, pen: &mut QuadPen, target: &Texture) -> Result<Vec<u32>, Error> {
	renderer.frame(pen, target, |frame| synchronize(frame, target))?;
	renderer.finish()?;
	Ok(download(target))
}

/// A glyph `w`×`h` of full coverage.
fn glyph(w: usize, h: usize) -> Glyph {
	Glyph {
		metrics: Metrics {
			width: w,
			height: h,
			..Metrics::default()
		},
		color: None,
		coverage: vec![255; w * h],
	}
}

/// An opaque bitmap `width`×`height` of texels `0xff000000 | i`.
fn bitmap(width: usize, height: usize) -> Arc<Bitmap> {
	let pixels = (0..width * height)
		.map(|i| 0xff00_0000 | i as u32)
		.collect();
	Arc::new(Bitmap::new(width, height, pixels))
}

/// Places the whole of `bitmap` at the top left, cells 8×16.
fn place(pen: &mut QuadPen, bitmap: &Arc<Bitmap>) {
	let whole = Rect {
		x: 0,
		y: 0,
		width: SIZE.0,
		height: SIZE.1,
	};
	let placement = Placement::natural(bitmap.clone(), 0, 0);
	pen.placement(whole, &placement, (0, 0), (8, 16));
}

#[test]
fn a_frame_of_rects_and_a_glyph_reads_back_as_drawn() {
	let Some(device) = device() else { return };
	let mut renderer = Renderer::new(&device, ATLAS).unwrap();
	let target = target(&device);
	let mut pen = QuadPen::new(SIZE, ATLAS);
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, SIZE.1, 0x10_2030);
	pen.rect(4, 5, 3, 2, 0xc0_8040);
	let whole = (0, 0, SIZE.0 as i32, SIZE.1 as i32);
	pen.glyph(
		&glyph(2, 3),
		Ink::Drawn('a', (2, 3)),
		(20, 10),
		whole,
		false,
		0xff_ffff,
	);
	let read = drawn(&mut renderer, &mut pen, &target).unwrap();
	let at = |x: usize, y: usize| read[y * SIZE.0 + x] & 0xff_ffff;
	assert_eq!(at(0, 0), 0x10_2030);
	assert_eq!((at(4, 5), at(6, 6)), (0xc0_8040, 0xc0_8040));
	assert_eq!((at(7, 5), at(4, 7)), (0x10_2030, 0x10_2030));
	assert_eq!((at(20, 10), at(21, 12)), (0xff_ffff, 0xff_ffff));
	assert_eq!((at(22, 10), at(20, 13)), (0x10_2030, 0x10_2030));
	// The glyph went to the atlas; the next frame draws it from there.
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, SIZE.1, 0);
	pen.glyph(
		&glyph(2, 3),
		Ink::Drawn('a', (2, 3)),
		(0, 0),
		whole,
		false,
		0xff_ffff,
	);
	assert!(pen.uploads.is_empty());
	let read = drawn(&mut renderer, &mut pen, &target).unwrap();
	assert_eq!(read[0] & 0xff_ffff, 0xff_ffff);
}

#[test]
fn the_quad_buffer_grows_to_the_next_power_of_two_quads_a_frame_needs() {
	let Some(device) = device() else { return };
	let mut renderer = Renderer::new(&device, ATLAS).unwrap();
	assert_eq!(renderer.quads.size, 48);
	let target = target(&device);
	let mut pen = QuadPen::new(SIZE, ATLAS);
	pen.begin_frame(SIZE);
	for x in 0..SIZE.0 {
		pen.rect(x, 0, 1, SIZE.1, x as u32 * 0x01_0101);
	}
	let read = drawn(&mut renderer, &mut pen, &target).unwrap();
	assert_eq!(renderer.quads.size, 64 * 48);
	assert_eq!(read[39] & 0xff_ffff, 39 * 0x01_0101);
	// Fewer quads keep the larger buffer.
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, SIZE.1, 0x12_3456);
	let read = drawn(&mut renderer, &mut pen, &target).unwrap();
	assert_eq!(renderer.quads.size, 64 * 48);
	assert_eq!(read[39] & 0xff_ffff, 0x12_3456);
}

#[test]
fn an_image_is_uploaded_once_kept_while_shown_and_drawn_texel_for_texel() {
	let Some(device) = device() else { return };
	let mut renderer = Renderer::new(&device, ATLAS).unwrap();
	let target = target(&device);
	let mut pen = QuadPen::new(SIZE, ATLAS);
	let picture = bitmap(5, 4);
	for _ in 0..2 {
		pen.begin_frame(SIZE);
		pen.rect(0, 0, SIZE.0, SIZE.1, 0);
		place(&mut pen, &picture);
		let read = drawn(&mut renderer, &mut pen, &target).unwrap();
		assert_eq!(renderer.textures.len(), 1);
		for y in 0..4 {
			for x in 0..5 {
				assert_eq!(read[y * SIZE.0 + x], picture.pixels[y * 5 + x], "{x}, {y}");
			}
		}
		assert_eq!(read[5] & 0xff_ffff, 0);
	}
	// A frame without it lets it go.
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, SIZE.1, 0);
	drawn(&mut renderer, &mut pen, &target).unwrap();
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, SIZE.1, 0);
	drawn(&mut renderer, &mut pen, &target).unwrap();
	assert!(renderer.textures.len() <= 1);
}

#[test]
fn a_frame_lacking_glyphs_is_refused_and_the_pen_starts_over() {
	let Some(device) = device() else { return };
	let mut renderer = Renderer::new(&device, ATLAS).unwrap();
	let target = target(&device);
	let mut pen = QuadPen::new(SIZE, ATLAS);
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, SIZE.1, 0);
	// Larger than a page.
	let whole = (0, 0, SIZE.0 as i32, SIZE.1 as i32);
	pen.glyph(
		&glyph(100, 3),
		Ink::Drawn('W', (100, 3)),
		(0, 0),
		whole,
		false,
		0xff_ffff,
	);
	assert!(!pen.complete);
	let wanted = pen.wanted;
	assert_eq!(drawn(&mut renderer, &mut pen, &target), Err(Error::Draw));
	assert!(pen.complete && pen.quads.is_empty());
	// The atlas grows to hold it, and the frame then draws.
	assert_eq!(renderer.grow(wanted), Ok(true));
	assert_eq!(renderer.atlas.size[0], 128);
	renderer.restart(&mut pen);
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, SIZE.1, 0);
	pen.glyph(
		&glyph(100, 3),
		Ink::Drawn('W', (100, 3)),
		(0, 0),
		whole,
		false,
		0xff_ffff,
	);
	assert!(pen.complete);
	let read = drawn(&mut renderer, &mut pen, &target).unwrap();
	assert_eq!(read[39] & 0xff_ffff, 0xff_ffff);
}

#[test]
fn an_atlas_past_what_metal_holds_is_not_grown() {
	let Some(device) = device() else { return };
	let mut renderer = Renderer::new(&device, ATLAS).unwrap();
	let past = Wanted {
		side: 1 << 15,
		pages: false,
	};
	assert_eq!(renderer.grow(past), Ok(false));
	assert_eq!(renderer.atlas.size, [64, 64, 4]);
	let more = Wanted {
		side: 0,
		pages: true,
	};
	assert_eq!(renderer.grow(more), Ok(true));
	assert_eq!(renderer.atlas.size, [64, 64, 8]);
}

#[test]
fn restarting_forgets_every_texture_and_the_pen_s_glyphs() {
	let Some(device) = device() else { return };
	let mut renderer = Renderer::new(&device, ATLAS).unwrap();
	let target = target(&device);
	let mut pen = QuadPen::new(SIZE, ATLAS);
	pen.begin_frame(SIZE);
	place(&mut pen, &bitmap(3, 3));
	drawn(&mut renderer, &mut pen, &target).unwrap();
	assert_eq!(renderer.textures.len(), 1);
	renderer.restart(&mut pen);
	assert_eq!(renderer.textures.len(), 0);
	// The same glyph is uploaded again.
	pen.begin_frame(SIZE);
	let whole = (0, 0, SIZE.0 as i32, SIZE.1 as i32);
	pen.glyph(
		&glyph(2, 2),
		Ink::Drawn('a', (2, 2)),
		(0, 0),
		whole,
		false,
		0xff_ffff,
	);
	assert_eq!(pen.uploads.len(), 1);
	// Waiting with nothing in flight is done at once.
	assert_eq!(renderer.finish(), Ok(()));
	assert_eq!(renderer.finish(), Ok(()));
}
