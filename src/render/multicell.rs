//! Draws scaled text (kitty's OSC 66) over its block of cells, one row of
//! the block at a time, so rows redraw alone.

use super::{
	glyphs::{FontSet, Presentation, Shape},
	grid::GridView,
	pen::{Ink, Pen},
};
use crate::term::multicell::Multicell;

/// Where a block's text goes: its offset from the block's top-left corner
/// and the size of one cell of text, in pixels.
#[derive(Debug, PartialEq)]
pub(super) struct TextBox {
	pub x: f32,
	pub y: f32,
	pub cell_w: f32,
	pub cell_h: f32,
	/// How much larger than normal text it is.
	pub scale: f32,
}

/// Lays out `part`'s text with cells `cell` pixels in size: scaled, then
/// shrunk by the fraction and placed by the alignment keys.
pub(super) fn text_box(part: &Multicell, (cw, ch): (usize, usize)) -> TextBox {
	let size = part.size;
	let scale = f32::from(size.scale) * size.fraction();
	let (cell_w, cell_h) = (cw as f32 * scale, ch as f32 * scale);
	let text_cells: usize = part.text.chars().map(crate::width::width).sum();
	let free_x = (usize::from(part.columns) * cw) as f32 - text_cells as f32 * cell_w;
	let free_y = (usize::from(size.scale) * ch) as f32 - cell_h;
	// The alignment keys place fractionally scaled text only.
	let fractional = size.fraction() < 1.0;
	let align = |free: f32, key: u8| match key {
		1 if fractional => free.max(0.0),
		2 if fractional => free.max(0.0) / 2.0,
		_ => 0.0,
	};
	TextBox {
		x: align(free_x, size.horizontal),
		y: align(free_y, size.vertical),
		cell_w,
		cell_h,
		scale,
	}
}

/// Draws the row of `part`'s block that the cell at `(x0, y0)` starts,
/// clipped to that row and the block's columns.
pub(super) fn draw_multicell(
	p: &mut impl Pen,
	fonts: &mut FontSet,
	view: &GridView,
	part: &Multicell,
	(x0, y0): (usize, usize),
	italic: bool,
	fg: u32,
) {
	let (cw, ch) = view.cell;
	let layout = text_box(part, view.cell);
	let left = x0 as i32;
	let top = y0 as i32 - (usize::from(part.dy) * ch) as i32;
	let px = view.px * layout.scale;
	let baseline = top + (layout.y + view.ascent as f32 * layout.scale).round() as i32;
	let clip = (
		left,
		y0 as i32,
		(x0 + usize::from(part.columns) * cw) as i32,
		(y0 + ch) as i32,
	);
	let mut pen = layout.x;
	// The span of the last character, which marks are centered over.
	let mut last = (pen, layout.cell_w);
	let mut chars = part.text.chars().peekable();
	while let Some(c) = chars.next() {
		let width = crate::width::width(c);
		if width == 0 && matches!(c, '\u{200d}' | '\u{fe0e}' | '\u{fe0f}') {
			continue;
		}
		let presentation = match chars.peek() {
			Some(&next) => Presentation::of(&[next]),
			None => Presentation::Default,
		};
		let glyph = fonts.presented(c, px, presentation);
		let x = if width == 0 {
			last.0 + (last.1 - glyph.metrics.width as f32) / 2.0
		} else {
			last = (pen, width as f32 * layout.cell_w);
			pen + glyph.metrics.xmin as f32
		};
		let y = baseline - glyph.metrics.ymin - glyph.metrics.height as i32;
		let ink = Ink::Text(Shape::Char(c, presentation), px);
		p.glyph(glyph, ink, (left + x.round() as i32, y), clip, italic, fg);
		pen += width as f32 * layout.cell_w;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::term::multicell::Size;

	fn part(text: &str, size: Size, columns: u8) -> Multicell {
		Multicell {
			text: text.into(),
			size,
			columns,
			dx: 0,
			dy: 0,
		}
	}

	#[test]
	fn scaled_text_fills_its_block() {
		let size = Size {
			scale: 3,
			width: 1,
			..Size::default()
		};
		let layout = text_box(&part("a", size, 3), (10, 20));
		assert_eq!(
			layout,
			TextBox {
				x: 0.0,
				y: 0.0,
				cell_w: 30.0,
				cell_h: 60.0,
				scale: 3.0,
			}
		);
	}

	#[test]
	fn fractions_shrink_text_inside_the_block() {
		let size = |vertical, horizontal| Size {
			scale: 2,
			width: 2,
			numerator: 1,
			denominator: 2,
			vertical,
			horizontal,
		};
		// One cell of text at half of double size is 10×20 in a 40×40 block.
		let at = |vertical, horizontal| {
			let layout = text_box(&part("a", size(vertical, horizontal), 4), (10, 20));
			assert_eq!(
				(layout.cell_w, layout.cell_h, layout.scale),
				(10.0, 20.0, 1.0)
			);
			(layout.x, layout.y)
		};
		assert_eq!(at(0, 0), (0.0, 0.0));
		assert_eq!(at(1, 1), (30.0, 20.0));
		assert_eq!(at(2, 2), (15.0, 10.0));
		// Text wider than the block starts at its left edge.
		let layout = text_box(&part("abcdefgh", size(0, 1), 4), (10, 20));
		assert_eq!(layout.x, 0.0);
		// Alignment is for fractions only; whole sizes start at the corner.
		let whole = Size {
			width: 3,
			horizontal: 2,
			vertical: 1,
			..Size::default()
		};
		let layout = text_box(&part("c", whole, 3), (10, 20));
		assert_eq!((layout.x, layout.y), (0.0, 0.0));
	}
}
