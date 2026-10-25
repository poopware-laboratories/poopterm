//! Draws the visible part of a terminal grid (cells, cursor, highlights, images).
use super::{
	blend::blend,
	boxdraw::{self, Drawn},
	cell_joins::{Join, join, may_join},
	fit::{fit_left, fit_scale},
	glyphs::{FontSet, Presentation, Shape, joins},
	images::draw_placement,
	multicell::draw_multicell,
	padding::extend_padding,
	painter::{Painter, glyph_top},
};
use crate::{
	graphics::{Placement, Rect, kitty::PLACEHOLDER},
	term::{Cell, CursorShape, Frame, NO_EXTRA, attr},
	ui::selection::{CellPoint, Selection},
};

/// Everything needed to draw one terminal viewport.
pub(crate) struct GridView<'a> {
	pub frame: &'a Frame,
	/// Pixel position of the top-left cell.
	pub origin: (usize, usize),
	/// Cell width and height in pixels.
	pub cell: (usize, usize),
	pub px: f32,
	pub ascent: i32,
	pub scale: usize,
	/// Cells with this background are skipped; the caller already cleared to it.
	pub default_bg: u32,
	pub link_color: u32,
	pub selection_colors: (u32, u32),
	pub match_colors: (u32, u32),
	pub selection: Option<Selection>,
	pub search_match: Option<Selection>,
	/// Unfocused windows draw a hollow block cursor.
	pub focused: bool,
	/// The area around the grid that edge cell backgrounds extend into.
	pub padding: Option<Rect>,
}

fn contains(range: Option<Selection>, point: CellPoint) -> bool {
	range.is_some_and(|range| range.contains(point))
}

/// Images with a lower z-index are drawn under non-default cell backgrounds
/// too, not only under text (kitty graphics protocol).
const UNDER_BACKGROUNDS: i32 = i32::MIN / 2;

/// What part of a cell to draw: images can sit between backgrounds and text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
	All,
	Backgrounds,
	Text,
}

/// Draws the rows of `view` for which `rows` is true (all rows if `None`).
/// Glyphs and images are clipped to their row, so each row can be redrawn
/// alone.
pub(crate) fn draw_grid(
	p: &mut Painter,
	fonts: &mut FontSet,
	view: &GridView,
	rows: Option<&[bool]>,
) {
	let frame = view.frame;
	let mut images: Vec<&Placement> = frame.images.iter().collect();
	// Equal z: the lower image id is lower; Sixel images count as id 0.
	images.sort_by_key(|image| (image.z, image.kitty.map_or(0, |id| id.image)));
	let wanted = |row: usize| rows.is_none_or(|rows| rows[row]);
	for row in (0..frame.rows).filter(|&row| wanted(row)) {
		extend_padding(p, view, row);
		if images.is_empty() {
			draw_row(p, fonts, view, row, Layer::All);
			draw_placeholders(p, view, row);
			continue;
		}
		let clip = row_clip(p, view, row);
		draw_images(p, view, &images, clip, |z| z < UNDER_BACKGROUNDS);
		draw_row(p, fonts, view, row, Layer::Backgrounds);
		draw_images(p, view, &images, clip, |z| {
			(UNDER_BACKGROUNDS..0).contains(&z)
		});
		draw_row(p, fonts, view, row, Layer::Text);
		draw_placeholders(p, view, row);
		draw_images(p, view, &images, clip, |z| z >= 0);
	}
}

/// Draws the image slices of the Unicode placeholder cells in `row`, where
/// their text would be.
fn draw_placeholders(p: &mut Painter, view: &GridView, row: usize) {
	let (cw, ch) = view.cell;
	let row_clip = row_clip(p, view, row);
	for cell in view.frame.placeholders_in(row) {
		let (x, y) = (view.origin.0 + cell.col * cw, view.origin.1 + row * ch);
		let clip = Rect {
			x,
			y,
			width: cw.min((row_clip.x + row_clip.width).saturating_sub(x)),
			height: row_clip.height,
		};
		// The image's top-left cell sits up and left of this one.
		let at = (
			x as isize - (cell.slot.1 * cw) as isize,
			y as isize - (cell.slot.0 * ch) as isize,
		);
		draw_placement(
			p.pixels,
			p.width,
			clip,
			&cell.placement(view.cell),
			at,
			view.cell,
		);
	}
}

