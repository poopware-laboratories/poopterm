//! The grid on Metal: once the GPU is done with the last frame, a frame's
//! quads written where the shaders read them, the glyphs the atlas lacks
//! written to its pages and the images it lacks to textures of their own,
//! and the quads drawn over a target, a run for each image.
use super::{
	buffer::Buffer,
	commands::{Draw, Frame, Submitted},
	device::{Device, Error, TEXTURE_LIMITS},
	pipeline::Pipeline,
	texture::{Texture, Usage},
};
use crate::{
	gpu::{
		atlas::{BUDGET, Wanted, grown},
		encode::{QUAD, encode},
		quads::{Fill, QuadPen},
		shrink::{in_texture, regrown, shown_sizes, shrink, texture_size},
		textures::{Textures, runs},
	},
	graphics::Bitmap,
};
use std::{num::NonZeroUsize, ops::Range, rc::Rc, sync::Arc};

/// Bytes the images' textures may take on the GPU: a frame needing more is
/// refused, for software to draw.
pub(crate) const TEXTURES: usize = 256 << 20;

pub(crate) struct Renderer {
	pub(super) device: Rc<Device>,
	pipeline: Pipeline,
	/// Pages of glyphs, as the pen's atlas lays them out.
	pub(super) atlas: Texture,
	/// The images' textures, and the one bound where no image is drawn.
	pub(super) textures: Textures<Rc<Texture>>,
	blank: Texture,
	/// The frame's quads; grown as frames need.
	pub(super) quads: Buffer,
	/// The last frame, which the GPU may still be drawing.
	last: Option<Submitted>,
}

impl Renderer {
	/// A renderer on `device` with an atlas of `pages` pages `side` pixels
	/// square.
	pub(crate) fn new(device: &Rc<Device>, (side, pages): (usize, usize)) -> Result<Self, Error> {
		let atlas = [side, side, pages].map(|n| u32::try_from(n).unwrap_or(u32::MAX));
		let bytes = NonZeroUsize::new(QUAD * 4).ok_or(Error::Draw)?;
		Ok(Self {
			pipeline: Pipeline::new(device)?,
			atlas: Texture::new(device, atlas, true, Usage::Sampled)?,
			textures: Textures::new(TEXTURES),
			blank: Texture::new(device, [1, 1, 1], false, Usage::Sampled)?,
			quads: Buffer::new(device, bytes)?,
			device: device.clone(),
			last: None,
		})
	}

	/// Waits for the GPU to be done with the last frame.
	pub(crate) fn finish(&mut self) -> Result<(), Error> {
		match self.last.take() {
			Some(last) => last.wait(),
			None => Ok(()),
		}
	}

	/// Starts `pen` and the textures over, so the next frame uploads every
	/// glyph and image it draws: for when a frame's uploads never reached
	/// the GPU.
	pub(crate) fn restart(&mut self, pen: &mut QuadPen) {
		let [side, _, pages] = self.atlas.size.map(|n| n as usize);
		*pen = QuadPen::new(pen.size, (side, pages));
		self.textures.clear();
	}

	/// Makes the atlas larger for a frame that lacked room in it, when
	/// Metal and the budget allow one that holds more: whether it did. The
	/// frames drawn before keep the atlas they sample.
	pub(crate) fn grow(&mut self, wanted: Wanted) -> Result<bool, Error> {
		let [side, _, pages] = self.atlas.size.map(|n| n as usize);
		let Some((side, pages)) = grown((side, pages), wanted, TEXTURE_LIMITS, BUDGET) else {
			return Ok(false);
		};
		let size = [side, side, pages].map(|n| n as u32);
		self.atlas = Texture::new(&self.device, size, true, Usage::Sampled)?;
		Ok(true)
	}

	/// Draws `pen`'s frame over `target`, once the GPU is done with the
	/// last, has `then` add to its commands (to show or read back the
	/// target), and commits them. A frame lacking glyphs, or with images
	/// the GPU cannot hold, is refused; a frame that fails starts the pen
	/// and the textures over, as what it uploaded may be lost.
	pub(crate) fn frame(
		&mut self,
		pen: &mut QuadPen,
		target: &Texture,
		then: impl FnOnce(&mut Frame) -> Result<(), Error>,
	) -> Result<(), Error> {
		let drawn = self.record(pen, target, then);
		if drawn.is_err() {
			self.restart(pen);
		}
		drawn
	}

