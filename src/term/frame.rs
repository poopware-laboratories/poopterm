//! A copy of the visible screen, taken under the terminal lock so frames
//! render without blocking the parser thread.
use std::collections::HashMap;

use super::{Cell, CursorShape, NO_EXTRA, Term, multicell::Multicell};
use crate::graphics::{
	PlaceholderCell, Placement,
	kitty::{PLACEHOLDER, PlaceholderInput, resolve_row},
};

/// What a frame keeps of a cell's extras.
#[derive(Clone, Default)]
struct FrameExtra {
	link: bool,
	marks: Box<[char]>,
	multicell: Option<Multicell>,
	underline: Option<u32>,
}

/// The visible rows of a [`Term`] plus what the renderer needs to draw them.
/// Buffers are reused between frames.
#[derive(Clone, Default)]
pub(crate) struct Frame {
	pub cols: usize,
	pub rows: usize,
	/// `rows * cols` cells, row-major.
	cells: Vec<Cell>,
	/// Row index (as used by [`Term::row_cells`] and selections) of the top row.
	pub first_row: usize,
	/// A number that grows by one each time a line scrolls into history;
	/// equal values in two frames mean the same line is at the top.
	pub top_line: usize,
	/// Cursor cell, when visible on screen.
	pub cursor: Option<(usize, usize)>,
	pub cursor_shape: CursorShape,
	/// The row iTerm2's cursor guide highlights.
	pub guide: Option<usize>,
	/// Cursor and selection colors programs set (OSC 12, 17, 19, 21), or else
	/// the theme's.
	pub colors: super::Colors,
	pub scroll_offset: usize,
	/// Cell size images were laid out with.
	pub cell: (usize, usize),
	/// Images at least partly on screen.
	pub images: Vec<Placement>,
	/// Cells showing kitty images through Unicode placeholders, in row order.
	pub placeholders: Vec<PlaceholderCell>,
	/// Frame-local extras: `cell.extra` indexes this; 0 means none.
	extras: Vec<FrameExtra>,
}

impl Frame {
	/// The cells of visible row `row`.
	pub(crate) fn row(&self, row: usize) -> &[Cell] {
		&self.cells[row * self.cols..][..self.cols]
	}

	/// Combining marks attached to `cell`.
	pub(crate) fn marks(&self, cell: &Cell) -> &[char] {
		self
			.extras
			.get(cell.extra as usize)
			.map_or(&[], |extra| &extra.marks)
	}

	/// The part of a block of scaled text `cell` holds, if any.
	pub(crate) fn multicell(&self, cell: &Cell) -> Option<&Multicell> {
		self.extras.get(cell.extra as usize)?.multicell.as_ref()
	}

	/// The SGR 58 color `cell`'s underlines are drawn in, if not its text's.
	pub(crate) fn underline_color(&self, cell: &Cell) -> Option<u32> {
		self.extras.get(cell.extra as usize)?.underline
	}

	/// Whether row `row` looks the same as `other`'s row `other_row`: same
	/// cells, marks, links and underline colors. Extra ids are frame-local, so they are
	/// compared by content.
	pub(crate) fn row_matches(&self, row: usize, other: &Frame, other_row: usize) -> bool {
		let (mine, theirs) = (self.row(row), other.row(other_row));
		mine.iter().zip(theirs).all(|(a, b)| {
			let same_cell = Cell { extra: 0, ..*a } == Cell { extra: 0, ..*b };
			same_cell
				&& (a.extra == NO_EXTRA) == (b.extra == NO_EXTRA)
				&& (a.extra == NO_EXTRA
					|| (self.marks(a) == other.marks(b)
						&& self.is_link(a) == other.is_link(b)
						&& self.multicell(a) == other.multicell(b)
						&& self.underline_color(a) == other.underline_color(b)))
		})
	}

	/// The placeholder cells of visible row `row`.
	pub(crate) fn placeholders_in(&self, row: usize) -> &[PlaceholderCell] {
		let start = self.placeholders.partition_point(|cell| cell.row < row);
		let end = self.placeholders.partition_point(|cell| cell.row <= row);
		&self.placeholders[start..end]
	}

	/// Whether `cell` is part of an OSC 8 hyperlink.
	pub(crate) fn is_link(&self, cell: &Cell) -> bool {
		self
			.extras
			.get(cell.extra as usize)
			.is_some_and(|extra| extra.link)
	}
}

