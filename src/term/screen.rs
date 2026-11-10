//! Grid geometry: resize, scrolling, alternate screen and erasing.
use super::{Cell, Term, grid::Grid};

impl Term {
	/// Shows the live screen again after scrolling back; whether it moved.
	pub(crate) fn scroll_to_bottom(&mut self) -> bool {
		std::mem::take(&mut self.scroll_offset) != 0
	}

	pub(crate) fn resize(&mut self, cols: usize, rows: usize) {
		self.finish_sync();
		let cols = cols.max(1);
		let rows = rows.max(1);
		let blank = self.blank();
		let old_cols = self.cols;
		self.grid.resize(cols, rows, blank);
		self.tab_stops.resize_with(cols, || false);
		for (col, val) in self.tab_stops.iter_mut().enumerate().skip(old_cols) {
			*val = col > 0 && col % 8 == 0;
		}
		self.scrollback.truncate_cols(cols);
		self.cols = cols;
		self.rows = rows;
		self.wrap_pending = false;
		self.scroll_top = 0;
		self.scroll_bottom = rows.saturating_sub(1);
		if let Some((primary, cx, cy, ..)) = &mut self.primary_screen {
			primary.resize(cols, rows, blank);
			*cx = (*cx).min(cols - 1);
			*cy = (*cy).min(rows - 1);
		}
		self.cx = self.cx.min(cols - 1);
		self.cy = self.cy.min(rows - 1);
		if let Some(modes) = &mut self.primary_modes {
			modes.3 = false;
		}
		self.repair_wide_rows(0, rows);
	}

	/// Blanks orphaned halves of wide characters in rows `top..bottom`, e.g.
	/// after an erase or shift split a character from its spacer.
	pub(crate) fn repair_wide_rows(&mut self, top: usize, bottom: usize) {
		for y in top..bottom.min(self.rows) {
			// Only the used cells can be broken halves: padding is narrow.
			let used = self.grid.used(y);
			let Some(first) = (0..used).find(|&col| broken_half(&self.grid[y], col)) else {
				continue;
			};
			let row = self.grid.row_upto(y, used);
			for col in first..used {
				if broken_half(row, col) {
					row[col].ch = ' ';
					row[col].width = 1;
				}
			}
		}
	}

	pub(crate) fn scroll_view(&mut self, lines: isize) {
		if self.primary_screen.is_some() {
			return;
		}
		self.scroll_offset = if lines > 0 {
			self
				.scroll_offset
				.saturating_add(lines as usize)
				.min(self.scrollback.len())
		} else {
			self.scroll_offset.saturating_sub(lines.unsigned_abs())
		};
	}

	pub(crate) fn alternate_screen(&mut self, enabled: bool) {
		if enabled && self.primary_screen.is_none() {
			self.primary_images = std::mem::take(&mut self.images);
			self.primary_modes = Some((
				self.scroll_top,
				self.scroll_bottom,
				self.origin_mode,
				self.wrap_pending,
			));
			self.scroll_top = 0;
			self.scroll_bottom = self.rows - 1;
			self.origin_mode = false;
			self.wrap_pending = false;
			let mut alternate = Grid::new(self.cols, self.rows, self.blank());
			alternate.sync_pad(self.padding());
			self.primary_screen = Some((
				std::mem::replace(&mut self.grid, alternate),
				self.cx,
				self.cy,
				self.fg,
				self.bg,
			));
			self.cx = 0;
			self.cy = 0;
			self.scroll_offset = 0;
		} else if !enabled && let Some((grid, cx, cy, fg, bg)) = self.primary_screen.take() {
			self.images = std::mem::take(&mut self.primary_images);
			self.grid = grid;
			self.cx = cx;
			self.cy = cy;
			self.fg = fg;
			self.bg = bg;
			if let Some((top, bottom, origin, wrap)) = self.primary_modes.take() {
				self.scroll_top = top.min(self.rows - 1);
				self.scroll_bottom = bottom.min(self.rows - 1).max(self.scroll_top);
				self.origin_mode = origin;
				self.wrap_pending = wrap;
			}
			self.scroll_offset = 0;
			self.repair_wide_rows(0, self.rows);
		}
	}