	/// The texture of `bitmap`, `shown` that large, made, uploaded and
	/// kept when new: its own size, or shrunk when Metal or the budget
	/// cannot hold that; refused when not even a texel fits.
	fn texture(&mut self, bitmap: &Arc<Bitmap>, shown: (usize, usize)) -> Result<Rc<Texture>, Error> {
		let own = (bitmap.width, bitmap.height);
		let dimension = TEXTURE_LIMITS[0] as usize;
		if let Some(texture) = self.textures.get(bitmap).cloned() {
			let [width, height, _] = texture.size.map(|n| n as usize);
			if regrown((width, height), own, shown, dimension, self.textures.room()).is_none() {
				return Ok(texture);
			}
			// Shrunk, and now shown larger than it holds.
			self.textures.remove(bitmap);
		}
		let size = texture_size(own, shown, dimension, self.textures.room()).ok_or(Error::Draw)?;
		let bytes = size.0 * size.1 * 4;
		if !self.textures.make_room(bytes) {
			return Err(Error::Draw);
		}
		let texels = [size.0, size.1].map(|n| n as u32);
		let texture = Texture::new(
			&self.device,
			[texels[0], texels[1], 1],
			false,
			Usage::Sampled,
		)?;
		let shrunk = (size != own).then(|| shrink(bitmap, size));
		let pixels = &shrunk.as_ref().map_or(&**bitmap, |shrunk| shrunk).pixels;
		texture.upload(0, [0, 0], texels, pixels)?;
		Ok(
			self
				.textures
				.insert(bitmap, Rc::new(texture), bytes)
				.clone(),
		)
	}

	fn record(
		&mut self,
		pen: &QuadPen,
		target: &Texture,
		then: impl FnOnce(&mut Frame) -> Result<(), Error>,
	) -> Result<(), Error> {
		if !pen.complete {
			return Err(Error::Draw);
		}
		// What follows writes what the last frame reads.
		self.finish()?;
		self.textures.begin_frame();
		let mut shown = Vec::with_capacity(pen.images.len());
		for (image, size) in pen.images.iter().zip(shown_sizes(pen)) {
			shown.push(self.texture(&image.placement.bitmap, size)?);
		}
		// Room for the next power of two quads, as frames grow a few at a time.
		if self.quads.size < pen.quads.len() * QUAD * 4 {
			let bytes = pen.quads.len().next_power_of_two() * QUAD * 4;
			let bytes = NonZeroUsize::new(bytes).ok_or(Error::Draw)?;
			self.quads = Buffer::new(&self.device, bytes)?;
		}
		let words = self.quads.bytes_mut().as_chunks_mut::<4>().0;
		for (chunks, &quad) in words.as_chunks_mut::<QUAD>().0.iter_mut().zip(&pen.quads) {
			let quad = match quad.fill {
				Fill::Image(index, _) => {
					let bitmap = &pen.images[index].placement.bitmap;
					let [width, height, _] = shown[index].size.map(|n| n as usize);
					in_texture(quad, (bitmap.width, bitmap.height), (width, height))
				}
				_ => quad,
			};
			let encoded = encode(&quad).ok_or(Error::Draw)?;
			for (chunk, word) in chunks.iter_mut().zip(encoded) {
				*chunk = word.to_ne_bytes();
			}
		}
		for upload in &pen.uploads {
			let slot = upload.slot;
			let at = [slot.x, slot.y].map(|n| n as u32);
			let size = [slot.w, slot.h].map(|n| n as u32);
			self
				.atlas
				.upload(slot.page as u32, at, size, &upload.pixels)?;
		}
		let needs = pen.quads.iter().map(|quad| match quad.fill {
			Fill::Image(index, _) => Some(Rc::as_ptr(&shown[index])),
			_ => None,
		});
		let runs: Vec<(Range<u32>, &Texture)> = (runs(needs).into_iter())
			.map(|(range, need)| {
				let texture = shown
					.iter()
					.find(|texture| Some(Rc::as_ptr(texture)) == need);
				(range, texture.map_or(&self.blank, |texture| &**texture))
			})
			.collect();
		let mut frame = Frame::new(&self.device)?;
		frame.draw(Draw {
			pipeline: &self.pipeline,
			quads: &self.quads,
			runs: &runs,
			atlas: &self.atlas,
			target,
		})?;
		then(&mut frame)?;
		self.last = Some(frame.commit());
		Ok(())
	}
}

#[cfg(test)]
#[path = "renderer_tests.rs"]
mod tests;