fn draw_row(p: &mut Painter, fonts: &mut FontSet, view: &GridView, row: usize, layer: Layer) {
	let frame = view.frame;
	let absolute_row = frame.first_row + row;
	let cells = frame.row(row);
	let may_join = layer != Layer::Backgrounds && may_join(cells);
	// Right to left, so a wide glyph is drawn after the cell it spills into.
	for (col, cell) in cells.iter().enumerate().rev() {
		let point = CellPoint {
			row: absolute_row,
			col,
		};
		let is_cursor = frame.cursor == Some((col, row));
		let solid_block = is_cursor && view.focused && frame.cursor_shape == CursorShape::Block;
		let colors = &frame.colors;
		let (fg, bg) = if contains(view.selection, point) {
			// Dynamic selection colors are reverse video.
			let (default_fg, default_bg) = view.selection_colors;
			(
				colors.selection_foreground.or(default_fg, cell.bg),
				colors.selection_background.or(default_bg, cell.fg),
			)
		} else if contains(view.search_match, point) {
			view.match_colors
		} else if solid_block {
			(
				colors.cursor_text.or(cell.bg, cell.bg),
				colors.cursor.or(cell.fg, cell.fg),
			)
		} else if frame.guide == Some(row) {
			(cell.fg, blend(cell.bg, cell.fg, 24))
		} else {
			(cell.fg, cell.bg)
		};
		let joined = if !may_join {
			Join::Alone
		} else {
			join(
				cells,
				col,
				|cell, chars| cell_chars(frame, cell, chars),
				|chars| fonts.cluster(chars, view.px).is_some(),
			)
		};
		draw_cell(
			p,
			fonts,
			view,
			cell,
			(col, row),
			(fg, bg),
			layer,
			(&joined, cells),
		);
		if is_cursor && !solid_block && layer != Layer::Backgrounds {
			draw_cursor(p, view, cell, (col, row));
		}
	}
}

/// Appends the characters of `cell` and its marks, unless it is part of a
/// block of scaled text.
fn cell_chars(frame: &Frame, cell: &Cell, chars: &mut Vec<char>) -> bool {
	if cell.extra == NO_EXTRA {
		chars.push(cell.ch);
		return true;
	}
	if frame.multicell(cell).is_some() {
		return false;
	}
	chars.push(cell.ch);
	chars.extend_from_slice(frame.marks(cell));
	true
}

/// The pixels of grid row `row` inside the surface.
fn row_clip(p: &Painter, view: &GridView, row: usize) -> Rect {
	let (cw, ch) = view.cell;
	let (left, top) = (view.origin.0, view.origin.1 + row * ch);
	Rect {
		x: left,
		y: top,
		width: (view.frame.cols * cw).min(p.width.saturating_sub(left)),
		height: ch.min(p.height.saturating_sub(top)),
	}
}

