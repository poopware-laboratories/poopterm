//! Left and right margins (DECLRMM `?69`, DECSLRM `CSI Pl ; Pr s`): the
//! columns the cursor and text stay between, and scrolling the rectangle
//! they make with the top and bottom margins.
use super::Term;

impl Term {
	/// Whether left or right margins narrow the screen.
	#[inline(always)]
	pub(crate) fn has_lr_margins(&self) -> bool {
		self.left_margin != 0 || self.right_margin + 1 != self.cols
	}

	/// Whether the cursor's column is between the margins, where line feeds
	/// at the bottom margin scroll.
	#[inline(always)]
	pub(crate) fn inside_lr_margins(&self) -> bool {
		(self.left_margin..=self.right_margin).contains(&self.cx)
	}

	/// The first column the cursor may reach moving left: the left margin
	/// from at or right of it, else the screen's edge.
	pub(crate) fn line_start(&self) -> usize {
		if self.cx >= self.left_margin {
			self.left_margin
		} else {
			0
		}
	}

	/// The last column the cursor may reach moving right: the right margin
	/// from at or left of it, else the screen's edge.
	#[inline(always)]
	pub(crate) fn line_end(&self) -> usize {
		if self.cx <= self.right_margin {
			self.right_margin
		} else {
			self.cols - 1
		}
	}

	/// The column origin mode counts from.
	pub(crate) fn origin_column(&self) -> usize {
		if self.origin_mode {
			self.left_margin
		} else {
			0
		}
	}

	/// CR: to the left margin from at or right of it (always in origin
	/// mode), else to the first column.
	pub(crate) fn carriage_return(&mut self) {
		self.cx = if self.origin_mode {
			self.left_margin
		} else {
			self.line_start()
		};
	}

	/// DECSLRM, counted from 1: a left margin not left of the right one is
	/// ignored. Homes the cursor.
	pub(crate) fn set_lr_margins(&mut self, left: usize, right: usize) {
		let right = right.min(self.cols);
		if left >= right {
			return;
		}
		(self.left_margin, self.right_margin) = (left.saturating_sub(1), right - 1);
		self.wrap_pending = false;
		self.cx = self.origin_column();
		self.cy = if self.origin_mode { self.scroll_top } else { 0 };
	}

	/// Puts the margins back on the screen's edges.
	pub(crate) fn reset_lr_margins(&mut self) {
		(self.left_margin, self.right_margin) = (0, self.cols - 1);
	}

	/// Scrolls the rectangle inside all four margins up (`up`) or down by
	/// `count`, blanking what it leaves. Nothing goes to the scrollback.
	pub(crate) fn scroll_rectangle(&mut self, count: usize, up: bool) {
		let (top, bottom) = (self.scroll_top, self.scroll_bottom);
		let (left, end) = (self.left_margin, self.right_margin + 1);
		let count = count.min(bottom - top + 1);
		for y in top..=bottom {
			let base = y * self.cols;
			self.erase_images(base + left, base + end);
			self.erase_multicells_in_row(y, left, end);
		}
		let blank = self.blank();
		let rows: Vec<usize> = if up {
			(top..=bottom).collect()
		} else {
			(top..=bottom).rev().collect()
		};
		for (i, &y) in rows.iter().enumerate() {
			match rows.get(i + count) {
				Some(&from) => {
					let cells = self.grid[from][left..end].to_vec();
					self.grid.row_upto(y, end)[left..end].copy_from_slice(&cells);
				}
				None => self.grid.fill(y, left, end, blank),
			}
		}
		self.repair_wide_rows(top, bottom + 1);
	}
}

#[cfg(test)]
#[path = "margins_tests.rs"]
mod tests;
