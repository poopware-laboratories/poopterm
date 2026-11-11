//! Scrollback history stored as trimmed rows in one ring buffer.
//!
//! Rows lose their trailing default blanks, so typical output takes a
//! fraction of a full row, and new rows are copied into recently written
//! memory instead of recycling the coldest row in the history.
use std::{collections::VecDeque, ops::Index};

use super::Cell;

/// One row: its logical position in the ring and its length.
#[derive(Clone, Copy)]
struct Line {
	start: u64,
	len: u32,
}

#[derive(Clone, Default)]
pub(crate) struct Scrollback {
	/// Ring storage. A row is never split across its end.
	cells: Vec<Cell>,
	/// Rows, oldest first. Their physical offset is `start % cells.len()`.
	lines: VecDeque<Line>,
	/// Logical position where the next row is written.
	write: u64,
}

impl Scrollback {
	pub(crate) fn len(&self) -> usize {
		self.lines.len()
	}

	#[cfg(test)]
	pub(crate) fn is_empty(&self) -> bool {
		self.lines.is_empty()
	}

	#[inline]
	pub(crate) fn get(&self, row: usize) -> Option<&[Cell]> {
		let line = self.lines.get(row)?;
		let start = (line.start % self.cells.len() as u64) as usize;
		Some(&self.cells[start..start + line.len as usize])
	}

	/// Cells and rows the ring holds memory for, used or not.
	#[cfg(test)]
	pub(crate) fn capacity(&self) -> (usize, usize) {
		(self.cells.capacity(), self.lines.capacity())
	}

	#[cfg(test)]
	pub(crate) fn iter(&self) -> impl Iterator<Item = &[Cell]> {
		(0..self.len()).filter_map(|row| self.get(row))
	}

	/// Drops every row and gives the ring's memory back: after a flood it
	/// holds megabytes, which `ESC [3J` should release.
	pub(crate) fn clear(&mut self) {
		*self = Self::default();
	}

	/// Gives back ring memory past `cap` cells, keeping every row: after the
	/// line limit is lowered, the ring sized for the old one is not needed.
	pub(crate) fn shrink(&mut self, cap: usize) {
		let used = self
			.lines
			.iter()
			.map(|line| line.len as usize)
			.sum::<usize>();
		if cap.max(used) < self.cells.len() {
			self.grow(cap.max(used));
			self.lines.shrink_to_fit();
		}
	}

	/// Drops the `count` oldest rows.
	pub(crate) fn drain_front(&mut self, count: usize) {
		self.lines.drain(..count.min(self.lines.len()));
	}

	/// Shortens rows longer than `cols`, after the terminal narrows.
	pub(crate) fn truncate_cols(&mut self, cols: usize) {
		for line in &mut self.lines {
			line.len = line.len.min(cols as u32);
		}
	}

	/// All stored cells, including stale ones, for recoloring in place.
	pub(crate) fn cells_mut(&mut self) -> &mut [Cell] {
		&mut self.cells
	}

	/// Appends `row` without its trailing `pad` cells, keeping at most
	/// `limit` (at least 1) rows. The caller vouches that the cells from
	/// `used` on are all `pad`, so they are not scanned. Returns how many
	/// old rows were dropped, and how many cells were stored.
	pub(crate) fn push(
		&mut self,
		row: &[Cell],
		used: usize,
		pad: Cell,
		limit: usize,
	) -> (usize, usize) {
		let len = trimmed_len(&row[..used], pad);
		let limit = limit.max(1);
		let evicted = self.lines.len().saturating_sub(limit - 1);
		self.lines.drain(..evicted);
		// `limit` rows of at most `row.len()` cells, plus less than a row
		// skipped at the ring's end, always fit in `most` cells: the line
		// limit, not space, decides which rows are dropped.
		let most = (limit + 1) * row.len().max(1);
		if self.cells.len() < most && self.overwrites(len) {
			self.grow(
				(self.cells.len() * 2)
					.max(row.len() * 64)
					.min(most)
					.max(len),
			);
		}
		debug_assert!(!self.overwrites(len), "scrollback ring overflow");
		let (start, cap) = (self.place(len), self.cells.len() as u64);
		let offset = (start % cap) as usize;
		self.cells[offset..offset + len].copy_from_slice(&row[..len]);
		self.lines.push_back(Line {
			start,
			len: len as u32,
		});
		self.write = start + len as u64;
		(evicted, len)
	}

	/// Where a row of `len` cells goes: the write position, or the start of
	/// the next lap when the row would cross the ring's end.
	fn place(&self, len: usize) -> u64 {
		let cap = self.cells.len().max(1) as u64;
		if self.write % cap + len as u64 > cap {
			self.write.next_multiple_of(cap)
		} else {
			self.write
		}
	}

	/// Whether a row of `len` cells would overwrite the oldest stored row.
	fn overwrites(&self, len: usize) -> bool {
		let end = self.place(len) + len as u64;
		self.cells.len() < len.max(1)
			|| (self.lines.front()).is_some_and(|front| front.start + (self.cells.len() as u64) < end)
	}

	/// Moves the rows, packed from offset 0, into a buffer of `cap` cells.
	#[cold]
	fn grow(&mut self, cap: usize) {
		let mut cells = Vec::with_capacity(cap);
		let mut write = 0;
		for row in 0..self.lines.len() {
			let line = self.lines[row];
			if let Some(stored) = self.get(row) {
				cells.extend_from_slice(stored);
			}
			self.lines[row] = Line {
				start: write,
				..line
			};
			write += line.len as u64;
		}
		cells.resize(cap, Cell::default());
		self.cells = cells;
		self.write = write;
	}
}

/// Length of `row` without its trailing `pad` cells, scanning four cells
/// per step.
pub(super) fn trimmed_len(row: &[Cell], pad: Cell) -> usize {
	let pad = pad.bits();
	let mut end = row.len();
	// Four cells at a time, which compiles to wide compares.
	while end >= 4 && row[end - 4..end].iter().all(|cell| cell.bits() == pad) {
		end -= 4;
	}
	while end > 0 && row[end - 1].bits() == pad {
		end -= 1;
	}
	end
}

impl Index<usize> for Scrollback {
	type Output = [Cell];

	fn index(&self, row: usize) -> &[Cell] {
		self.get(row).expect("scrollback row out of range")
	}
}
