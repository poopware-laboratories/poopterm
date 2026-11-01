//! The grid on the GPU: a frame's quads written where the shaders read
//! them, the glyphs the atlas lacks uploaded to its pages and the images
//! it lacks to textures of their own, and the quads drawn over a target,
//! a run for each image, recorded for the caller to finish and submit.
use super::{
	buffer::Buffer,
	call::Error,
	commands::Commands,
	descriptors::{QuadSet, TextureSet},
	device::Device,
	draw::Draw,
	image::{Format, Image},
	memory::Usage,
	pipeline::QuadPipeline,
	recording::{Recording, Region},
	timeline::Timeline,
};
use crate::{
	gpu::{
		atlas::Slot,
		quads::{Fill, Quad, QuadPen},
		textures::{Textures, runs},
	},
	graphics::Bitmap,
};
use std::{num::NonZeroUsize, ops::Range, rc::Rc, sync::Arc};

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
const IMAGE: u32 = 3;
/// Words a quad.
const QUAD: usize = 12;
/// Bytes the images' textures may take on the GPU: a frame needing more is
/// refused, for software to draw.
pub(crate) const TEXTURES: usize = 256 << 20;

/// `quad` as the shaders read it: its rectangle, its clip, and its atlas
/// position and page, or its image's source corner and size, then its kind
/// and color; `None` for an image source past what 16 bits hold.
pub(crate) fn encode(quad: &Quad) -> Option<[u32; QUAD]> {
	let (kind, from) = match quad.fill {
		Fill::Solid => (SOLID, [0, 0]),
		Fill::Coverage(slot) => (COVERAGE | u32::from(quad.italic) << 8, place(slot)),
		Fill::Color(slot) => (COLOR, place(slot)),
		Fill::Image(_, source) => {
			let [x, y, width, height] =
				[source.x, source.y, source.width, source.height].map(u16::try_from);
			let pair = |low: u16, high: u16| u32::from(low) | u32::from(high) << 16;
			(
				IMAGE,
				[pair(x.ok()?, y.ok()?), pair(width.ok()?, height.ok()?)],
			)
		}
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
	let mut words = [0; QUAD];
	for (word, value) in words.iter_mut().zip(place) {
		*word = value as u32;
	}
	words[8..].copy_from_slice(&[from[0], from[1], kind, quad.color]);
	Some(words)
}

/// A slot's atlas position x | y << 16 and its page.
fn place(slot: Slot) -> [u32; 2] {
	[(slot.x | slot.y << 16) as u32, slot.page as u32]
}

pub(crate) struct Renderer {
	commands: Commands,
	set: Rc<QuadSet>,
	/// Pages of glyphs, as the pen's atlas lays them out.
	atlas: Rc<Image>,
	/// The images' textures, and the one bound where no image is drawn.
	pub(super) textures: Textures<Rc<TextureSet>>,
	blank: Rc<TextureSet>,
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
			textures: Textures::new(TEXTURES),
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
			// Made last, after the atlas.
			blank: Rc::new(TextureSet::new(
				pipeline,
				&Rc::new(Image::new(
					device,
					Format::Bgra,
					[1, 1, 1],
					SAMPLED | TRANSFER_DST,
				)?),
			)?),
			backdrop: None,
		})
	}

	/// Records `pen`'s frame over `target`, once the GPU has finished the
	/// last: the caller may record more, and submits. A frame lacking
	/// glyphs, or with images the GPU cannot hold, is refused, for software
	/// to draw. A frame that fails never uploads its glyphs or images, so
	/// the pen starts over and forgets the textures it made, and the next
	/// frame uploads every one it draws.
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

	/// Starts `pen` and the textures over, so the next frame uploads every
	/// glyph and image it draws: for when a frame's uploads never reached
	/// the GPU.
	pub(crate) fn restart(&mut self, pen: &mut QuadPen) {
		let [side, _, pages] = self.atlas.size.map(|n| n as usize);
		*pen = QuadPen::new(pen.size, (side, pages));
		self.textures.clear();
	}

	/// The texture of `bitmap`, made and kept when new, and listed in
	/// `fresh` for the frame to upload; refused when the budget or the
	/// device cannot hold it.
	fn texture(
		&mut self,
		bitmap: &Arc<Bitmap>,
		fresh: &mut Vec<(Arc<Bitmap>, Rc<TextureSet>)>,
	) -> Result<Rc<TextureSet>, Error> {
		if let Some(texture) = self.textures.get(bitmap) {
			return Ok(texture.clone());
		}
		let size = [bitmap.width, bitmap.height].map(|n| u32::try_from(n).unwrap_or(u32::MAX));
		if !self.textures.make_room(bitmap) {
			return Err(Error::Draw);
		}
		let device = &self.set.pipeline.device;
		let image = Image::new(
			device,
			Format::Bgra,
			[size[0], size[1], 1],
			SAMPLED | TRANSFER_DST,
		);
		let image = image.map_err(|error| match error {
			Error::ImageSize(_) => Error::Draw,
			error => error,
		})?;
		let texture = Rc::new(TextureSet::new(&self.set.pipeline, &Rc::new(image))?);
		fresh.push((bitmap.clone(), texture.clone()));
		Ok(self.textures.insert(bitmap, texture).clone())
	}

	fn record(
		&mut self,
		pen: &QuadPen,
		target: &Rc<Image>,
		backdrop: Option<Backdrop>,
	) -> Result<Recording<'_>, Error> {
		if !pen.complete {
			return Err(Error::Draw);
		}
		self.textures.begin_frame();
		let mut fresh = Vec::new();
		let mut shown = Vec::with_capacity(pen.images.len());
		for image in &pen.images {
			match self.texture(&image.placement.bitmap, &mut fresh) {
				Ok(texture) => shown.push(texture),
				Err(error) => {
					for (bitmap, _) in &fresh {
						self.textures.remove(bitmap);
					}
					return Err(error);
				}
			}
		}
		let needs = (pen.quads.iter()).map(|quad| match quad.fill {
			Fill::Image(index, _) => Some(Rc::as_ptr(&shown[index])),
			_ => None,
		});
		let runs: Vec<(Range<u32>, Rc<TextureSet>)> = (runs(needs).into_iter())
			.map(|(range, need)| {
				let texture = shown
					.iter()
					.find(|texture| Some(Rc::as_ptr(texture)) == need);
				(range, texture.unwrap_or(&self.blank).clone())
			})
			.collect();
		let Self {
			commands,
			set,
			atlas,
			quads,
			staging,
			backdrop: backdrop_buffer,
			textures,
			..
		} = self;
		let (fresh_ref, runs_ref) = (&fresh, &runs);
		let recorded = (move || {
			// Moved in, so the recording may borrow it past the closure.
			let commands = commands;
			let mut recording = commands.record()?;
			let device = &set.pipeline.device;
			let words = writable(quads, device, (pen.quads.len(), QUAD * 4), STORAGE)?;
			for (chunks, quad) in words.as_chunks_mut::<QUAD>().0.iter_mut().zip(&pen.quads) {
				let encoded = encode(quad).ok_or(Error::Draw)?;
				for (chunk, word) in chunks.iter_mut().zip(encoded) {
					*chunk = word.to_ne_bytes();
				}
			}
			let glyphs = pen.uploads.iter().map(|upload| &upload.pixels[..]);
			let images = fresh_ref.iter().map(|(bitmap, _)| &bitmap.pixels[..]);
			let texels = glyphs
				.clone()
				.chain(images.clone())
				.map(<[u32]>::len)
				.sum::<usize>();
			let words = writable(staging, device, (texels, 4), TRANSFER_SRC)?;
			let all = glyphs.chain(images).flatten();
			for (chunk, &texel) in words.iter_mut().zip(all) {
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
				recording.upload(staging, offset, atlas, region)?;
				offset += upload.pixels.len() as u64 * 4;
			}
			for (bitmap, texture) in fresh_ref {
				let region = Region {
					layer: 0,
					at: [0, 0],
					size: [bitmap.width as u32, bitmap.height as u32],
				};
				recording.upload(staging, offset, &texture.image, region)?;
				offset += bitmap.pixels.len() as u64 * 4;
			}
			if let Some(Backdrop { pixels, width }) = backdrop {
				upload_backdrop(&mut recording, backdrop_buffer, (pixels, width), target)?;
			}
			recording.draw(Draw {
				set,
				quads,
				runs: runs_ref,
				atlas,
				target,
			})?;
			Ok(recording)
		})();
		match recorded {
			Ok(recording) => Ok(recording),
			Err(error) => {
				for (bitmap, _) in &fresh {
					textures.remove(bitmap);
				}
				Err(error)
			}
		}
	}

	/// Waits for the GPU to finish every frame submitted.
	pub(crate) fn finish(&self) -> Result<(), Error> {
		self.commands.finish()
	}
}

