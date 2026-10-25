//! Extends the grid's edge cell backgrounds into the padding around it, so
//! an application's background reaches the window edge instead of stopping
//! at a frame of the theme color.
use super::{grid::GridView, pen::Pen};

/// Paints the padding next to grid row `row` with its edge cells'
/// backgrounds, and above or below it for the first and last rows,
/// corners included. Default backgrounds are left alone; the caller has
/// already restored the window background there.
pub(crate) fn extend_padding(p: &mut impl Pen, view: &GridView, row: usize) {
	let Some(area) = view.padding else {
		return;
	};
	let cells = view.frame.row(row);
	let (Some(first), Some(last)) = (cells.first(), cells.last()) else {
		return;
	};
	let y = view.origin.1 + row * view.cell.1;
	let right = view.origin.0 + view.frame.cols * view.cell.0;
	fill(
		p,
		view,
		(area.x, view.origin.0),
		(y, y + view.cell.1),
		first.bg,
	);
	fill(
		p,
		view,
		(right, area.x + area.width),
		(y, y + view.cell.1),
		last.bg,
	);
	if row == 0 {
		extend_above(p, view);
	}
	if row + 1 == view.frame.rows {
		extend_below(p, view);
	}
}

/// Paints the padding above the grid with the first row's backgrounds.
pub(crate) fn extend_above(p: &mut impl Pen, view: &GridView) {
	if let Some(area) = view.padding {
		extend_row(p, view, 0, (area.y, view.origin.1));
	}
}

/// Paints the padding below the grid with the last row's backgrounds.
pub(crate) fn extend_below(p: &mut impl Pen, view: &GridView) {
	if let Some(area) = view.padding {
		let bottom = view.origin.1 + view.frame.rows * view.cell.1;
		let row = view.frame.rows.saturating_sub(1);
		extend_row(p, view, row, (bottom, area.y + area.height));
	}
}

/// Paints pixel rows `ys` across the padding area with `row`'s
/// backgrounds, each cell's under it and the edge cells' to the sides.
/// Only a row colored all the way across extends: a partly colored one,
/// such as less's reverse-video prompt, would leave a band that looks
/// like a selected line.
fn extend_row(p: &mut impl Pen, view: &GridView, row: usize, ys: (usize, usize)) {
	let (Some(area), Some(cells)) = (
		view.padding,
		(row < view.frame.rows).then(|| view.frame.row(row)),
	) else {
		return;
	};
	if cells.iter().any(|cell| cell.bg == view.default_bg) {
		return;
	}
	let (Some(first), Some(last)) = (cells.first(), cells.last()) else {
		return;
	};
	let (left, cw) = (view.origin.0, view.cell.0);
	fill(p, view, (area.x, left), ys, first.bg);
	for (col, cell) in cells.iter().enumerate() {
		fill(
			p,
			view,
			(left + col * cw, left + (col + 1) * cw),
			ys,
			cell.bg,
		);
	}
	fill(
		p,
		view,
		(left + cells.len() * cw, area.x + area.width),
		ys,
		last.bg,
	);
}