	/// Erases the scrollback and the images placed in it (`CSI 3 J`).
	pub(crate) fn clear_scrollback(&mut self) {
		self.clear_kitty_placements(true);
		self.history_base = self.history_base.saturating_add(self.scrollback.len());
		self.scrollback.clear();
		self.scroll_offset = 0;
	}

	#[inline(always)]
	pub(crate) fn linefeed(&mut self) {
		if self.cy == self.scroll_bottom {
			self.scroll_region_up(1);
		} else if self.cy + 1 < self.rows {
			self.cy += 1;
		}
	}

	/// Scrolls the scroll region up. When the region is the whole primary
	/// screen, rows leaving the top move into the scrollback.
	pub(crate) fn scroll_region_up(&mut self, count: usize) {
		let (top, bottom) = (self.scroll_top, self.scroll_bottom);
		let count = count.min(bottom - top + 1);
		self.split_multicells_above(&[top, bottom + 1]);
		self.scroll_images(-(count as isize));
		let blank = self.blank();
		if top != 0 || bottom + 1 != self.rows || self.primary_screen.is_some() {
			self
				.grid
				.scroll_up(top, bottom, count, blank, |row, _| row.len());
			return;
		}
		let pad = self.padding();
		let Self {
			grid,
			scrollback,
			scrollback_limit,
			history_base,
			..
		} = self;
		grid.scroll_up(top, bottom, count, blank, |row, used| {
			if *scrollback_limit == 0 {
				*history_base = history_base.saturating_add(scrollback.len() + 1);
				scrollback.clear();
				return row.len();
			}
			let (evicted, len) = scrollback.push(row, used, pad, *scrollback_limit);
			*history_base = history_base.saturating_add(evicted);
			// Cells after the stored ones are padding, already blank when it is.
			if pad == blank { len } else { row.len() }
		});
		if self.scroll_offset > 0 {
			self.scroll_offset += count;
		}
		self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
	}

	/// Changes the history limit, discarding the oldest lines that no longer fit.
	pub(crate) fn set_scrollback_limit(&mut self, limit: usize) {
		self.scrollback_limit = limit;
		let excess = self.scrollback.len().saturating_sub(limit);
		self.scrollback.drain_front(excess);
		if limit == 0 {
			self.scrollback.clear();
		} else {
			// What `Scrollback::push` would grow to for this limit.
			self.scrollback.shrink((limit + 1) * self.cols);
		}
		self.history_base = self.history_base.saturating_add(excess);
		self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
	}

	pub(crate) fn scroll_region_down(&mut self, count: usize) {
		let (top, bottom) = (self.scroll_top, self.scroll_bottom);
		let count = count.min(bottom - top + 1);
		self.split_multicells_above(&[top, bottom + 1]);
		self.scroll_images(count as isize);
		let blank = self.blank();
		self.grid.scroll_down(top, bottom, count, blank);
	}

	/// Blanks columns `from..to` of row `y` with the current background,
	/// removing images that overlap them: [`clear_range`](Self::clear_range)
	/// for erasures that stay on one row, without dividing indices.
	pub(crate) fn clear_row(&mut self, y: usize, from: usize, to: usize) {
		let to = to.min(self.cols);
		if from >= to || y >= self.rows {
			return;
		}
		let base = y * self.cols;
		self.erase_images(base + from, base + to);
		self.erase_multicells_in_row(y, from, to);
		let blank = self.blank();
		self.grid.fill(y, from, to, blank);
	}

	/// Blanks cells in the flat range `from..to` (index `y * cols + x`) with
	/// the current background, removing images that overlap it.
	pub(crate) fn clear_range(&mut self, from: usize, to: usize) {
		self.erase_images(from, to);
		self.erase_multicells(from, to);
		let end = to.min(self.rows * self.cols);
		if from >= end {
			return;
		}
		let blank = self.blank();
		let cols = self.cols;
		let (first, last) = (from / cols, (end - 1) / cols);
		for y in first..=last {
			let start = if y == first { from % cols } else { 0 };
			let stop = if y == last {
				(end - 1) % cols + 1
			} else {
				cols
			};
			self.grid.fill(y, start, stop, blank);
		}
	}
}

/// Whether `row[col]` is half of a wide character whose other half is gone.
fn broken_half(row: &[Cell], col: usize) -> bool {
	(row[col].width == 0 && (col == 0 || row[col - 1].width != 2))
		|| (row[col].width == 2 && (col + 1 == row.len() || row[col + 1].width != 0))
}
