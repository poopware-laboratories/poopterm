//! kitty's text sizing protocol (OSC 66): blocks of cells holding scaled
//! text, placing them, and erasing the blocks other output cuts apart.
use std::sync::Arc;

use super::{NO_EXTRA, Term};

/// Longest text one block holds, in bytes.
pub(crate) const MAX_TEXT: usize = 4096;

/// How a block's text is sized: the `s`, `w`, `n`, `d`, `v` and `h` keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Size {
	/// Rows the block spans, and columns per cell of text width (1–7).
	pub scale: u8,
	/// The text's width in cells before scaling (1–7); 0 while parsing
	/// means each character takes its own width.
	pub width: u8,
	/// Text is drawn at `numerator / denominator` of the scaled size when
	/// both are set.
	pub numerator: u8,
	pub denominator: u8,
	/// Where smaller text sits: 0 top, 1 bottom, 2 centered.
	pub vertical: u8,
	/// 0 left, 1 right, 2 centered.
	pub horizontal: u8,
}

impl Default for Size {
	fn default() -> Self {
		Self {
			scale: 1,
			width: 0,
			numerator: 0,
			denominator: 0,
			vertical: 0,
			horizontal: 0,
		}
	}
}

impl Size {
	/// Parses `key=value` pairs separated by colons. Unknown keys are
	/// ignored; a bad value rejects the whole sequence.
	pub(crate) fn parse(metadata: &[u8]) -> Option<Self> {
		let mut size = Self::default();
		for pair in metadata
			.split(|&byte| byte == b':')
			.filter(|pair| !pair.is_empty())
		{
			let (key, value) = pair.split_at(pair.iter().position(|&byte| byte == b'=')?);
			let (field, max) = match key {
				b"s" => (&mut size.scale, 7),
				b"w" => (&mut size.width, 7),
				b"n" => (&mut size.numerator, 15),
				b"d" => (&mut size.denominator, 15),
				b"v" => (&mut size.vertical, 2),
				b"h" => (&mut size.horizontal, 2),
				_ => continue,
			};
			let value = std::str::from_utf8(&value[1..]).ok()?.parse().ok()?;
			*field = Some(value).filter(|&value| value <= max)?;
		}
		let fraction = size.denominator == 0 || size.numerator < size.denominator;
		(size.scale >= 1 && fraction).then_some(size)
	}

	/// The share of the scaled size the text is drawn at.
	pub(crate) fn fraction(&self) -> f32 {
		if self.numerator == 0 || self.denominator == 0 {
			1.0
		} else {
			f32::from(self.numerator) / f32::from(self.denominator)
		}
	}
}

/// One cell of a block: the block's text and size, and where the cell lies
/// in it. Every cell carries its own copy, so rows stand alone when drawn,
/// scrolled or copied.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Multicell {
	pub text: Box<str>,
	pub size: Size,
	/// The block's width in cells: `scale * width`.
	pub columns: u8,
	pub dx: u8,
	pub dy: u8,
}

impl Multicell {
	fn same_block(&self, other: &Self, (dx, dy): (usize, usize)) -> bool {
		self.text == other.text
			&& self.size == other.size
			&& self.columns == other.columns
			&& (usize::from(other.dx), usize::from(other.dy)) == (dx, dy)
	}
}

impl Term {
	/// The block part in the cell at `(x, y)`, if any.
	pub(crate) fn multicell_at(&self, x: usize, y: usize) -> Option<&Multicell> {
		let extra = self.grid[y][x].extra;
		if extra == NO_EXTRA {
			return None;
		}
		self.extras.multicell(extra)
	}

	/// Draws `text` as one block at the cursor, `size.width` cells wide
	/// before scaling. Blocks larger than the screen are discarded.
	pub(crate) fn print_multicell(&mut self, text: &str, size: Size) {
		let rows = usize::from(size.scale);
		let columns = rows * usize::from(size.width);
		if columns == 0 || columns > self.cols || rows > self.rows {
			return;
		}
		if self.wrap_pending || self.cx + columns > self.cols {
			if self.auto_wrap {
				self.cx = 0;
				self.linefeed();
			} else {
				self.cx = self.cols - columns;
			}
		}
		self.wrap_pending = false;
		self.skip_lower_rows(columns);
		// A block reaching below the scroll region scrolls it up.
		let below = (self.cy + rows - 1).saturating_sub(self.scroll_bottom);
		if below > 0
			&& (self.scroll_top..=self.scroll_bottom).contains(&self.cy)
			&& rows <= self.scroll_bottom - self.scroll_top + 1
		{
			self.scroll_region_up(below);
			self.cy -= below;
		}
		if self.cy + rows > self.rows {
			return;
		}
		let (x, y) = (self.cx, self.cy);
		let template = self.styled_cell();
		let extras = Arc::make_mut(&mut self.extras);
		let mut ids = Vec::with_capacity(columns * rows);
		for dy in 0..rows {
			for dx in 0..columns {
				let part = Multicell {
					text: text.into(),
					size,
					columns: columns as u8,
					dx: dx as u8,
					dy: dy as u8,
				};
				// A full table leaves the text out.
				let Some(id) = extras.with_multicell(template.extra, Some(part)) else {
					return;
				};
				ids.push(id);
			}
		}
		let blank = self.blank();
		for row in y..y + rows {
			self.erase_multicells_in_row(row, x, x + columns);
			let start = self.idx(x, row);
			self.erase_images(start, start + columns);
			// Wide characters cut by the block's edges lose their other half.
			let cells = &mut self.grid[row];
			if cells[x].width == 0 && x > 0 {
				cells[x - 1] = blank;
			}
			if cells[x + columns - 1].width == 2 && x + columns < self.cols {
				cells[x + columns] = blank;
			}
		}
		let first = text.chars().next().unwrap_or(' ');
		let mut ids = ids.into_iter();
		for row in y..y + rows {
			for (col, cell) in self.grid[row][x..x + columns].iter_mut().enumerate() {
				*cell = super::Cell {
					ch: if (col, row) == (0, y) { first } else { ' ' },
					width: 1,
					extra: ids.next().unwrap_or(NO_EXTRA),
					..template
				};
			}
		}
		self.wrap_pending = x + columns >= self.cols;
		self.cx = (x + columns).min(self.cols - 1);
		self.has_multicells = true;
		self.last_printed = None;
	}