fn fill(
	p: &mut impl Pen,
	view: &GridView,
	(x0, x1): (usize, usize),
	(y0, y1): (usize, usize),
	color: u32,
) {
	if color != view.default_bg && x1 > x0 && y1 > y0 {
		p.rect(x0, y0, x1 - x0, y1 - y0, color);
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		graphics::Rect,
		parser::Parser,
		render::{glyphs::FontSet, grid::GridView, painter::Painter},
		term::{Frame, Term},
		theme::{Theme, ThemeKind},
	};

	const RED: u32 = 0xcd0000;
	const BLUE: u32 = 0x0000ee;
	/// What the caller painted before drawing: the restored background.
	const UNDER: u32 = 0x5a5a5a;

	fn frame(input: &str) -> Frame {
		let mut term = Term::new(4, 3, Theme::from_kind(ThemeKind::Poop), "/".into());
		Parser::new().advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		frame
	}

	/// Draws a 4×3 grid of 10×20 cells at (5, 7) inside a 52×80 surface
	/// whose padding area starts at (1, 2), and returns the pixels.
	fn draw(frame: &Frame, rows: Option<&[bool]>) -> Vec<u32> {
		let (width, height) = (52, 80);
		let mut pixels = vec![UNDER; width * height];
		let mut painter = Painter::new(&mut pixels, width, height);
		let view = GridView {
			frame,
			origin: (5, 7),
			cell: (10, 20),
			px: 16.0,
			ascent: 15,
			scale: 1,
			default_bg: Theme::from_kind(ThemeKind::Poop).bg,
			link_color: 0,
			selection_colors: (0, 0),
			match_colors: (0, 0),
			selection: None,
			search_match: None,
			focused: true,
			padding: Some(Rect {
				x: 1,
				y: 2,
				width: 50,
				height: 76,
			}),
		};
		let mut fonts = FontSet::new(
			crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap(),
			Vec::new(),
		);
		crate::render::grid::draw_grid(&mut painter, &mut fonts, &view, rows);
		pixels
	}

	#[test]
	fn edge_backgrounds_fill_the_padding_beside_above_and_below() {
		// Row 0 red across, row 1 default, row 2 blue in its first cell only.
		let frame = frame("\x1b[48;2;205;0;0m\x1b[K\r\n\x1b[0m\r\n\x1b[48;2;0;0;238m \x1b[0m");
		let pixels = draw(&frame, None);
		let at = |x: usize, y: usize| pixels[y * 52 + x];
		// Left and right of row 0 (y 7..27), out to the padding area's edge.
		assert_eq!((at(1, 10), at(4, 10)), (RED, RED));
		assert_eq!((at(45, 10), at(50, 10)), (RED, RED));
		// Above row 0, corners included; nothing outside the padding area.
		assert_eq!((at(1, 2), at(20, 6), at(50, 2)), (RED, RED, RED));
		assert_eq!((at(0, 10), at(51, 10), at(20, 1)), (UNDER, UNDER, UNDER));
		// Default backgrounds leave the restored background alone.
		assert_eq!((at(2, 30), at(48, 30)), (UNDER, UNDER));
		// Row 2 (y 47..67): blue only where its edge cell is blue.
		assert_eq!((at(2, 50), at(48, 50)), (BLUE, UNDER));
		// Row 2 is only partly colored, so nothing extends below it.
		assert_eq!((at(2, 70), at(10, 77), at(20, 70)), (UNDER, UNDER, UNDER));
	}

	#[test]
	fn a_partly_colored_edge_row_does_not_extend_above_or_below() {
		// A less prompt: two reverse-video cells, then default ones.
		let frame = frame("\x1b[7mab\x1b[0m\x1b[3;1H\x1b[7mab\x1b[0m");
		let pixels = draw(&frame, None);
		let at = |x: usize, y: usize| pixels[y * 52 + x];
		// The prompt's own cells and the padding beside them stay colored.
		assert_ne!((at(2, 10), at(2, 50)), (UNDER, UNDER));
		// No band above the first row or below the last.
		assert_eq!((at(2, 4), at(10, 4), at(20, 4)), (UNDER, UNDER, UNDER));
		assert_eq!((at(2, 70), at(10, 77), at(20, 70)), (UNDER, UNDER, UNDER));
	}

	#[test]
	fn a_fully_colored_last_row_extends_below() {
		// A statusline: every cell of the last row colored.
		let frame = frame("\x1b[3;1H\x1b[48;2;0;0;238m\x1b[K");
		let pixels = draw(&frame, None);
		let at = |x: usize, y: usize| pixels[y * 52 + x];
		assert_eq!((at(1, 77), at(20, 70), at(50, 77)), (BLUE, BLUE, BLUE));
		assert_eq!(at(20, 78), UNDER);
	}

	#[test]
	fn only_drawn_rows_extend() {
		let frame = frame("\x1b[48;2;205;0;0m\x1b[K\r\n\x1b[K\r\n\x1b[K");
		let pixels = draw(&frame, Some(&[false, true, false]));
		let at = |x: usize, y: usize| pixels[y * 52 + x];
		assert_eq!((at(2, 30), at(48, 30)), (RED, RED));
		assert_eq!((at(2, 10), at(20, 4), at(20, 70)), (UNDER, UNDER, UNDER));
	}
}
