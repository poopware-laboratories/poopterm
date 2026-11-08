//! Tests of drawing on the real device: draws that would read past their
//! quads or draw into what they sample are refused before Metal sees them,
//! and known quads (solid, clipped, coverage, color, image, italic) leave
//! exactly the pixels they should.
use super::{Draw, Frame, drawable};
use crate::gpu::mtl::{
	buffer::Buffer,
	device::{Device, Error, tests::device},
	pipeline::Pipeline,
	texture::{Texture, Usage, tests::download},
};
use std::{num::NonZeroUsize, ops::Range};

/// The target, and the atlas page the glyphs are on, of two.
const WIDTH: u32 = 8;
const HEIGHT: u32 = 6;
const PAGE: u32 = 1;
const SOLID: u32 = 0;
const COVERAGE: u32 = 1;
const COLOR: u32 = 2;
const IMAGE: u32 = 3;
const BACKGROUND: u32 = 0xff10_2030;

/// The atlas page, 4×2 `0xAARRGGBB` texels: a coverage glyph's alphas in
/// the first two columns, a color glyph's texels in the third.
const ATLAS: [u32; 8] = [
	0xff00_0000,
	0x8000_0000,
	0xff00_ff00,
	0,
	0,
	0x4000_0000,
	0x00ab_cdef,
	0,
];
/// A 2×2 image.
const PICTURE: [u32; 4] = [0xffff_0000, 0xff00_ff00, 0xff00_00ff, 0xffff_ffff];

/// A quad as the shaders read it: its rectangle, clip and source.
fn quad(rect: [i32; 4], clip: [i32; 4], source: [u32; 4]) -> [u32; 12] {
	let mut words = [0; 12];
	for (word, value) in words.iter_mut().zip(rect.into_iter().chain(clip)) {
		*word = value as u32;
	}
	words[8..].copy_from_slice(&source);
	words
}

/// What is drawn, in order: everything but the image, then the image.
fn scene() -> Vec<[u32; 12]> {
	let whole = [0, 0, WIDTH as i32, HEIGHT as i32];
	vec![
		quad(whole, whole, [0, 0, SOLID, BACKGROUND]),
		// Clipped on the left to columns 3 to 5.
		quad([2, 1, 4, 2], [3, 0, 8, 4], [0, 0, SOLID, 0xff80_40c0]),
		// White at the coverage of the atlas's first two columns.
		quad([0, 2, 2, 2], whole, [0, PAGE, COVERAGE, 0xffff_ffff]),
		// The third column's texels, clipped to its first column.
		quad([6, 0, 2, 2], [0, 0, 7, 4], [2, PAGE, COLOR, 0]),
		// The image, from (0, 0) and 2×2, stretched over 4×2 at (4, 4).
		quad([4, 4, 4, 2], whole, [0, 2 | 2 << 16, IMAGE, 0]),
	]
}

/// `source` at `alpha` over `below`, both `0xAARRGGBB`; the target stays
/// opaque.
fn over(source: u32, alpha: u32, below: u32) -> u32 {
	let channel = |shift: u32| {
		let (top, bottom) = (
			(source >> shift & 0xff) as f32,
			(below >> shift & 0xff) as f32,
		);
		let alpha = alpha as f32 / 255.0;
		((top * alpha + bottom * (1.0 - alpha)).round() as u32) << shift
	};
	0xff00_0000 | channel(16) | channel(8) | channel(0)
}

/// The pixels the scene should leave.
fn expected() -> Vec<u32> {
	let mut pixels = vec![BACKGROUND; (WIDTH * HEIGHT) as usize];
	let at = |x: u32, y: u32| (y * WIDTH + x) as usize;
	for y in 1..3 {
		for x in 3..6 {
			pixels[at(x, y)] = 0xff80_40c0;
		}
	}
	for (y, x) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
		let alpha = ATLAS[(y * 4 + x) as usize] >> 24;
		pixels[at(x, 2 + y)] = over(0xffff_ffff, alpha, pixels[at(x, 2 + y)]);
	}
	for y in 0..2 {
		let texel = ATLAS[(y * 4 + 2) as usize];
		pixels[at(6, y)] = over(texel, texel >> 24, pixels[at(6, y)]);
	}
	// Each image texel two pixels wide.
	for y in 0..2 {
		for x in 0..4 {
			pixels[at(4 + x, 4 + y)] = PICTURE[(y * 2 + x / 2) as usize];
		}
	}
	pixels
}

/// What draws the scene: the pipeline, its quads, the atlas, the image and
/// the target.
struct Kit {
	pipeline: Pipeline,
	quads: Buffer,
	atlas: Texture,
	image: Texture,
	target: Texture,
}

