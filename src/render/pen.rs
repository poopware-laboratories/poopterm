//! What the terminal grid draws with: a [`Painter`] puts the pixels down at
//! once, and a GPU renderer takes the same calls to draw them itself.
use super::{
	glyphs::{Glyph, Shape},
	images::draw_placement,
	painter::Painter,
};
use crate::graphics::{Placement, Rect};

/// What a glyph bitmap shows, for a renderer that draws glyphs itself.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(
	not(test),
	expect(dead_code, reason = "the GPU renderer reads it next (#22)")
)]
pub(crate) enum Ink<'a> {
	/// Font text at a pixel size: one character, or a cluster a font joins.
	Text(Shape<'a>, f32),
	/// A box drawing, block, Braille or Powerline character made to fill a
	/// cell of this size.
	Drawn(char, (usize, usize)),
}

pub(crate) trait Pen {
	/// The surface's width and height in pixels.
	fn size(&self) -> (usize, usize);

	fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32);

	/// Draws `glyph`, the bitmap of `ink`, with its top-left corner at
	/// `origin`, clipped to `clip` (left, top, right, bottom).
	#[allow(clippy::too_many_arguments)]
	fn glyph(
		&mut self,
		glyph: &Glyph,
		ink: Ink,
		origin: (i32, i32),
		clip: (i32, i32, i32, i32),
		italic: bool,
		color: u32,
	);

	/// Draws the part of an image inside `clip`, its top-left corner at `at`.
	fn placement(
		&mut self,
		clip: Rect,
		placement: &Placement,
		at: (isize, isize),
		cell: (usize, usize),
	);
}

impl Pen for Painter<'_> {
	fn size(&self) -> (usize, usize) {
		(self.width, self.height)
	}

	fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
		Painter::rect(self, x, y, w, h, color);
	}

	#[inline]
	fn glyph(
		&mut self,
		glyph: &Glyph,
		_: Ink,
		origin: (i32, i32),
		clip: (i32, i32, i32, i32),
		italic: bool,
		color: u32,
	) {
		Painter::glyph(self, glyph, origin, clip, italic, color);
	}

	fn placement(
		&mut self,
		clip: Rect,
		placement: &Placement,
		at: (isize, isize),
		cell: (usize, usize),
	) {
		draw_placement(self.pixels, self.width, clip, placement, at, cell);
	}
}
