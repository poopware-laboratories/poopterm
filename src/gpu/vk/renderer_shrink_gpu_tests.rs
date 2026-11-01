//! An image shrunk to fit the texture budget, drawn by a real GPU at the
//! size it is shown: read back, it matches what software paints from the
//! whole bitmap, and its texture takes only the shrunk size.
use super::{
	buffer::Buffer,
	image::{Format, Image},
	loader_tests::{entry, offscreen},
	memory::Usage,
	pipeline::QuadPipeline,
	recording::Region,
	renderer::Renderer,
	renderer_gpu_tests::ATLAS,
	timeline::Timeline,
};
use crate::{
	gpu::{quads::QuadPen, textures::Textures},
	graphics::{Bitmap, Extent, Placement, Rect},
	render::{painter::Painter, pen::Pen},
};
use std::{num::NonZeroUsize, rc::Rc, sync::Arc};

const SIZE: (usize, usize) = (40, 24);
const BG: u32 = 0xff10_2030;
/// The colors of the image's quarters: left to right, top to bottom.
const QUARTERS: [u32; 4] = [0xffff_0000, 0xff00_00ff, 0xff00_ff00, 0xffff_ffff];

/// A 64×32 bitmap of four flat quarters, so each texel of it shrunk by
/// half is one quarter's color.
fn quarters() -> Arc<Bitmap> {
	let pixels = (0..32)
		.flat_map(|y| (0..64).map(move |x| QUARTERS[usize::from(y >= 16) * 2 + usize::from(x >= 32)]))
		.collect();
	Arc::new(Bitmap::new(64, 32, pixels))
}

/// Places `bitmap` whole, 32×16 large, at 4,4.
fn place(pen: &mut impl Pen, bitmap: &Arc<Bitmap>) {
	let mut placement = Placement::natural(bitmap.clone(), 0, 0);
	placement.extent = Extent::Pixels {
		width: 32,
		height: 16,
	};
	let clip = Rect {
		x: 0,
		y: 0,
		width: SIZE.0,
		height: SIZE.1,
	};
	pen.placement(clip, &placement, (4, 4), (8, 16));
}

#[test]
fn an_image_shrunk_to_fit_its_budget_is_drawn_as_software_paints_it() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	let pipeline = Rc::new(QuadPipeline::new(&device, Format::Bgra).unwrap());
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let mut renderer = Renderer::new(&pipeline, &timeline, ATLAS).unwrap();
	// Room for the image at the size it is shown, a quarter of its own.
	renderer.textures = Textures::new(32 * 16 * 4);
	let size = [SIZE.0 as u32, SIZE.1 as u32, 1];
	// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT` and `_TRANSFER_SRC_BIT`.
	let target = Rc::new(Image::new(&device, Format::Bgra, size, 0x10 | 0x1).unwrap());
	let bytes = NonZeroUsize::new(SIZE.0 * SIZE.1 * 4).unwrap();
	// `VK_BUFFER_USAGE_TRANSFER_DST_BIT`.
	let readback = Rc::new(Buffer::new(&device, bytes, 0x2, Usage::Readback).unwrap());
	let image = quarters();
	let mut pen = QuadPen::new(SIZE, ATLAS);
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, SIZE.1, BG);
	place(&mut pen, &image);
	let mut painted = vec![BG; SIZE.0 * SIZE.1];
	place(&mut Painter::new(&mut painted, SIZE.0, SIZE.1), &image);
	let mut recording = renderer.frame(&mut pen, &target).unwrap();
	let whole = Region {
		layer: 0,
		at: [0, 0],
		size: [size[0], size[1]],
	};
	recording.download(&target, whole, &readback, 0).unwrap();
	recording.submit().unwrap();
	renderer.finish().unwrap();
	let read: Vec<u32> = (readback.bytes().unwrap().as_chunks::<4>().0.iter())
		.map(|&chunk| u32::from_ne_bytes(chunk))
		.collect();
	// Software leaves alpha alone: only colors compare.
	let differ = (read.iter().zip(&painted))
		.filter(|&(a, b)| (a ^ b) & 0xff_ffff != 0)
		.count();
	assert_eq!(differ, 0);
	assert_eq!(renderer.textures.bytes(), 32 * 16 * 4);
	drop((renderer, target, readback, pen));
}
