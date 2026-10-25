//! Printable characters and C0/C1 control execution.
use std::sync::Arc;

use super::{cluster::join_marks, narrow};
use crate::{
	graphics::kitty::PLACEHOLDER,
	term::{Cell, NO_EXTRA, Term, attr},
};

impl Term {
	#[inline(always)]
	pub(super) fn apply_print(&mut self, c: char) {
		let c = if self.charsets[self.active_charset] {
			dec_graphic(c)
		} else {
			c
		};
		let width = crate::width::width(c).min(self.cols);
		let mut template = self.styled_cell();
		if c == PLACEHOLDER {
			// Kitty Unicode placeholders keep the image id as the client set it.
			template.fg = self.fg_id;
		}
		self.print_styled(c, width, template);
	}

	#[inline(always)]
	fn print_styled(&mut self, c: char, width: usize, template: Cell) {
		if self.attach_to_previous(c, width) || width == 0 {
			return;
		}
		if self.wrap_pending || self.cx + width > self.cols {
			if self.auto_wrap {
				self.cx = 0;
				self.linefeed();
			} else {
				self.cx = self.cols.saturating_sub(width);
			}
		}
		self.wrap_pending = false;
		if self.has_multicells {
			// Lower rows of scaled text are skipped; text over a top row
			// erases the whole block.
			self.skip_lower_rows(width);
			self.erase_multicells_in_row(self.cy, self.cx, self.cx + width);
		}
		let (x, y, cols) = (self.cx, self.cy, self.cols);
		let i = self.idx(x, y);
		if self.insert_mode {
			let count = width.min(cols - x);
			self.erase_multicells_for_shift(x, y, count, true);
			self.grid[y].copy_within(x..cols - count, x + count);
			self.clear_range(i, i + count);
			self.repair_wide_rows(y, y + 1);
		}
		self.erase_images(i, i + width);
		let blank = self.blank();
		// Other halves of wide characters are never padding, so only the new
		// cells can extend the row.
		let row = self.grid.row_upto(y, x + width);
		// Remove both halves when overwriting a wide character.
		if row[x].width == 0 && x > 0 {
			row[x - 1] = blank;
		}
		if row[x].width == 2 && x + 1 < cols {
			row[x + 1] = blank;
		}
		if width == 2 && x + 2 < cols && row[x + 1].width == 2 {
			row[x + 2] = blank;
		}
		row[x] = Cell {
			ch: c,
			width: width as u8,
			..template
		};
		if width == 2 && x + 1 < cols {
			row[x + 1] = Cell {
				width: 0,
				..template
			};
		}
		self.wrap_pending = x + width >= cols;
		self.cx = (x + width).min(cols - 1);
		self.last_printed = Some(c);
	}

	/// Prints text without C0 or C1 controls with one style lookup. Equivalent
	/// to calling [`Term::apply_print`] per character.
	pub(super) fn print_text(&mut self, text: &str) {
		#[cfg(target_arch = "x86_64")]
		if crate::cpu::has_v3() {
			// SAFETY: the CPU supports the features.
			return unsafe { self.print_text_v3(text) };
		}
		self.print_text_with(text);
	}

	/// # Safety
	///
	/// The CPU must have the features [`crate::cpu::has_v3`] checks.
	#[cfg(target_arch = "x86_64")]
	#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,fma")]
	pub(super) unsafe fn print_text_v3(&mut self, text: &str) {
		self.print_text_with(text);
	}

