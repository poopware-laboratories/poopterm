//! The grid as the GPU draws it: a [`Pen`] that turns each rectangle, glyph
//! and image into a quad, in drawing order, and keeps glyph bitmaps in an
//! atlas, listing the ones the GPU has not been sent yet.
use super::atlas::{Atlas, Slot};
use crate::{
	graphics::{Placement, Rect},
	render::{
		glyphs::{Glyph, Shape},
		pen::{Ink, Pen},
	},
};

/// What a quad is filled with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Fill {
	/// Its color.
	Solid,
	/// Its color, at the alpha of the atlas texels in `Slot`.
	Coverage(Slot),
	/// The atlas texels in `Slot`, straight alpha.
	Color(Slot),
	/// The image [`QuadPen::images`] holds at this index.
	Image(usize),
}

/// A rectangle drawn at `at`, `size` pixels large, showing only what lies
/// inside `clip` (left, top, right, bottom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Quad {
	pub at: (i32, i32),
	pub size: (usize, usize),
	pub clip: (i32, i32, i32, i32),
	pub fill: Fill,
	/// Shears coverage rows as [`crate::render::painter::Painter::glyph`] does.
	pub italic: bool,
	pub color: u32,
}

/// Pixels for the GPU to copy into `slot` of the atlas, row by row.
#[derive(Debug)]
pub(crate) struct Upload {
	pub slot: Slot,
	/// `0xAARRGGBB`, straight alpha; coverage is white at its alpha.
	pub pixels: Vec<u32>,
}

/// An image drawn: its placement, top-left corner and cell size.
#[derive(Debug)]
pub(crate) struct Image {
	pub placement: Placement,
	pub at: (isize, isize),
	pub cell: (usize, usize),
}

/// A frame's quads, with the images and uploads they need.
pub(crate) struct QuadPen {
	pub size: (usize, usize),
	pub quads: Vec<Quad>,
	pub images: Vec<Image>,
	pub uploads: Vec<Upload>,
	/// False when the atlas could not hold a glyph, which the frame lacks.
	pub complete: bool,
	atlas: Atlas<Box<[u32]>>,
	/// The key being looked up, kept to save allocating one per glyph.
	key: Vec<u32>,
}

impl QuadPen {
	/// A pen for a `size` surface, with an atlas of up to `pages` pages
	/// `side` pixels square.
	pub(crate) fn new(size: (usize, usize), (side, pages): (usize, usize)) -> Self {
		Self {
			size,
			quads: Vec::new(),
			images: Vec::new(),
			uploads: Vec::new(),
			complete: true,
			atlas: Atlas::new(side, pages),
			key: Vec::new(),
		}
	}

	/// Starts a frame `size` pixels large, forgetting the last one's quads.
	pub(crate) fn begin_frame(&mut self, size: (usize, usize)) {
		self.size = size;
		self.quads.clear();
		self.images.clear();
		self.uploads.clear();
		self.complete = true;
		self.atlas.begin_frame();
	}

	/// The slot of `glyph`, the bitmap of `ink`, uploading it when new.
	fn slot(&mut self, glyph: &Glyph, ink: Ink) -> Option<Slot> {
		self.key.clear();
		match ink {
			Ink::Text(Shape::Char(ch, presentation), px) => {
				(self.key).extend([0, px.to_bits(), presentation as u32, ch as u32]);
			}
			Ink::Text(Shape::Cluster(chars), px) => {
				self.key.extend([1, px.to_bits()]);
				self.key.extend(chars.iter().map(|&ch| ch as u32));
			}
			Ink::Drawn(ch, (w, h)) => self.key.extend([2, w as u32, h as u32, ch as u32]),
		}
		if let Some(slot) = self.atlas.get(&self.key[..]) {
			return Some(slot);
		}
		let size = (glyph.metrics.width, glyph.metrics.height);
		let Ok(slot) = self.atlas.insert(self.key.as_slice().into(), size) else {
			self.complete = false;
			return None;
		};
		let pixels = match &glyph.color {
			Some(pixels) => pixels.clone(),
			None => (glyph.coverage.iter())
				.map(|&c| u32::from(c) << 24 | 0xff_ffff)
				.collect(),
		};
		self.uploads.push(Upload { slot, pixels });
		Some(slot)
	}
}

impl Pen for QuadPen {
	fn size(&self) -> (usize, usize) {
		self.size
	}

	fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
		let (x, y) = (x as i32, y as i32);
		self.quads.push(Quad {
			at: (x, y),
			size: (w, h),
			clip: (x, y, x + w as i32, y + h as i32),
			fill: Fill::Solid,
			italic: false,
			color,
		});
	}

	fn glyph(
		&mut self,
		glyph: &Glyph,
		ink: Ink,
		origin: (i32, i32),
		clip: (i32, i32, i32, i32),
		italic: bool,
		color: u32,
	) {
		if glyph.metrics.width == 0 || glyph.metrics.height == 0 {
			return;
		}
		let Some(slot) = self.slot(glyph, ink) else {
			return;
		};
		let fill = match glyph.color {
			Some(_) => Fill::Color(slot),
			None => Fill::Coverage(slot),
		};
		self.quads.push(Quad {
			at: origin,
			size: (slot.w, slot.h),
			clip,
			fill,
			italic,
			color,
		});
	}

	fn placement(
		&mut self,
		clip: Rect,
		placement: &Placement,
		at: (isize, isize),
		cell: (usize, usize),
	) {
		let (x, y) = (clip.x as i32, clip.y as i32);
		self.quads.push(Quad {
			at: (x, y),
			size: (clip.width, clip.height),
			clip: (x, y, x + clip.width as i32, y + clip.height as i32),
			fill: Fill::Image(self.images.len()),
			italic: false,
			color: 0,
		});
		self.images.push(Image {
			placement: placement.clone(),
			at,
			cell,
		});
	}
}