fn kit(device: &Device, quads: &[[u32; 12]]) -> Kit {
	let bytes = NonZeroUsize::new(quads.len().max(1) * 48).unwrap();
	let mut buffer = Buffer::new(device, bytes).unwrap();
	let words = quads.iter().flatten().flat_map(|word| word.to_ne_bytes());
	for (byte, word) in buffer.bytes_mut().iter_mut().zip(words) {
		*byte = word;
	}
	let atlas = Texture::new(device, [4, 2, 2], true, Usage::Sampled).unwrap();
	atlas.upload(PAGE, [0, 0], [4, 2], &ATLAS).unwrap();
	let image = Texture::new(device, [2, 2, 1], false, Usage::Sampled).unwrap();
	image.upload(0, [0, 0], [2, 2], &PICTURE).unwrap();
	Kit {
		pipeline: Pipeline::new(device).unwrap(),
		quads: buffer,
		atlas,
		image,
		target: Texture::new(device, [WIDTH, HEIGHT, 1], false, Usage::Target).unwrap(),
	}
}

/// Draws `runs` of `kit`'s quads over its target and reads it back.
fn drawn(device: &Device, kit: &Kit, runs: &[(Range<u32>, &Texture)]) -> Result<Vec<u32>, Error> {
	let mut frame = Frame::new(device)?;
	frame.draw(Draw {
		pipeline: &kit.pipeline,
		quads: &kit.quads,
		runs,
		atlas: &kit.atlas,
		target: &kit.target,
	})?;
	frame.synchronize(&kit.target)?;
	frame.commit().wait()?;
	Ok(download(&kit.target))
}

#[test]
fn known_quads_leave_the_pixels_they_should() {
	let Some(device) = device() else { return };
	let kit = kit(&device, &scene());
	// The image is bound for the last run only; the others need none.
	let runs = [(0..4, &kit.atlas), (4..5, &kit.image)];
	let read = drawn(&device, &kit, &runs).unwrap();
	let expected = expected();
	let off: Vec<_> = (0..read.len())
		.filter(|&i| {
			(0..3).any(|c| (read[i] >> (c * 8) & 0xff).abs_diff(expected[i] >> (c * 8) & 0xff) > 1)
		})
		.map(|i| (i % WIDTH as usize, i / WIDTH as usize, read[i], expected[i]))
		.collect();
	assert!(off.is_empty(), "{off:x?}");
}

#[test]
fn italic_coverage_leans_right_by_a_sixth_of_its_height() {
	let Some(device) = device() else { return };
	let whole = [0, 0, WIDTH as i32, HEIGHT as i32];
	// A glyph one column of full coverage six rows tall, italic: the top
	// row leans (6 - 0) / 6 = 1 pixel right, the others (6 - y) / 6 = 0.
	let quads = [
		quad(whole, whole, [0, 0, SOLID, 0xff00_0000]),
		quad([2, 0, 1, 6], whole, [0, 0, COVERAGE | 1 << 8, 0xffff_ffff]),
	];
	let mut kit = kit(&device, &quads);
	kit.atlas = Texture::new(&device, [1, 6, 1], true, Usage::Sampled).unwrap();
	kit
		.atlas
		.upload(0, [0, 0], [1, 6], &[0xff00_0000; 6])
		.unwrap();
	let read = drawn(&device, &kit, &[(0..2, &kit.image)]).unwrap();
	let lit: Vec<(u32, u32)> = (0..read.len() as u32)
		.filter(|&i| read[i as usize] & 0xff_ffff != 0)
		.map(|i| (i % WIDTH, i / WIDTH))
		.collect();
	assert_eq!(lit, [(3, 0), (2, 1), (2, 2), (2, 3), (2, 4), (2, 5)]);
}

#[test]
fn draws_reading_past_their_quads_or_into_what_they_sample_are_refused() {
	let Some(device) = device() else { return };
	let kit = kit(&device, &scene());
	let draw = |runs, atlas, target| Draw {
		pipeline: &kit.pipeline,
		quads: &kit.quads,
		runs,
		atlas,
		target,
	};
	let fits = [(0..5, &kit.image)];
	assert!(drawable(&draw(&fits, &kit.atlas, &kit.target)));
	assert!(drawable(&draw(&[], &kit.atlas, &kit.target)));
	// The buffer holds five quads, not six.
	let past = [(0..4, &kit.image), (4..6, &kit.image)];
	assert!(!drawable(&draw(&past, &kit.atlas, &kit.target)));
	// Drawing to a texture made only to be sampled, or that is sampled.
	assert!(!drawable(&draw(&fits, &kit.atlas, &kit.image)));
	let into_image = [(0..5, &kit.target)];
	assert!(!drawable(&draw(&into_image, &kit.atlas, &kit.target)));
	assert!(!drawable(&draw(&fits, &kit.target, &kit.target)));
	// A target of layers.
	let layered = Texture::new(&device, [WIDTH, HEIGHT, 2], true, Usage::Target).unwrap();
	assert!(!drawable(&draw(&fits, &kit.atlas, &layered)));
	let mut frame = Frame::new(&device).unwrap();
	assert_eq!(
		frame.draw(draw(&past, &kit.atlas, &kit.target)),
		Err(Error::Draw)
	);
}

#[test]
fn a_frame_with_nothing_drawn_is_done_once_waited_for() {
	let Some(device) = device() else { return };
	let submitted = Frame::new(&device).unwrap().commit();
	assert_eq!(submitted.wait(), Ok(()));
	assert!(submitted.done());
}