	/// The body of [`Term::print_text`], compiled into each variant.
	#[inline(always)]
	pub(super) fn print_text_with(&mut self, text: &str) {
		let template = self.styled_cell();
		let blank = self.blank();
		let general = self.charsets[self.active_charset]
			|| self.insert_mode
			|| self.has_multicells
			|| self.has_sixel_images();
		let mut chars = text.chars();
		while let Some(c) = chars.next() {
			debug_assert!(c >= ' ' && !('\u{80}'..='\u{9f}').contains(&c));
			let width = crate::width::width(c).min(self.cols);
			if general || c == PLACEHOLDER {
				self.apply_print(c);
				continue;
			}
			if width == 0 || self.wrap_pending || self.cx + width > self.cols {
				self.print_styled(c, width, template);
				continue;
			}
			if self.attach_to_previous(c, width) {
				continue;
			}
			// Resolve the row and style once for the whole segment. Zero-width
			// marks and margin handling return to the general path above.
			let cols = self.cols;
			let mut x = self.cx;
			let y = self.cy;
			// Marked as used once the segment's end is known.
			let row = &mut self.grid.row_unmarked(y)[..cols];
			// Cells inside the segment are all rewritten; only wide characters
			// cut by its edges need repair.
			if row[x].width == 0 && x > 0 {
				row[x - 1] = blank;
			}
			// Narrow characters in a row; the fast path is tried after a few, so
			// text with wide characters rarely pays for misses.
			let mut narrow_run = 0;
			let (mut c, mut width) = (c, width);
			// Cells are built in vector registers and stored whole: `spacer`
			// is the template with width 0, the right half of wide characters.
			let spacer = Cell {
				width: 0,
				..template
			}
			.words();
			let base = Cell::without_char(spacer);
			// Built once, so the lane holding the width is not rebuilt per character.
			let (narrow, wide) = (Cell::with_width(base, 1), Cell::with_width(base, 2));
			// SAFETY: every store below is `spacer`, a whole cell, or
			// `with_char` of a `char`; `narrow::print_chunk` stores decoded
			// characters only.
			let cells = unsafe { Cell::words_mut(row) };
			loop {
				cells[x] = Cell::with_char(if width == 2 { wide } else { narrow }, c as u32);
				if width == 2 {
					cells[x + 1] = spacer;
				}
				x += width;
				narrow_run = if width == 1 { narrow_run + 1 } else { 0 };
				while narrow_run >= narrow::RUN
					&& let Some(chunk) = chars.as_str().as_bytes().first_chunk()
				{
					let Some((len, used, last)) = narrow::print_chunk(chunk, &mut cells[x..], narrow) else {
						narrow_run = 0;
						break;
					};
					chars = chars.as_str()[len..].chars();
					(x, c) = (x + used, last);
				}
				// Peek: a character that does not fit goes to the general path.
				let before = chars.clone();
				let Some(next) = chars.next() else {
					break;
				};
				let next_width = crate::width::width(next).min(cols);
				if next_width == 0 || x + next_width > cols || next == PLACEHOLDER {
					// Marks join the character just written, and the run goes on
					// after them without setting up the segment again.
					let rest = before.as_str();
					let (used, resumed) = join_marks(&mut self.extras, &mut cells[x - width], rest, cols - x);
					chars = rest[used..].chars();
					let Some(resumed) = resumed else {
						break;
					};
					(c, width) = resumed;
					continue;
				}
				(c, width) = (next, next_width);
			}
			if x < cols && row[x].width == 0 {
				row[x] = blank;
			}
			self.grid.mark_used(y, x);
			self.last_printed = Some(c);
			self.wrap_pending = x >= cols;
			self.cx = x.min(cols - 1);
		}
	}

	/// Prints a run of printable ASCII (`0x20..=0x7e`). Equivalent to calling
	/// [`Term::apply_print`] per byte, but resolves the cell style once and
	/// fills each row segment in one pass.
	#[inline(always)]
	pub(crate) fn print_ascii(&mut self, mut bytes: &[u8]) {
		if self.charsets[self.active_charset] || self.insert_mode || self.has_multicells {
			for &byte in bytes {
				self.apply_print(byte as char);
			}
			return;
		}
		// Only the first character can join a cluster (after a ZWJ).
		if let Some((&first, rest)) = bytes.split_first()
			&& self.attach_to_previous(first as char, 1)
		{
			bytes = rest;
		}
		let template = self.styled_cell();
		while !bytes.is_empty() {
			if self.wrap_pending {
				if self.auto_wrap {
					self.cx = 0;
					self.linefeed();
				} else {
					// Without wrap, each character overwrites the last column.
					bytes = &bytes[bytes.len() - 1..];
				}
			}
			self.wrap_pending = false;
			let (x, y, cols) = (self.cx, self.cy, self.cols);
			let count = (cols - x).min(bytes.len());
			if self.has_sixel_images() {
				let i = self.idx(x, y);
				self.erase_images(i, i + count);
			}
			let blank = self.blank();
			// The other halves are never padding: only the run extends the row.
			let row = self.grid.row_upto(y, x + count);
			// Remove the other half of wide characters cut by the run's edges.
			if row[x].width == 0 && x > 0 {
				row[x - 1] = blank;
			}
			if row[x + count - 1].width == 2 && x + count < cols {
				row[x + count] = blank;
			}
			for (cell, &byte) in row[x..x + count].iter_mut().zip(bytes) {
				*cell = Cell {
					ch: byte as char,
					..template
				};
			}
			self.last_printed = Some(bytes[count - 1] as char);
			bytes = &bytes[count..];
			self.wrap_pending = x + count >= cols;
			self.cx = (x + count).min(cols - 1);
		}
	}

