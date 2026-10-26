//! The grid on the GPU: a frame's quads written where the shaders read
//! them, the glyphs the atlas lacks uploaded to its pages, and the quads
//! drawn over a target, recorded for the caller to finish and submit.
use super::{
	buffer::Buffer,
	call::Error,
	commands::Commands,
	descriptors::QuadSet,
	device::Device,
	draw::Draw,
	image::{Format, Image},
	memory::Usage,
	pipeline::QuadPipeline,
	recording::{Recording, Region},
	timeline::Timeline,
};
use crate::gpu::quads::{Fill, Quad, QuadPen};
use std::{num::NonZeroUsize, rc::Rc};

/// `VK_BUFFER_USAGE_STORAGE_BUFFER_BIT` and `_TRANSFER_SRC_BIT`, and
/// `VK_IMAGE_USAGE_SAMPLED_BIT` and `_TRANSFER_DST_BIT`.
const STORAGE: u32 = 0x20;
const TRANSFER_SRC: u32 = 0x1;
const SAMPLED: u32 = 0x4;
const TRANSFER_DST: u32 = 0x2;
/// Quad kinds, as the shaders know them.
const SOLID: u32 = 0;
const COVERAGE: u32 = 1;
const COLOR: u32 = 2;
/// Words a quad.
const QUAD: usize = 12;

/// `quad` as the shaders read it: its rectangle, its clip, and its atlas
/// position, page, kind and color; `None` for an image, which they do not
/// draw.
pub(crate) fn encode(quad: &Quad) -> Option<[u32; QUAD]> {
	let (kind, slot) = match quad.fill {
		Fill::Solid => (SOLID, None),
		Fill::Coverage(slot) => (COVERAGE | u32::from(quad.italic) << 8, Some(slot)),
		Fill::Color(slot) => (COLOR, Some(slot)),
		Fill::Image(_) => return None,
	};
	let (x, y) = quad.at;
	let (left, top, right, bottom) = quad.clip;
	let place = [
		x,
		y,
		quad.size.0 as i32,
		quad.size.1 as i32,
		left,
		top,
		right,
		bottom,
	];
	let (at, page) = slot.map_or((0, 0), |slot| (slot.x | slot.y << 16, slot.page));
	let mut words = [0; QUAD];
	for (word, value) in words.iter_mut().zip(place) {
		*word = value as u32;
	}
	words[8..].copy_from_slice(&[at as u32, page as u32, kind, quad.color]);
	Some(words)
}

pub(crate) struct Renderer {
	commands: Commands,
	set: Rc<QuadSet>,
	/// Pages of glyphs, as the pen's atlas lays them out.
	atlas: Rc<Image>,
	/// The frame's quads, and the glyphs it uploads; grown as frames need,
	/// and written only once the GPU is done with the last frame.
	pub(super) quads: Rc<Buffer>,
	pub(super) staging: Rc<Buffer>,
	/// The backdrop, packed to the target's width; made for the first.
	pub(super) backdrop: Option<Rc<Buffer>>,
}

/// Pixels software painted, which a frame's quads are drawn over: rows of
/// `width` pixels from the top left corner.
#[derive(Clone, Copy)]
pub(crate) struct Backdrop<'p> {
	pub pixels: &'p [u32],
	pub width: usize,
}

impl Renderer {
	/// A renderer drawing with `pipeline`, raising `timeline`, with an
	/// atlas of `pages` pages `side` pixels square.
	pub(crate) fn new(
		pipeline: &Rc<QuadPipeline>,
		timeline: &Rc<Timeline>,
		(side, pages): (usize, usize),
	) -> Result<Self, Error> {
		let device = &pipeline.device;
		let size = [side, side, pages].map(|n| n as u32);
		Ok(Self {
			commands: Commands::new(timeline)?,
			set: Rc::new(QuadSet::new(pipeline)?),
			atlas: Rc::new(Image::new(
				device,
				Format::Bgra,
				size,
				SAMPLED | TRANSFER_DST,
			)?),
			quads: buffer(device, QUAD * 4, STORAGE)?,
			staging: buffer(device, 4, TRANSFER_SRC)?,
			backdrop: None,
		})
	}