	/// Moves the cursor past the lower rows of blocks it is on, wrapping as
	/// text `width` cells wide would (kitty's last overwriting rule).
	pub(crate) fn skip_lower_rows(&mut self, width: usize) {
		for _ in 0..self.rows * self.cols {
			let Some(part) = self
				.multicell_at(self.cx, self.cy)
				.filter(|part| part.dy > 0)
			else {
				return;
			};
			let end = self.cx.saturating_sub(part.dx.into()) + usize::from(part.columns);
			if end + width <= self.cols {
				self.cx = end;
			} else if self.auto_wrap {
				self.cx = 0;
				self.linefeed();
			} else {
				// Nowhere to go: the text overwrites the block.
				return;
			}
		}
	}

	/// The cells of the block holding `(x, y)` that are still in place.
	fn block_cells(&self, x: usize, y: usize) -> Vec<(usize, usize)> {
		let Some(part) = self.multicell_at(x, y) else {
			return Vec::new();
		};
		let left = x.saturating_sub(part.dx.into());
		let top = y.saturating_sub(part.dy.into());
		let right = (left + usize::from(part.columns)).min(self.cols);
		let bottom = (top + usize::from(part.size.scale)).min(self.rows);
		(top..bottom)
			.flat_map(|row| (left..right).map(move |col| (col, row)))
			.filter(|&(col, row)| {
				self
					.multicell_at(col, row)
					.is_some_and(|other| part.same_block(other, (col - left, row - top)))
			})
			.collect()
	}

	/// Blanks the whole block holding the cell at `(x, y)`, if any.
	pub(crate) fn erase_multicell(&mut self, x: usize, y: usize) {
		let blank = self.blank();
		for (col, row) in self.block_cells(x, y) {
			self.grid[row][col] = blank;
		}
	}

	/// Erases blocks with a cell in columns `from..to` of `row`.
	pub(crate) fn erase_multicells_in_row(&mut self, row: usize, from: usize, to: usize) {
		if self.has_multicells {
			for x in from..to.min(self.cols) {
				self.erase_multicell(x, row);
			}
		}
	}

	/// Erases blocks with a cell in the flat range `from..to`.
	pub(crate) fn erase_multicells(&mut self, from: usize, to: usize) {
		let end = to.min(self.rows * self.cols);
		if self.has_multicells && from < end {
			let cols = self.cols;
			for y in from / cols..=(end - 1) / cols {
				let start = (y * cols).max(from) - y * cols;
				let stop = ((y + 1) * cols).min(end) - y * cols;
				self.erase_multicells_in_row(y, start, stop);
			}
		}
	}

	/// Before cells from column `x` of `row` shift by `count` (ICH, DCH,
	/// insert mode): erases blocks spanning several rows there, and blocks
	/// the shift would split, across `x` or the edge cells cross.
	pub(crate) fn erase_multicells_for_shift(
		&mut self,
		x: usize,
		row: usize,
		count: usize,
		insert: bool,
	) {
		if !self.has_multicells {
			return;
		}
		for col in x..self.cols {
			if self
				.multicell_at(col, row)
				.is_some_and(|part| part.size.scale > 1)
			{
				self.erase_multicell(col, row);
			}
		}
		let edge = if insert {
			self.cols.saturating_sub(count)
		} else {
			x + count
		};
		for col in [x, edge] {
			if col < self.cols && self.multicell_at(col, row).is_some_and(|part| part.dx > 0) {
				self.erase_multicell(col, row);
			}
		}
	}

	/// Erases blocks crossing the line above each of `rows`, before lines
	/// move apart there (IL, DL, scrolling a region).
	pub(crate) fn split_multicells_above(&mut self, rows: &[usize]) {
		if !self.has_multicells {
			return;
		}
		let height = self.rows;
		for &row in rows.iter().filter(|&&row| row > 0 && row < height) {
			for col in 0..self.cols {
				if self.multicell_at(col, row).is_some_and(|part| part.dy > 0) {
					self.erase_multicell(col, row);
				}
			}
		}
	}

	/// Adds `c` to the text of the block holding `(x, y)`, as a combining
	/// character joins it.
	pub(crate) fn extend_multicell(&mut self, x: usize, y: usize, c: char) {
		let Some(part) = self.multicell_at(x, y) else {
			return;
		};
		if part.text.len() + c.len_utf8() > MAX_TEXT {
			return;
		}
		let text: Box<str> = format!("{}{c}", part.text).into();
		for (col, row) in self.block_cells(x, y) {
			let extra = self.grid[row][col].extra;
			let Some(part) = self.extras.multicell(extra).cloned() else {
				continue;
			};
			let part = Multicell {
				text: text.clone(),
				..part
			};
			if let Some(id) = Arc::make_mut(&mut self.extras).with_multicell(extra, Some(part)) {
				self.grid[row][col].extra = id;
			}
		}
	}
}