	/// A blank cell carrying the current colors and attributes, with bold
	/// brightening, reverse video, dim and conceal already applied.
	#[inline]
	pub(crate) fn styled_cell(&self) -> Cell {
		let mut fg = self.fg;
		let mut bg = self.bg;
		if self.bold
			&& let Some(index) = (0..8).find(|&index| self.indexed_color(index as u8) == fg)
		{
			fg = self.indexed_color((index + 8) as u8);
		}
		if self.reverse {
			std::mem::swap(&mut fg, &mut bg);
		}
		if self.attrs & attr::DIM != 0 {
			fg = crate::render::blend::blend(bg, fg, 160);
		}
		if self.attrs & attr::CONCEAL != 0 {
			fg = bg;
		}
		Cell {
			attrs: self.attrs,
			fg,
			bg,
			extra: self.active_extra,
			..Cell::default()
		}
	}

	/// Appends zero-width marks (and anything after a ZWJ) to the previous
	/// cell's cluster. Returns true when `c` was consumed that way.
	#[inline]
	fn attach_to_previous(&mut self, c: char, width: usize) -> bool {
		let mut x = if self.wrap_pending {
			self.cx
		} else if self.cx > 0 {
			self.cx - 1
		} else {
			return false;
		};
		let row = &self.grid[self.cy];
		if row[x].width == 0 {
			x = x.saturating_sub(1);
		}
		let extra = row[x].extra;
		if self.has_multicells
			&& let Some(part) = self.extras.multicell(extra)
		{
			if width != 0 && !part.text.ends_with('\u{200d}') {
				return false;
			}
			self.extend_multicell(x, self.cy, c);
			return true;
		}
		// Fast path: most cells have no marks, so nothing can join them.
		if width != 0 && (extra == NO_EXTRA || self.extras.marks(extra).last() != Some(&'\u{200d}')) {
			return false;
		}
		self.join_mark(x, extra, c);
		true
	}

	/// Adds `c` to the marks of cell `x` in the cursor row, which hold `extra`.
	/// Kept out of line so the checks above stay small enough to inline.
	#[cold]
	#[inline(never)]
	fn join_mark(&mut self, x: usize, extra: u16, c: char) {
		// Repeated marks are cached; only a miss needs the table unique.
		let id = match self.extras.remembered_mark(extra, c) {
			None => Arc::make_mut(&mut self.extras).with_mark(extra, c),
			hit => hit,
		};
		if let Some(id) = id {
			self.grid.row_upto(self.cy, x + 1)[x].extra = id;
		}
	}

	#[inline]
	pub(super) fn apply_execute(&mut self, byte: u8) {
		if matches!(byte, 8..=13) {
			self.wrap_pending = false;
		}
		match byte {
			0x0e => self.active_charset = 1,
			0x0f => self.active_charset = 0,
			7 => self.bell = true,
			0x84 => self.linefeed(),
			0x85 => {
				self.cx = 0;
				self.linefeed();
			}
			0x88 => self.tab_stops[self.cx] = true,
			0x8d => {
				if self.cy == self.scroll_top {
					self.scroll_region_down(1);
				} else {
					self.cy = self.cy.saturating_sub(1);
				}
			}
			b'\n' | 0x0b | 0x0c => {
				// LNM: line feeds also return the carriage.
				if self.newline_mode {
					self.cx = 0;
				}
				self.linefeed();
			}
			b'\r' => self.cx = 0,
			0x08 => self.backspace(),
			b'\t' => {
				self.cx = (self.cx + 1..self.cols)
					.find(|&col| self.tab_stops[col])
					.unwrap_or(self.cols - 1)
			}
			_ => {}
		}
	}
}

impl Term {
	/// BS; with reverse wraparound the first column leads to the end of the
	/// line above, stopping at the top margin as cursor up does.
	fn backspace(&mut self) {
		let top = if self.cy >= self.scroll_top {
			self.scroll_top
		} else {
			0
		};
		if self.cx == 0 && self.reverse_wrap && self.auto_wrap && self.cy > top {
			self.cy -= 1;
			self.cx = self.cols - 1;
		} else {
			self.cx = self.cx.saturating_sub(1);
		}
	}
}

/// Maps ASCII to the DEC Special Graphics (line drawing) character set.
pub(crate) fn dec_graphic(c: char) -> char {
	match c {
		'_' => ' ',
		'`' => '◆',
		'a' => '▒',
		'b' => '␉',
		'c' => '␌',
		'd' => '␍',
		'e' => '␊',
		'f' => '°',
		'g' => '±',
		'h' => '␤',
		'i' => '␋',
		'j' => '┘',
		'k' => '┐',
		'l' => '┌',
		'm' => '└',
		'n' => '┼',
		'o' => '⎺',
		'p' => '⎻',
		'q' => '─',
		'r' => '⎼',
		's' => '⎽',
		't' => '├',
		'u' => '┤',
		'v' => '┴',
		'w' => '┬',
		'x' => '│',
		'y' => '≤',
		'z' => '≥',
		'{' => 'π',
		'|' => '≠',
		'}' => '£',
		'~' => '·',
		_ => c,
	}
}

#[cfg(test)]
#[path = "print_tests.rs"]
mod tests;