	/// Records `pen`'s frame over `target`, once the GPU has finished the
	/// last: the caller may record more, and submits. A frame lacking
	/// glyphs or with images is refused, for software to draw. A frame that
	/// fails never uploads its glyphs, so the pen starts over and the next
	/// frame uploads every glyph it draws.
	pub(crate) fn frame(
		&mut self,
		pen: &mut QuadPen,
		target: &Rc<Image>,
	) -> Result<Recording<'_>, Error> {
		self.frame_over(pen, target, None)
	}

	/// Records `pen`'s frame as [`Self::frame`] does, over `backdrop`, which
	/// is first copied into the part of `target` it covers.
	pub(crate) fn frame_over(
		&mut self,
		pen: &mut QuadPen,
		target: &Rc<Image>,
		backdrop: Option<Backdrop>,
	) -> Result<Recording<'_>, Error> {
		let [side, _, pages] = self.atlas.size.map(|n| n as usize);
		let recorded = self.record(pen, target, backdrop);
		if recorded.is_err() {
			*pen = QuadPen::new(pen.size, (side, pages));
		}
		recorded
	}

	/// Starts `pen` over, so its next frame uploads every glyph it draws:
	/// for when a frame's uploads never reached the GPU.
	pub(crate) fn restart(&self, pen: &mut QuadPen) {
		let [side, _, pages] = self.atlas.size.map(|n| n as usize);
		*pen = QuadPen::new(pen.size, (side, pages));
	}

	fn record(
		&mut self,
		pen: &QuadPen,
		target: &Rc<Image>,
		backdrop: Option<Backdrop>,
	) -> Result<Recording<'_>, Error> {
		if !pen.complete || !pen.images.is_empty() {
			return Err(Error::Draw);
		}
		let mut recording = self.commands.record()?;
		let device = &self.set.pipeline.device;
		let quads = (pen.quads.iter()).filter_map(encode);
		let words = writable(
			&mut self.quads,
			device,
			(pen.quads.len(), QUAD * 4),
			STORAGE,
		)?;
		for (chunk, word) in words.iter_mut().zip(quads.flatten()) {
			*chunk = word.to_ne_bytes();
		}
		let texels = pen
			.uploads
			.iter()
			.map(|upload| upload.pixels.len())
			.sum::<usize>();
		let words = writable(&mut self.staging, device, (texels, 4), TRANSFER_SRC)?;
		for (chunk, &texel) in words
			.iter_mut()
			.zip(pen.uploads.iter().flat_map(|upload| &upload.pixels))
		{
			*chunk = texel.to_ne_bytes();
		}
		let mut offset = 0;
		for upload in &pen.uploads {
			let slot = upload.slot;
			let region = Region {
				layer: slot.page as u32,
				at: [slot.x as u32, slot.y as u32],
				size: [slot.w as u32, slot.h as u32],
			};
			recording.upload(&self.staging, offset, &self.atlas, region)?;
			offset += upload.pixels.len() as u64 * 4;
		}
		if let Some(Backdrop { pixels, width }) = backdrop {
			let [columns, rows] = [
				width.min(target.size[0] as usize),
				(pixels.len().checked_div(width).unwrap_or(0)).min(target.size[1] as usize),
			];
			if columns * rows > 0 {
				let bytes = columns * rows * 4;
				let backdrop = match &mut self.backdrop {
					Some(backdrop) => backdrop,
					None => self.backdrop.insert(buffer(device, bytes, TRANSFER_SRC)?),
				};
				let words = writable(backdrop, device, (columns * rows, 4), TRANSFER_SRC)?;
				let packed = pixels.chunks_exact(width).flat_map(|row| &row[..columns]);
				for (chunk, &pixel) in words.iter_mut().zip(packed) {
					*chunk = pixel.to_ne_bytes();
				}
				let region = Region {
					layer: 0,
					at: [0, 0],
					size: [columns as u32, rows as u32],
				};
				recording.upload(backdrop, 0, target, region)?;
			}
		}
		recording.draw(Draw {
			set: &self.set,
			quads: &self.quads,
			count: pen.quads.len() as u32,
			atlas: &self.atlas,
			target,
		})?;
		Ok(recording)
	}

	/// Waits for the GPU to finish every frame submitted.
	pub(crate) fn finish(&self) -> Result<(), Error> {
		self.commands.finish()
	}
}

/// A buffer of `bytes` for `usage`, mapped for the CPU to write.
fn buffer(device: &Rc<Device>, bytes: usize, usage: u32) -> Result<Rc<Buffer>, Error> {
	let size = NonZeroUsize::new(bytes).ok_or(Error::NoMemory)?;
	Ok(Rc::new(Buffer::new(device, size, usage, Usage::Upload)?))
}

/// The words of `buffer`, replaced by one of the next power of two items
/// when it holds fewer than `count` items of `size` bytes.
fn writable<'b>(
	buffer: &'b mut Rc<Buffer>,
	device: &Rc<Device>,
	(count, size): (usize, usize),
	usage: u32,
) -> Result<&'b mut [[u8; 4]], Error> {
	if buffer.size < count * size {
		*buffer = self::buffer(device, count.next_power_of_two() * size, usage)?;
	}
	// The commands let go of it once the GPU finished the last frame.
	let buffer = Rc::get_mut(buffer).ok_or(Error::Busy)?;
	let bytes = buffer.bytes_mut().ok_or(Error::NoMemory)?;
	Ok(bytes.as_chunks_mut::<4>().0)
}
