//! Quads drawn by a real GPU: a solid background, a clipped solid, a
//! glyph's coverage and a color glyph, from an atlas uploaded first, read
//! back and compared with straight alpha blending computed here, within
//! one step a channel. Run by the loader's tests and by its validation
//! child.
use super::{
	buffer::Buffer,
	call::Error,
	commands::Commands,
	descriptors::QuadSet,
	device::Device,
	draw::Draw,
	image::{Format, Image},
	loader_tests::{entry, offscreen},
	memory::Usage,
	pipeline::QuadPipeline,
	recording::Region,
	timeline::Timeline,
};
use std::{num::NonZeroUsize, rc::Rc};

/// `VK_BUFFER_USAGE_TRANSFER_SRC_BIT` and `_DST_BIT`, which image usage
/// shares; `VK_IMAGE_USAGE_SAMPLED_BIT` and `_COLOR_ATTACHMENT_BIT`; and
/// `VK_BUFFER_USAGE_STORAGE_BUFFER_BIT`.
const TRANSFER_SRC: u32 = 1;
const TRANSFER_DST: u32 = 2;
const SAMPLED: u32 = 4;
const COLOR_ATTACHMENT: u32 = 0x10;
const STORAGE: u32 = 0x20;
/// The target, and the atlas page the glyphs are on, of two.
const WIDTH: u32 = 8;
const HEIGHT: u32 = 4;
const PAGE: u32 = 1;
/// Quad kinds.
const SOLID: u32 = 0;
const COVERAGE: u32 = 1;
const COLOR: u32 = 2;
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

/// A quad as the shaders read it: its rectangle, clip and source.
fn quad(rect: [i32; 4], clip: [i32; 4], source: [u32; 4]) -> [u32; 12] {
	let mut words = [0; 12];
	for (word, value) in words.iter_mut().zip(rect.into_iter().chain(clip)) {
		*word = value as u32;
	}
	words[8..].copy_from_slice(&source);
	words
}

/// What is drawn, in order.
fn scene() -> [[u32; 12]; 4] {
	let whole = [0, 0, WIDTH as i32, HEIGHT as i32];
	[
		quad(whole, whole, [0, 0, SOLID, BACKGROUND]),
		// Clipped on the left to columns 3 to 5.
		quad([2, 1, 4, 2], [3, 0, 8, 4], [0, 0, SOLID, 0xff80_40c0]),
		// White at the coverage of the atlas's first two columns.
		quad([0, 2, 2, 2], whole, [0, PAGE, COVERAGE, 0xffff_ffff]),
		// The third column's texels, clipped to its first column.
		quad([6, 0, 2, 2], [0, 0, 7, 4], [2, PAGE, COLOR, 0]),
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

/// The pixels the quads should leave.
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
	pixels
}

/// Whether every channel of `a` is within one of `b`'s.
fn close(a: u32, b: u32) -> bool {
	(0..4).all(|i| (a >> (i * 8) & 0xff).abs_diff(b >> (i * 8) & 0xff) <= 1)
}

/// A buffer for `usage`, holding `words`.
fn filled(device: &Rc<Device>, usage: u32, words: &[u32]) -> Result<Rc<Buffer>, Error> {
	let size = NonZeroUsize::new(words.len() * 4).ok_or(Error::NoMemory)?;
	let mut buffer = Buffer::new(device, size, usage, Usage::Upload)?;
	let bytes = buffer.bytes_mut().ok_or(Error::NoMemory)?;
	for (chunk, word) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(words) {
		*chunk = word.to_ne_bytes();
	}
	Ok(Rc::new(buffer))
}

/// Uploads the atlas, draws the quads, reads the target back, and says
/// whether it holds what it should.
pub(super) fn draw_known_quads(device: &Rc<Device>) -> Result<bool, Error> {
	let staging = filled(device, TRANSFER_SRC, &ATLAS)?;
	let quads = filled(device, STORAGE, scene().as_flattened())?;
	let bytes = NonZeroUsize::new((WIDTH * HEIGHT * 4) as usize).ok_or(Error::NoMemory)?;
	let readback = Rc::new(Buffer::new(device, bytes, TRANSFER_DST, Usage::Readback)?);
	let usage = SAMPLED | TRANSFER_DST;
	let atlas = Rc::new(Image::new(device, Format::Bgra, [4, 2, 2], usage)?);
	let usage = COLOR_ATTACHMENT | TRANSFER_SRC;
	let target = Rc::new(Image::new(device, Format::Bgra, [WIDTH, HEIGHT, 1], usage)?);
	let pipeline = Rc::new(QuadPipeline::new(device, Format::Bgra)?);
	let set = Rc::new(QuadSet::new(&pipeline)?);
	let timeline = Rc::new(Timeline::new(device)?);
	let mut commands = Commands::new(&timeline)?;
	let page = Region {
		layer: PAGE,
		at: [0, 0],
		size: [4, 2],
	};
	let whole = Region {
		layer: 0,
		at: [0, 0],
		size: [WIDTH, HEIGHT],
	};
	let mut recording = commands.record()?;
	recording.upload(&staging, 0, &atlas, page)?;
	let count = scene().len() as u32;
	recording.draw(Draw {
		set: &set,
		quads: &quads,
		count,
		atlas: &atlas,
		target: &target,
	})?;
	recording.download(&target, whole, &readback, 0)?;
	recording.submit()?;
	commands.finish()?;
	let read = readback.bytes().ok_or(Error::NoMemory)?;
	let read = read
		.as_chunks::<4>()
		.0
		.iter()
		.map(|&chunk| u32::from_ne_bytes(chunk));
	Ok(
		read
			.zip(expected())
			.all(|(read, expected)| close(read, expected)),
	)
}

#[test]
fn quads_drawn_by_the_gpu_come_back_within_a_step_a_channel() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	assert_eq!(draw_known_quads(&device), Ok(true));
}

#[test]
fn the_expected_pixels_blend_each_quad_over_the_ones_before() {
	let pixels = expected();
	let at = |x: usize, y: usize| pixels[y * 8 + x];
	// The background, and the clipped solid from column 3, not 2.
	assert_eq!([at(0, 0), at(2, 1), at(7, 3)], [BACKGROUND; 3]);
	assert_eq!(
		[at(3, 1), at(5, 2), at(6, 2)],
		[0xff80_40c0, 0xff80_40c0, BACKGROUND]
	);
	// Full, half, none and a quarter of white over the background.
	assert_eq!(
		[at(0, 2), at(1, 2), at(0, 3)],
		[0xffff_ffff, 0xff88_9098, BACKGROUND]
	);
	assert_eq!(at(1, 3), 0xff4c_5864);
	// The color glyph's opaque texel, its clear one, and the column clipped.
	assert_eq!(
		[at(6, 0), at(6, 1), at(7, 0)],
		[0xff00_ff00, BACKGROUND, BACKGROUND]
	);
}

#[test]
fn pixels_are_close_when_no_channel_is_off_by_more_than_one() {
	assert!(close(0xff10_2030, 0xfe11_1f31));
	for shift in [0, 8, 16, 24] {
		assert!(!close(0xff10_2030 ^ 2 << shift, 0xff10_2030), "{shift}");
	}
	assert_eq!(over(0xffff_ffff, 0, 0xff10_2030), 0xff10_2030);
	assert_eq!(over(0x0000_0000, 255, 0xffff_ffff), 0xff00_0000);
}