/// Records copying the part of `pixels`, rows of `width`, that fits
/// `target` into it, packed into `buffer`, made for the first.
fn upload_backdrop(
	recording: &mut Recording,
	buffer: &mut Option<Rc<Buffer>>,
	(pixels, width): (&[u32], usize),
	target: &Rc<Image>,
) -> Result<(), Error> {
	let [columns, rows] = [
		width.min(target.size[0] as usize),
		(pixels.len().checked_div(width).unwrap_or(0)).min(target.size[1] as usize),
	];
	if columns * rows == 0 {
		return Ok(());
	}
	let (device, bytes) = (&target.device, columns * rows * 4);
	let buffer = match buffer {
		Some(buffer) => buffer,
		None => buffer.insert(self::buffer(device, bytes, TRANSFER_SRC)?),
	};
	let words = writable(buffer, device, (columns * rows, 4), TRANSFER_SRC)?;
	let packed = pixels.chunks_exact(width).flat_map(|row| &row[..columns]);
	for (chunk, &pixel) in words.iter_mut().zip(packed) {
		*chunk = pixel.to_ne_bytes();
	}
	let region = Region {
		layer: 0,
		at: [0, 0],
		size: [columns as u32, rows as u32],
	};
	recording.upload(buffer, 0, target, region)
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
