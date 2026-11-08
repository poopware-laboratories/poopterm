//! The grid drawn by a real GPU: every shared grid scene of
//! `gpu::reference_tests`, drawn as quads by the Vulkan renderer and read
//! back, matches what the software renderer paints. Run by the loader's
//! tests and by its validation child.
use super::{
	buffer::Buffer,
	call::Error,
	device::Device,
	image::{Format, Image},
	loader_tests::{entry, offscreen},
	memory::Usage,
	pipeline::QuadPipeline,
	recording::Region,
	renderer::Renderer,
	timeline::Timeline,
};
use crate::render::grid_case_tests::SIZE;
use std::{num::NonZeroUsize, rc::Rc};

/// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT` and `_TRANSFER_SRC_BIT`, and
/// `VK_BUFFER_USAGE_TRANSFER_DST_BIT`.
const COLOR_ATTACHMENT: u32 = 0x10;
const TRANSFER_SRC: u32 = 0x1;
const TRANSFER_DST: u32 = 0x2;
/// The atlas the pen and the renderer share: four pages 256 pixels square.
pub(super) const ATLAS: (usize, usize) = (256, 4);

/// Draws every scene on `device` and says which differ from software, and
/// by how many pixels.
pub(super) fn unlike_software(device: &Rc<Device>) -> Result<Vec<String>, Error> {
	let (width, height) = SIZE;
	let pipeline = Rc::new(QuadPipeline::new(device, Format::Bgra)?);
	let timeline = Rc::new(Timeline::new(device)?);
	let mut renderer = Renderer::new(&pipeline, &timeline, ATLAS)?;
	let size = [width as u32, height as u32, 1];
	let target = Rc::new(Image::new(
		device,
		Format::Bgra,
		size,
		COLOR_ATTACHMENT | TRANSFER_SRC,
	)?);
	let bytes = NonZeroUsize::new(width * height * 4).ok_or(Error::NoMemory)?;
	let readback = Rc::new(Buffer::new(device, bytes, TRANSFER_DST, Usage::Readback)?);
	let whole = Region {
		layer: 0,
		at: [0, 0],
		size: [size[0], size[1]],
	};
	crate::gpu::reference_tests::unlike_software(ATLAS, |pen| {
		let mut recording = renderer.frame(pen, &target)?;
		recording.download(&target, whole, &readback, 0)?;
		recording.submit()?;
		renderer.finish()?;
		let read = readback.bytes().ok_or(Error::NoMemory)?;
		Ok(
			(read.as_chunks::<4>().0.iter())
				.map(|&chunk| u32::from_ne_bytes(chunk))
				.collect(),
		)
	})
}

#[test]
fn every_grid_scene_drawn_by_the_gpu_paints_what_software_paints() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	assert_eq!(unlike_software(&device), Ok(vec![]));
}
