//! Selective erase: DECSCA (`CSI Ps " q`) protects the characters printed
//! next, and DECSED (`CSI ? Ps J`) and DECSEL (`CSI ? Ps K`) erase all
//! others. Erased characters become spaces that keep their colors and
//! attributes, as only the characters are erased.
use std::sync::Arc;

use crate::term::{NO_EXTRA as NONE, Term};

impl Term {
	/// DECSCA: 1 protects what is printed next; 0 and 2 stop protecting.
	pub(super) fn set_protection(&mut self, mode: u16) {
		if matches!(mode, 0..=2) {
			self.protected = mode == 1;
			self.update_active_extra();
		}
	}

	/// DECSED: from the cursor down (0), up to it (1), all (2), or the
	/// scrollback (3), which cannot hold protected characters.
	pub(super) fn selective_erase_display(&mut self, mode: u16) {
		let (x, y, rows, cols) = (self.cx.min(self.cols - 1), self.cy, self.rows, self.cols);
		match mode {
			0 => {
				self.erase_unprotected(y, x, cols);
				(y + 1..rows).for_each(|row| self.erase_unprotected(row, 0, cols));
			}
			1 => {
				(0..y).for_each(|row| self.erase_unprotected(row, 0, cols));
				self.erase_unprotected(y, 0, x + 1);
			}
			2 => (0..rows).for_each(|row| self.erase_unprotected(row, 0, cols)),
			3 => self.clear_scrollback(),
			_ => return,
		}
		self.repair_wide_rows(0, rows);
	}

	/// DECSEL: the cursor line from the cursor (0), up to it (1), or all (2).
	pub(super) fn selective_erase_line(&mut self, mode: u16) {
		let (x, y, cols) = (self.cx.min(self.cols - 1), self.cy, self.cols);
		match mode {
			0 => self.erase_unprotected(y, x, cols),
			1 => self.erase_unprotected(y, 0, x + 1),
			2 => self.erase_unprotected(y, 0, cols),
			_ => return,
		}
		self.repair_wide_rows(y, y + 1);
	}

	/// Erases the characters in columns `from..to` of row `y` that are not
	/// protected. Cells past the row's used end are blank already.
	fn erase_unprotected(&mut self, y: usize, from: usize, to: usize) {
		let to = to.min(self.grid.used(y));
		let erasable = |term: &Self, x: usize| !term.extras.protected(term.grid[y][x].extra);
		for x in from..to {
			if self.has_multicells && erasable(self, x) {
				self.erase_multicell(x, y);
			}
		}
		for x in from..to {
			let extra = self.grid[y][x].extra;
			if self.extras.protected(extra) {
				continue;
			}
			// The underline color is part of the look, so it stays.
			let extra = match self.extras.underline_color(extra) {
				None => NONE,
				color => Arc::make_mut(&mut self.extras)
					.with_underline_color(NONE, color)
					.unwrap_or_default(),
			};
			let cell = &mut self.grid.row_upto(y, to)[x];
			(cell.ch, cell.width, cell.extra) = (' ', 1, extra);
		}
	}
}