#[allow(clippy::too_many_arguments)]
fn draw_cell(
	p: &mut Painter,
	fonts: &mut FontSet,
	view: &GridView,
	cell: &Cell,
	(col, row): (usize, usize),
	(fg, bg): (u32, u32),
	layer: Layer,
	(joined, cells): (&Join, &[Cell]),
) {
	let (cw, ch) = view.cell;
	let line = view.scale.max(1);
	let x0 = view.origin.0 + col * cw;
	let y0 = view.origin.1 + row * ch;
	if bg != view.default_bg && layer != Layer::Text {
		p.rect(x0, y0, cw, ch, bg);
	}
	if layer == Layer::Backgrounds {
		return;
	}
	if cell.attrs & (attr::UNDERLINE | attr::DOUBLE_UNDERLINE) != 0 {
		let color = match cell.extra {
			NO_EXTRA => fg,
			_ => view.frame.underline_color(cell).unwrap_or(fg),
		};
		if cell.attrs & attr::UNDERLINE != 0 {
			p.rect(x0, y0 + ch.saturating_sub(2 * view.scale), cw, line, color);
		}
		if cell.attrs & attr::DOUBLE_UNDERLINE != 0 {
			p.rect(x0, y0 + ch.saturating_sub(4 * view.scale), cw, line, color);
		}
	}
	if cell.attrs & attr::OVERLINE != 0 {
		p.rect(x0, y0 + view.scale, cw, line, fg);
	}
	if cell.attrs & attr::STRIKE != 0 {
		p.rect(x0, y0 + ch / 2, cw, line, fg);
	}
	if (cell.ch == ' ' && cell.extra == NO_EXTRA)
		|| cell.width == 0
		|| cell.ch == PLACEHOLDER
		|| *joined == Join::Second
	{
		return;
	}
	let italic = cell.attrs & attr::ITALIC != 0;
	if cell.extra != NO_EXTRA
		&& let Some(part) = view.frame.multicell(cell)
	{
		// A block's first column draws its row; the other columns only
		// have backgrounds and lines.
		if part.dx == 0 {
			draw_multicell(p, fonts, view, part, (x0, y0), italic, fg);
		}
		return;
	}
	let columns = match joined {
		Join::First { columns, .. } => *columns,
		_ => usize::from(cell.width),
	};
	let span = cw * columns;
	let (top, bottom) = (y0 as i32, (y0 + ch) as i32);
	let marks = match cell.extra {
		NO_EXTRA => &[][..],
		_ => view.frame.marks(cell),
	};
	let presentation = Presentation::of(marks);
	let cluster: Vec<char> = if let Join::First { chars, .. } = joined {
		chars.clone()
	} else if joins(marks) {
		std::iter::once(cell.ch)
			.chain(marks.iter().copied())
			.collect()
	} else {
		Vec::new()
	};
	let shape = if !cluster.is_empty() && fonts.cluster(&cluster, view.px).is_some() {
		Shape::Cluster(&cluster)
	} else {
		Shape::Char(cell.ch, presentation)
	};
	if boxdraw::covers(cell.ch) {
		draw_drawn(p, fonts, cell.ch, (x0, y0), (span, ch), fg);
	} else {
		let glyph = fonts.shaped(shape, view.px);
		let m = &glyph.metrics;
		if m.xmin >= 0 && m.xmin + m.width as i32 <= span as i32 && m.height <= ch + 1 {
			let origin = (x0 as i32 + m.xmin, glyph_top(glyph, y0, view.ascent));
			let clip = (x0 as i32, top, (x0 + span) as i32, bottom);
			p.glyph(glyph, origin, clip, italic, fg);
		} else {
			let style = (italic, fg, shape);
			let next = cells.get(col + 1);
			draw_oversized(p, fonts, view, span, (x0, y0), style, next);
		}
	}
	if cell.extra == NO_EXTRA {
		return;
	}
	if view.frame.is_link(cell) {
		p.rect(
			x0,
			y0 + ch.saturating_sub(2 * view.scale),
			span,
			line,
			view.link_color,
		);
	}
	// A joined cluster drew its marks as part of its glyph.
	let marks = if let Shape::Cluster(_) = shape {
		&[][..]
	} else {
		marks
	};
	for &mark in marks {
		// Joiners, variation selectors and tags are invisible.
		if matches!(
			mark,
			'\u{200d}' | '\u{fe0e}' | '\u{fe0f}' | '\u{e0020}'..='\u{e007f}'
		) {
			continue;
		}
		let glyph = fonts.glyph(mark, view.px);
		let x = x0 as i32 + (cw as i32 - glyph.metrics.width as i32) / 2;
		let origin = (x, glyph_top(glyph, y0, view.ascent));
		p.glyph(
			glyph,
			origin,
			(x0 as i32, top, (x0 + cw) as i32, bottom),
			false,
			fg,
		);
	}
}

