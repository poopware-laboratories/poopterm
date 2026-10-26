//! The grid drawn by a real GPU over a backdrop: every shared grid scene,
//! drawn as quads over a gradient software painted and read back, matches
//! what the software renderer paints over the same gradient. The backdrop
//! is wider and taller than the target, so each row is clipped and packed.
//! Run by the loader's tests and by its validation child.
use super::{
	buffer::Buffer,
	call::Error,
	device::Device,
	image::{Format, Image},
	loader_tests::{entry, offscreen},
	memory::Usage,
	pipeline::QuadPipeline,
	recording::Region,
	renderer::{Backdrop, Renderer},
	renderer_gpu_tests::{ATLAS, differing, layers},
	timeline::Timeline,
};
use crate::{
	gpu::quads::QuadPen,
	render::{
		grid::draw_grid,
		grid_case_tests::{SIZE, cases, fonts},
		painter::Painter,
	},
};
use std::{num::NonZeroUsize, rc::Rc};

/// How much wider and taller than the target the backdrop is.
const MORE: usize = 7;

/// A gradient `width`×`height` pixels large, red steps across and green
/// steps down, each wider than the one step pixels may differ by, so a row
/// or column out of place shows.
fn gradient(width: usize, height: usize) -> Vec<u32> {
	(0..width * height)
		.map(|i| {
			let (x, y) = (i % width, i / width);
			(((x * 7) & 0xff) << 16 | ((y * 13) & 0xff) << 8 | 0x40) as u32
		})
		.collect()
}

/// Draws every scene without images on `device` over a gradient, and says
/// which differ from software painting over it, and by how many pixels.
pub(super) fn unlike_software_over_backdrop(device: &Rc<Device>) -> Result<Vec<String>, Error> {
	let (width, height) = SIZE;
	let pipeline = Rc::new(QuadPipeline::new(device, Format::Bgra)?);
	let timeline = Rc::new(Timeline::new(device)?);
	let mut renderer = Renderer::new(&pipeline, &timeline, ATLAS)?;
	let size = [width as u32, height as u32, 1];
	// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT`, `_TRANSFER_SRC_BIT` and
	// `_TRANSFER_DST_BIT`; `VK_BUFFER_USAGE_TRANSFER_DST_BIT`.
	let target = Rc::new(Image::new(device, Format::Bgra, size, 0x10 | 0x1 | 0x2)?);
	let bytes = NonZeroUsize::new(width * height * 4).ok_or(Error::NoMemory)?;
	let readback = Rc::new(Buffer::new(device, bytes, 0x2, Usage::Readback)?);
	let whole = Region {
		layer: 0,
		at: [0, 0],
		size: [size[0], size[1]],
	};
	let wide = width + MORE;
	let backdrop = gradient(wide, height + MORE);
	let (mut pen, mut fonts) = (QuadPen::new(SIZE, ATLAS), fonts());
	let mut unlike = Vec::new();
	for case in cases().iter().filter(|case| case.frame.images.is_empty()) {
		let view = case.view();
		pen.begin_frame(SIZE);
		draw_grid(&mut pen, &mut fonts, &view, None);
		let under = Backdrop {
			pixels: &backdrop,
			width: wide,
		};
		let mut recording = renderer.frame_over(&mut pen, &target, Some(under))?;
		recording.download(&target, whole, &readback, 0)?;
		recording.submit()?;
		renderer.finish()?;
		let read = readback.bytes().ok_or(Error::NoMemory)?;
		let read: Vec<u32> = (read.as_chunks::<4>().0.iter())
			.map(|&chunk| u32::from_ne_bytes(chunk))
			.collect();
		let mut painted: Vec<u32> = (backdrop.chunks_exact(wide).take(height))
			.flat_map(|row| &row[..width])
			.copied()
			.collect();
		let mut painter = Painter::new(&mut painted, width, height);
		draw_grid(&mut painter, &mut fonts, &view, None);
		let differ = differing(&read, &painted, &layers(&pen));
		if differ > 0 {
			unlike.push(format!("{}: {differ} pixels", case.name));
		}
	}
	Ok(unlike)
}

#[test]
fn every_grid_scene_drawn_by_the_gpu_over_a_backdrop_paints_what_software_paints() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	assert_eq!(unlike_software_over_backdrop(&device), Ok(vec![]));
}
