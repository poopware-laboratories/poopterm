//! The grid drawn by a real GPU: every shared grid scene, drawn as quads
//! by the renderer and read back, matches what the software renderer
//! paints within one step a channel, and the scene with images is left to
//! software. Run by the loader's tests and by its validation child.
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
use crate::{
	gpu::{
		atlas::Slot,
		quads::{Fill, Quad, QuadPen},
	},
	render::{
		grid::draw_grid,
		grid_case_tests::{BG, SIZE, cases, direct, fonts},
		pen::Pen,
	},
};
use std::{num::NonZeroUsize, rc::Rc};

/// `VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT` and `_TRANSFER_SRC_BIT`, and
/// `VK_BUFFER_USAGE_TRANSFER_DST_BIT`.
const COLOR_ATTACHMENT: u32 = 0x10;
const TRANSFER_SRC: u32 = 0x1;
const TRANSFER_DST: u32 = 0x2;
/// The atlas the pen and the renderer share: four pages 256 pixels square.
const ATLAS: (usize, usize) = (256, 4);

/// How many translucent quads blend into each pixel since the last opaque
/// one covered it: software truncates each blend and the GPU rounds it, so
/// each may leave them a step apart.
fn layers(pen: &QuadPen) -> Vec<u32> {
	let (width, height) = pen.size;
	let mut layers = vec![0; width * height];
	for quad in &pen.quads {
		let (x, y) = quad.at;
		let (w, h) = (quad.size.0 as i32, quad.size.1 as i32);
		let (left, top, right, bottom) = quad.clip;
		// Italic coverage leans right by up to a sixth of its height.
		let lean = if quad.italic && matches!(quad.fill, Fill::Coverage(_)) {
			h / 6
		} else {
			0
		};
		let columns = x.max(left).max(0)..(x + w + lean).min(right).min(width as i32);
		for row in y.max(top).max(0)..(y + h).min(bottom).min(height as i32) {
			for column in columns.clone() {
				let layer = &mut layers[row as usize * width + column as usize];
				*layer = if quad.fill == Fill::Solid {
					0
				} else {
					*layer + 1
				};
			}
		}
	}
	layers
}

/// How many of `drawn`'s pixels differ from `painted`'s in a color channel
/// by more than the `layers` blended into them, and by more than one; the
/// GPU's alpha is not the software's.
pub(super) fn differing(drawn: &[u32], painted: &[u32], layers: &[u32]) -> usize {
	let close = |a: u32, b: u32, most: u32| {
		(0..3).all(|i| (a >> (i * 8) & 0xff).abs_diff(b >> (i * 8) & 0xff) <= most.max(1))
	};
	let pixels = drawn.iter().zip(painted).zip(layers);
	pixels
		.filter(|&((&a, &b), &most)| !close(a, b, most))
		.count()
}

/// Draws every scene on `device` and says which differ from software, and
/// by how many pixels; the scene with images must be refused.
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
	let (mut pen, mut fonts) = (QuadPen::new(SIZE, ATLAS), fonts());
	let mut unlike = Vec::new();
	for case in cases() {
		let view = case.view();
		pen.begin_frame(SIZE);
		// What the software renderer starts from.
		pen.rect(0, 0, width, height, BG);
		draw_grid(&mut pen, &mut fonts, &view, None);
		let images = !case.frame.images.is_empty();
		let mut recording = match renderer.frame(&mut pen, &target) {
			Err(Error::Draw) if images => continue,
			Err(error) => return Err(error),
			Ok(_) if images => {
				unlike.push(format!("{}: drawn with images", case.name));
				continue;
			}
			Ok(recording) => recording,
		};
		recording.download(&target, whole, &readback, 0)?;
		recording.submit()?;
		renderer.finish()?;
		let read = readback.bytes().ok_or(Error::NoMemory)?;
		let read: Vec<u32> = (read.as_chunks::<4>().0.iter())
			.map(|&chunk| u32::from_ne_bytes(chunk))
			.collect();
		let differ = differing(&read, &direct(&view), &layers(&pen));
		if differ > 0 {
			unlike.push(format!("{}: {differ} pixels", case.name));
		}
	}
	Ok(unlike)
}

#[test]
fn every_grid_scene_drawn_by_the_gpu_paints_what_software_paints() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	assert_eq!(unlike_software(&device), Ok(vec![]));
}

#[test]
fn pixels_differ_by_more_than_their_layers_in_a_color_channel_and_not_in_alpha() {
	let painted = [0x0010_2030; 5];
	let drawn = [
		0xff10_2030,
		0xff11_1f31,
		0x0012_2030,
		0x0010_2230,
		0x0010_2033,
	];
	assert_eq!(differing(&drawn, &painted, &[0; 5]), 3);
	assert_eq!(differing(&drawn, &painted, &[1; 5]), 3);
	// Two layers allow two steps, not three.
	assert_eq!(differing(&drawn, &painted, &[2; 5]), 1);
	assert_eq!(differing(&drawn, &painted, &[3; 5]), 0);
}

#[test]
fn layers_count_translucent_quads_since_the_last_opaque_one() {
	let mut pen = QuadPen::new((8, 4), (16, 1));
	let quad = |at, size, clip, fill, italic| Quad {
		at,
		size,
		clip,
		fill,
		italic,
		color: 0,
	};
	let slot = Slot {
		page: 0,
		x: 0,
		y: 0,
		w: 4,
		h: 4,
	};
	let whole = (0, 0, 8, 4);
	pen.quads = vec![
		quad((-2, 0), (4, 4), whole, Fill::Coverage(slot), false),
		quad((0, 0), (4, 4), (1, 1, 8, 3), Fill::Color(slot), false),
		// Opaque over the first column.
		quad((0, 0), (1, 4), whole, Fill::Solid, false),
		// Leans right by up to 6 / 6 = 1 pixel, and runs off the bottom.
		quad((5, 2), (2, 6), whole, Fill::Coverage(slot), true),
	];
	assert_eq!(
		layers(&pen),
		[
			[0, 1, 0, 0, 0, 0, 0, 0],
			[0, 2, 1, 1, 0, 0, 0, 0],
			[0, 2, 1, 1, 0, 1, 1, 1],
			[0, 1, 0, 0, 0, 1, 1, 1],
		]
		.concat()
	);
}