/// Draws a box drawing, block, Braille or Powerline character, made to fit
/// the cell exactly.
#[inline(never)]
fn draw_drawn(
	p: &mut Painter,
	fonts: &mut FontSet,
	ch: char,
	(x0, y0): (usize, usize),
	cell: (usize, usize),
	fg: u32,
) {
	match fonts.drawn(ch, cell) {
		Some(Drawn::Rects(rects)) => {
			for &(x, y, w, h) in rects {
				p.rect(x0 + x, y0 + y, w, h, fg);
			}
		}
		Some(Drawn::Bitmap { left, top, glyph }) => {
			let (x, y) = ((x0 + left) as i32, (y0 + top) as i32);
			let clip = (
				x0 as i32,
				y0 as i32,
				(x0 + cell.0) as i32,
				(y0 + cell.1) as i32,
			);
			p.glyph(glyph, (x, y), clip, false, fg);
		}
		None => {}
	}
}

/// Draws a font glyph whose ink is wider or taller than its cell: it spills
/// into a blank neighbor, or else shrinks.
#[cold]
fn draw_oversized(
	p: &mut Painter,
	fonts: &mut FontSet,
	view: &GridView,
	span: usize,
	(x0, y0): (usize, usize),
	(italic, fg, shape): (bool, u32, Shape),
	next: Option<&Cell>,
) {
	let (cw, ch) = view.cell;
	let (top, bottom) = (y0 as i32, (y0 + ch) as i32);
	let next_blank =
		next.is_some_and(|next| next.ch == ' ' && next.extra == NO_EXTRA && next.width == 1);
	let glyph = fonts.shaped(shape, view.px);
	let metrics = glyph.metrics;
	let overflows = metrics.xmin < 0 || metrics.xmin + metrics.width as i32 > span as i32;
	let room = if overflows && next_blank && span == cw {
		span + cw
	} else {
		span
	};
	// A pixel of slack keeps ordinary letters whose ink barely pokes out
	// at full size; that pixel is clipped as before.
	let scale = fit_scale((metrics.width, metrics.height), (room + 1, ch + 1));
	let glyph = if scale < 1.0 {
		fonts.shaped(shape, view.px * scale)
	} else {
		glyph
	};
	let (left, y) = if overflows || scale < 1.0 {
		let height = glyph.metrics.height as i32;
		(
			fit_left(glyph.metrics.xmin, glyph.metrics.width, room),
			glyph_top(glyph, y0, view.ascent).clamp(top, (bottom - height).max(top)),
		)
	} else {
		(glyph.metrics.xmin, glyph_top(glyph, y0, view.ascent))
	};
	p.glyph(
		glyph,
		(x0 as i32 + left, y),
		(x0 as i32, top, (x0 + room) as i32, bottom),
		italic,
		fg,
	);
}

/// Underline, bar, or (when unfocused) hollow block cursors, drawn over the cell.
fn draw_cursor(p: &mut Painter, view: &GridView, cell: &Cell, (col, row): (usize, usize)) {
	let (cw, ch) = view.cell;
	let x = view.origin.0 + col * cw;
	let y = view.origin.1 + row * ch;
	let thick = 2 * view.scale.max(1);
	let width = cw * usize::from(cell.width.max(1));
	let color = view.frame.colors.cursor.or(cell.fg, cell.fg);
	match (view.frame.cursor_shape, view.focused) {
		(CursorShape::Bar, _) => p.rect(x, y, thick, ch, color),
		(CursorShape::Underline, _) => p.rect(x, y + ch.saturating_sub(thick), width, thick, color),
		(CursorShape::Block, _) => {
			let line = view.scale.max(1);
			p.rect(x, y, width, line, color);
			p.rect(x, y + ch.saturating_sub(line), width, line, color);
			p.rect(x, y, line, ch, color);
			p.rect(x + width.saturating_sub(line), y, line, ch, color);
		}
	}
}

/// Draws the images whose z-index is in `layer`, in order, inside `clip`.
fn draw_images(
	p: &mut Painter,
	view: &GridView,
	images: &[&Placement],
	clip: Rect,
	layer: impl Fn(i32) -> bool,
) {
	let frame = view.frame;
	let (cw, ch) = view.cell;
	let (left, top) = view.origin;
	for image in images.iter().filter(|image| layer(image.z)) {
		let at = (
			(left + image.col * cw) as isize,
			top as isize + (image.row + frame.scroll_offset as isize) * ch as isize,
		);
		draw_placement(p.pixels, p.width, clip, image, at, view.cell);
	}
}

#[cfg(test)]
#[path = "decoration_tests.rs"]
mod decoration_tests;
