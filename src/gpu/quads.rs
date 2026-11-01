//! The grid as the GPU draws it: a [`Pen`] that turns each rectangle, glyph
//! and image into a quad, in drawing order, and keeps glyph bitmaps in an
//! atlas, listing the ones the GPU has not been sent yet.
use super::{
	atlas::{Atlas, Refused, Slot, Wanted},
	rows::Rows,
};
use crate::{
	graphics::{Placement, Rect},
	render::{
		glyphs::{Glyph, Shape},
		pen::{Ink, Pen},
		shapes::corner_coverage,
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
	/// The image [`QuadPen::images`] holds at this index: the part of its
	/// bitmap in the `Rect`, scaled to the quad's size.
	Image(usize, Rect),
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
	/// What the atlas lacked room for, for a larger one to hold.
	pub wanted: Wanted,
	pub(super) atlas: Atlas<Box<[u32]>>,
	/// The last frame's grid rows, for the next to keep.
	pub(super) rows: Rows,
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
			wanted: Wanted::default(),
			atlas: Atlas::new(side, pages),
			rows: Rows::default(),
			key: Vec::new(),
		}
	}

	/// Starts a frame `size` pixels large, forgetting the last one's quads.
	pub(crate) fn begin_frame(&mut self, size: (usize, usize)) {
		self.size = size;
		self.images.clear();
		self.uploads.clear();
		self.complete = true;
		self.wanted = Wanted::default();
		self.atlas.begin_frame();
		self.begin_rows();
	}

	/// The slot of `glyph`, the bitmap of `ink`, uploading it when new.
	fn glyph_slot(&mut self, glyph: &Glyph, ink: Ink) -> Option<Slot> {
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
			Ink::Ui(ch, px) => self.key.extend([3, px.to_bits(), ch as u32]),
		}
		let size = (glyph.metrics.width, glyph.metrics.height);
		self.slot(size, || match &glyph.color {
			Some(pixels) => pixels.clone(),
			None => coverage_texels(glyph.coverage.iter().copied()),
		})
	}

	/// The slot under the key being looked up, holding `size` pixels that
	/// `pixels` makes when the atlas lacks them; `None`, and the frame
	/// incomplete, when the atlas cannot hold them.
	fn slot(&mut self, size: (usize, usize), pixels: impl FnOnce() -> Vec<u32>) -> Option<Slot> {
		if let Some(slot) = self.atlas.get(&self.key[..]) {
			return Some(slot);
		}
		let slot = match self.atlas.insert(self.key.as_slice().into(), size) {
			Ok(slot) => slot,
			Err(refused) => {
				self.complete = false;
				match refused {
					Refused::TooBig => self.wanted.side = self.wanted.side.max(size.0).max(size.1),
					Refused::Full => self.wanted.pages = true,
				}
				return None;
			}
		};
		let pixels = pixels();
		self.uploads.push(Upload { slot, pixels });
		Some(slot)
	}

	/// The coverage quad of a rounded corner `radius` wide at `at`, the
	/// `right` and `bottom` one of its rectangle as asked.
	fn corner(
		&mut self,
		at: (usize, usize),
		radius: usize,
		(right, bottom): (bool, bool),
		color: u32,
	) {
		self.key.clear();
		self
			.key
			.extend([4, radius as u32, u32::from(right), u32::from(bottom)]);
		let texel = |column: usize, row: usize| {
			let column = if right { radius - 1 - column } else { column };
			let row = if bottom { radius - 1 - row } else { row };
			corner_coverage(radius, column, row) as u8
		};
		let texels = || {
			let rows = (0..radius).flat_map(|row| (0..radius).map(move |column| (column, row)));
			coverage_texels(rows.map(|(column, row)| texel(column, row)))
		};
		let Some(slot) = self.slot((radius, radius), texels) else {
			return;
		};
		let (x, y) = (at.0 as i32, at.1 as i32);
		let side = radius as i32;
		self.quads.push(Quad {
			at: (x, y),
			size: (radius, radius),
			clip: (x, y, x + side, y + side),
			fill: Fill::Coverage(slot),
			italic: false,
			color,
		});
	}
}

/// Coverage as the atlas holds it: white, at the coverage's alpha.
fn coverage_texels(coverage: impl Iterator<Item = u8>) -> Vec<u32> {
	coverage.map(|c| u32::from(c) << 24 | 0xff_ffff).collect()
}

impl Pen for QuadPen {
	fn size(&self) -> (usize, usize) {
		self.size
	}

	fn begin_row(&mut self, row: usize) {
		self.row_starts(row);
	}

	fn end_grid(&mut self, rows: usize) {
		self.grid_ends(rows);
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

	/// As [`crate::render::shapes::round_rect`] paints it: three rectangles,
	/// and a coverage quad for each corner.
	fn round_rect(&mut self, (x, y, w, h): (usize, usize, usize, usize), radius: usize, color: u32) {
		if w == 0 || h == 0 {
			return;
		}
		let r = radius.min(h / 2).min(w / 2);
		self.rect(x, y + r, w, h - 2 * r, color);
		if r == 0 {
			return;
		}
		self.rect(x + r, y, w - 2 * r, r, color);
		self.rect(x + r, y + h - r, w - 2 * r, r, color);
		let (right, bottom) = (x + w - r, y + h - r);
		for (at, side) in [
			((x, y), (false, false)),
			((right, y), (true, false)),
			((x, bottom), (false, true)),
			((right, bottom), (true, true)),
		] {
			self.corner(at, r, side, color);
		}
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
		let Some(slot) = self.glyph_slot(glyph, ink) else {
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
		let (width, height) = placement.size(cell);
		let source = placement.source;
		if width == 0 || height == 0 || source.width == 0 || source.height == 0 {
			return;
		}
		let left = (at.0 + placement.offset.0 as isize) as i32;
		let top = (at.1 + placement.offset.1 as isize) as i32;
		let (x, y) = (clip.x as i32, clip.y as i32);
		let clip = (x, y, x + clip.width as i32, y + clip.height as i32);
		// The grid draws each image once a row: rows it misses add nothing.
		let (right, bottom) = (left + width as i32, top + height as i32);
		if left.max(clip.0) >= right.min(clip.2) || top.max(clip.1) >= bottom.min(clip.3) {
			return;
		}
		self.quads.push(Quad {
			at: (left, top),
			size: (width, height),
			clip,
			fill: Fill::Image(self.images.len(), source),
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