impl Term {
	/// Copies the visible screen into `frame`, or the screen synchronized
	/// output froze.
	pub(crate) fn snapshot(&self, frame: &mut Frame) {
		if let Some(frozen) = self.frozen_frame() {
			frame.clone_from(frozen);
			return;
		}
		let (cols, rows) = (self.cols, self.rows);
		let first_row = self.scrollback.len().saturating_sub(self.scroll_offset);
		frame.cols = cols;
		frame.rows = rows;
		frame.first_row = first_row;
		frame.top_line = self.history_base + first_row;
		frame.scroll_offset = self.scroll_offset;
		frame.cursor_shape = self.cursor_shape;
		frame.guide =
			Some(self.cy + self.scroll_offset).filter(|&row| self.iterm.cursor_guide && row < rows);
		frame.colors = self.drawn_colors();
		frame.cursor = (self.cursor_visible && self.scroll_offset == 0 && self.cx < cols)
			.then_some((self.cx, self.cy));
		let cell = self.cell_size();
		frame.cell = cell;
		frame.images.clear();
		frame.images.extend(
			self
				.images
				.iter()
				.filter(|image| {
					let top = image.row + self.scroll_offset as isize;
					top < rows as isize && top + image.cells(cell).1 as isize > 0
				})
				.cloned(),
		);

		frame.cells.clear();
		// History rows are stored without their trailing default blanks.
		let blank = self.padding();
		for row in first_row..first_row + rows {
			let cells = self.row_cells(row).unwrap_or(&[]);
			let cells = &cells[..cells.len().min(cols)];
			frame.cells.extend_from_slice(cells);
			frame
				.cells
				.resize(frame.cells.len() + cols - cells.len(), blank);
		}
		if self.reverse_screen {
			for cell in &mut frame.cells {
				std::mem::swap(&mut cell.fg, &mut cell.bg);
			}
		}

		// Copy the extras visible cells use instead of sharing the table: the
		// parser updates it copy-on-write and would otherwise clone it.
		frame.extras.clear();
		frame.extras.push(FrameExtra::default());
		let mut local = HashMap::new();
		for cell in frame.cells.iter_mut().filter(|cell| cell.extra != NO_EXTRA) {
			cell.extra = *local.entry(cell.extra).or_insert_with(|| {
				frame.extras.push(FrameExtra {
					link: self.extras.hyperlink(cell.extra) != 0,
					marks: self.extras.marks(cell.extra).into(),
					multicell: self.extras.multicell(cell.extra).cloned(),
					underline: self.extras.underline_color(cell.extra),
				});
				(frame.extras.len() - 1) as u16
			});
		}

		frame.placeholders.clear();
		if self.kitty_images.has_virtuals() {
			self.resolve_placeholders(frame);
		}
	}

	/// Finds the virtual placement and image cell each placeholder cell
	/// shows; cells naming no virtual placement, or outside it, show nothing.
	fn resolve_placeholders(&self, frame: &mut Frame) {
		let mut placeholders = std::mem::take(&mut frame.placeholders);
		for row in 0..frame.rows {
			let cells = frame.row(row);
			if !cells.iter().any(|cell| cell.ch == PLACEHOLDER) {
				continue;
			}
			let inputs = cells.iter().map(|cell| PlaceholderInput {
				ch: cell.ch,
				color: cell.fg,
				marks: frame.marks(cell),
			});
			for (col, slot) in resolve_row(inputs).into_iter().enumerate() {
				let Some(slot) = slot else {
					continue;
				};
				let Some((bitmap, cells)) = self.kitty_images.virtual_placement(slot.image) else {
					continue;
				};
				let slot = (slot.row as usize, slot.col as usize);
				if slot.0 < cells.1 && slot.1 < cells.0 {
					placeholders.push(PlaceholderCell {
						row,
						col,
						slot,
						bitmap: bitmap.clone(),
						cells,
					});
				}
			}
		}
		frame.placeholders = placeholders;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		parser::Parser,
		theme::{Theme, ThemeKind},
	};

	fn term(cols: usize, rows: usize, input: &str) -> Term {
		let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
		Parser::new().advance(&mut term, input.as_bytes());
		term
	}

	fn text(frame: &Frame, row: usize) -> String {
		frame.row(row).iter().map(|cell| cell.ch).collect()
	}

	#[test]
	fn the_visible_rows_and_cursor_are_copied() {
		let term = term(4, 2, "ab\r\ncd");
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		assert_eq!((frame.cols, frame.rows), (4, 2));
		assert_eq!(text(&frame, 0), "ab  ");
		assert_eq!(text(&frame, 1), "cd  ");
		assert_eq!(frame.cursor, Some((2, 1)));
	}

	#[test]
	fn scrolling_back_shows_history_and_hides_the_cursor() {
		let mut term = term(3, 2, "1\r\n2\r\n3\r\n4");
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		assert_eq!(frame.first_row, 2);
		assert_eq!(text(&frame, 0), "3  ");
		term.scroll_view(1);
		term.snapshot(&mut frame);
		assert_eq!(frame.first_row, 1);
		assert_eq!(text(&frame, 0), "2  ");
		assert_eq!(text(&frame, 1), "3  ");
		assert_eq!(frame.cursor, None);
	}

	#[test]
	fn top_line_counts_lines_scrolled_into_history() {
		let mut term = term(3, 2, "1\r\n2");
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		let before = frame.top_line;
		term.set_scrollback_limit(1);
		Parser::new().advance(&mut term, b"\r\n3\r\n4");
		term.snapshot(&mut frame);
		assert_eq!(frame.top_line, before + 2, "counts evicted lines too");
		assert_eq!(text(&frame, 0), "3  ");
	}

	#[test]
	fn marks_and_links_survive_without_sharing_the_extras_table() {
		let term = term(4, 1, "e\u{301}\x1b]8;;https://x\x1b\\L\x1b]8;;\x1b\\");
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		let row = frame.row(0);
		assert_eq!(frame.marks(&row[0]), ['\u{301}']);
		assert!(!frame.is_link(&row[0]));
		assert!(frame.is_link(&row[1]));
		assert!(frame.marks(&row[2]).is_empty() && !frame.is_link(&row[2]));
		assert_eq!(std::sync::Arc::strong_count(&term.extras), 1);
	}

	#[test]
	fn rows_with_different_scaled_text_differ() {
		let snapshot = |text| {
			let mut frame = Frame::default();
			term(4, 1, text).snapshot(&mut frame);
			frame
		};
		let (ab, ac) = (
			snapshot("\x1b]66;w=1;ab\x07"),
			snapshot("\x1b]66;w=1;ac\x07"),
		);
		assert_eq!(
			ab.multicell(&ab.row(0)[0]).map(|part| &*part.text),
			Some("ab")
		);
		assert!(ab.row_matches(0, &snapshot("\x1b]66;w=1;ab\x07"), 0));
		assert!(!ab.row_matches(0, &ac, 0));
	}
}
