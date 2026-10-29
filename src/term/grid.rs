//! Row-based screen storage.
//!
//! Each row is its own `Vec<Cell>`, so full-screen scrolling advances a circular row index instead
//! of moving row headers or cells. Rows entering the screen are blanked in place,
//! in memory that is already in cache.
//!
//! Each row also records how far it is used: every cell from there on is
//! the padding cell, so moving a row to the scrollback and erasing to its
//! end skip the unused tail instead of scanning or rewriting it.
use std::ops::{Index, IndexMut};

use super::Cell;

pub(crate) type Row = Vec<Cell>;

#[derive(Clone)]
pub(crate) struct Grid {
	rows: Vec<Row>,
	first: usize,
	/// Per row: every cell at or after it equals `pad`.
	used: Vec<usize>,
	/// The padding cell `used` is measured against.
	pad: Cell,
	cols: usize,
}

impl Grid {
	#[inline(always)]
	fn physical(&self, y: usize) -> usize {
		assert!(y < self.rows.len(), "grid row out of bounds");
		let row = self.first + y;
		if row >= self.rows.len() {
			row - self.rows.len()
		} else {
			row
		}
	}
	fn normalize(&mut self) {
		if self.first != 0 {
			self.rows.rotate_left(self.first);
			self.used.rotate_left(self.first);
			self.first = 0;
		}
	}

	/// A grid of `blank` cells, which also become the padding until
	/// [`Grid::sync_pad`] says otherwise.
	pub(crate) fn new(cols: usize, rows: usize, blank: Cell) -> Self {
		Self {
			rows: (0..rows).map(|_| vec![blank; cols]).collect(),
			first: 0,
			used: vec![0; rows],
			pad: blank,
			cols,
		}
	}

	#[cfg(test)]
	pub(crate) fn iter(&self) -> impl Iterator<Item = &Row> {
		self.rows[self.first..]
			.iter()
			.chain(self.rows[..self.first].iter())
	}

	pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut Row> {
		self.used.fill(self.cols);
		let (before, after) = self.rows.split_at_mut(self.first);
		after.iter_mut().chain(before.iter_mut())
	}

	/// How far row `y` is used: the cells after it are all padding.
	#[inline]
	pub(crate) fn used(&self, y: usize) -> usize {
		self.used[self.physical(y)]
	}

	/// Measures `used` against `pad` from now on. A different padding cell
	/// makes every row count as fully used.
	#[inline]
	pub(crate) fn sync_pad(&mut self, pad: Cell) {
		if pad != self.pad {
			self.pad = pad;
			self.used.fill(self.cols);
		}
	}

	/// Counts row `y` as used at least up to `end`.
	#[inline(always)]
	pub(crate) fn mark_used(&mut self, y: usize, end: usize) {
		let y = self.physical(y);
		let used = &mut self.used[y];
		*used = (*used).max(end.min(self.cols));
	}

	/// Row `y` without marking it: the caller marks what it wrote with
	/// [`Grid::mark_used`] before anything else reads the grid.
	#[inline(always)]
	pub(crate) fn row_unmarked(&mut self, y: usize) -> &mut Row {
		let y = self.physical(y);
		&mut self.rows[y]
	}

	/// Row `y`, for writing cells before `end` only.
	#[inline(always)]
	pub(crate) fn row_upto(&mut self, y: usize, end: usize) -> &mut Row {
		self.mark_used(y, end);
		let y = self.physical(y);
		&mut self.rows[y]
	}

	/// Sets cells `start..stop` of row `y` to `cell`. Padding written up to
	/// the used end shortens the row instead of being written past it.
	#[inline]
	pub(crate) fn fill(&mut self, y: usize, start: usize, stop: usize, cell: Cell) {
		let y = self.physical(y);
		let used = &mut self.used[y];
		if cell == self.pad && stop >= *used {
			if start < *used {
				Cell::fill(&mut self.rows[y][start..*used], cell);
				*used = start;
			}
		} else {
			Cell::fill(&mut self.rows[y][start..stop], cell);
			*used = (*used).max(stop);
		}
	}

	/// Scrolls rows `top..=bottom` up by `count`. Rows leaving the region are
	/// passed to `evict` (oldest first) with how far they are used; it
	/// returns how many leading cells to blank, and the caller vouches that
	/// the rest already equal `blank`.
	pub(crate) fn scroll_up(
		&mut self,
		top: usize,
		bottom: usize,
		count: usize,
		blank: Cell,
		mut evict: impl FnMut(&[Cell], usize) -> usize,
	) {
		let count = count.min(bottom + 1 - top);
		if top == 0 && bottom + 1 == self.rows.len() {
			let emptied = if blank == self.pad { 0 } else { self.cols };
			for logical in 0..count {
				let y = self.physical(logical);
				let dirty = evict(&self.rows[y], self.used[y]);
				Cell::fill(&mut self.rows[y][..dirty], blank);
				self.used[y] = emptied;
			}
			self.first += count;
			if self.first >= self.rows.len() {
				self.first -= self.rows.len();
			}
			return;
		}
		self.normalize();
		let region = &mut self.rows[top..=bottom];
		let used = &mut self.used[top..=bottom];
		let emptied = if blank == self.pad { 0 } else { self.cols };
		for (row, &used) in region[..count].iter_mut().zip(&used[..count]) {
			let dirty = evict(row, used);
			Cell::fill(&mut row[..dirty], blank);
		}
		if count == 1 {
			rotate_left_one(region);
		} else {
			region.rotate_left(count);
		}
		// The rows that wrapped around are all blank: one move, unlike a rotation.
		let kept = used.len() - count;
		used.copy_within(count.., 0);
		used[kept..].fill(emptied);
	}

	/// Scrolls rows `top..=bottom` down by `count`, blanking the rows at the top.
	pub(crate) fn scroll_down(&mut self, top: usize, bottom: usize, count: usize, blank: Cell) {
		self.normalize();
		let count = count.min(bottom + 1 - top);
		let region = &mut self.rows[top..=bottom];
		region.rotate_right(count);
		for row in &mut region[..count] {
			Cell::fill(row, blank);
		}
		let used = &mut self.used[top..=bottom];
		used.rotate_right(count);
		used[..count].fill(if blank == self.pad { 0 } else { self.cols });
	}

	/// Resizes to `cols` × `rows`, keeping the top-left content.
	pub(crate) fn resize(&mut self, cols: usize, rows: usize, blank: Cell) {
		self.normalize();
		self.rows.truncate(rows);
		for row in &mut self.rows {
			row.resize(cols, blank);
		}
		self.rows.resize_with(rows, || vec![blank; cols]);
		self.cols = cols;
		self.used = vec![cols; rows];
	}

	/// Checks that cells after each row's used end are padding.
	#[cfg(test)]
	pub(crate) fn assert_extents(&self) {
		for (y, (row, &used)) in self.rows.iter().zip(&self.used).enumerate() {
			assert!(used <= self.cols, "row {y} used {used}");
			let stray = row[used..].iter().position(|cell| *cell != self.pad);
			assert_eq!(stray, None, "row {y} used {used}");
		}
	}
}

/// `rows.rotate_left(1)`, for a non-empty `rows`, with one move of the row headers; the slice method
/// copies through a buffer, in three calls, on every line feed.
#[inline]
fn rotate_left_one(rows: &mut [Row]) {
	let last = rows.len() - 1;
	let rows = rows.as_mut_ptr();
	// SAFETY: the first row is read out, the others shift down by one inside
	// the slice, and the first is written to the slot that frees up, so every
	// row is owned exactly once again; nothing between can panic.
	unsafe {
		let first = std::ptr::read(rows);
		std::ptr::copy(rows.add(1), rows, last);
		std::ptr::write(rows.add(last), first);
	}
}

impl Index<usize> for Grid {
	type Output = Row;

	#[inline(always)]
	fn index(&self, row: usize) -> &Row {
		&self.rows[self.physical(row)]
	}
}

impl IndexMut<usize> for Grid {
	/// Counts the whole row as used; writers that know better use
	/// [`Grid::row_upto`] or [`Grid::fill`].
	#[inline(always)]
	fn index_mut(&mut self, row: usize) -> &mut Row {
		let row = self.physical(row);
		self.used[row] = self.cols;
		&mut self.rows[row]
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn cell(ch: char) -> Cell {
		Cell {
			ch,
			..Cell::default()
		}
	}

	fn grid(lines: &[&str]) -> Grid {
		let mut grid = Grid::new(lines[0].len(), lines.len(), cell(' '));
		for (y, line) in lines.iter().enumerate() {
			for (x, ch) in line.chars().enumerate() {
				grid[y][x] = cell(ch);
			}
		}
		grid
	}

	fn text(grid: &Grid) -> Vec<String> {
		grid
			.iter()
			.map(|row| row.iter().map(|c| c.ch).collect())
			.collect()
	}

	#[test]
	fn scrolling_up_hands_leaving_rows_out_before_blanking_them() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		let mut evicted = Vec::new();
		g.scroll_up(0, 2, 2, cell('.'), |row, _| {
			evicted.push(row.iter().map(|c| c.ch).collect::<String>());
			row.len()
		});
		assert_eq!(evicted, ["aa", "bb"]);
		assert_eq!(text(&g), ["cc", "..", "..", "dd"]);
	}

	#[test]
	fn scrolling_up_by_more_than_the_region_evicts_and_blanks_it_all() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		let mut evicted = Vec::new();
		g.scroll_up(1, 2, 9, cell('.'), |row, _| {
			evicted.push(row.iter().map(|c| c.ch).collect::<String>());
			row.len()
		});
		assert_eq!(evicted, ["bb", "cc"]);
		assert_eq!(text(&g), ["aa", "..", "..", "dd"]);
	}

	#[test]
	fn scrolling_up_by_one_moves_rows_without_copying_them() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		let buffers: Vec<_> = g.iter().map(|row| row.as_ptr()).collect();
		g.scroll_up(0, 3, 1, cell('.'), |row, _| row.len());
		assert_eq!(text(&g), ["bb", "cc", "dd", ".."]);
		let moved: Vec<_> = g.iter().map(|row| row.as_ptr()).collect();
		assert_eq!(moved, [buffers[1], buffers[2], buffers[3], buffers[0]]);
	}

	#[test]
	fn scrolling_a_region_up_by_one_leaves_the_rows_around_it_in_place() {
		let mut g = grid(&["aa", "bb", "cc", "dd", "ee"]);
		let buffers: Vec<_> = g.iter().map(|row| row.as_ptr()).collect();
		g.scroll_up(1, 3, 1, cell('.'), |row, _| row.len());
		assert_eq!(text(&g), ["aa", "cc", "dd", "..", "ee"]);
		let moved: Vec<_> = g.iter().map(|row| row.as_ptr()).collect();
		let expected = [buffers[0], buffers[2], buffers[3], buffers[1], buffers[4]];
		assert_eq!(moved, expected);
	}

	#[test]
	fn scrolling_one_row_regions_up_by_one_blanks_them() {
		for (top, expected) in [
			(0, ["..", "bb", "cc"]),
			(1, ["aa", "..", "cc"]),
			(2, ["aa", "bb", ".."]),
		] {
			let mut g = grid(&["aa", "bb", "cc"]);
			g.scroll_up(top, top, 1, cell('.'), |row, _| row.len());
			assert_eq!(text(&g), expected, "row {top}");
		}
	}

	#[test]
	fn single_row_scrolls_add_up_to_one_larger_scroll() {
		let lines = ["aa", "bb", "cc", "dd", "ee", "ff"];
		for (top, bottom) in [(0, 5), (1, 4), (2, 3), (0, 2), (3, 5)] {
			for count in 1..=bottom + 1 - top {
				let (mut stepped, mut at_once) = (grid(&lines), grid(&lines));
				let (mut evicted_stepped, mut evicted_at_once) = (Vec::new(), Vec::new());
				for _ in 0..count {
					stepped.scroll_up(top, bottom, 1, cell('.'), |row, _| {
						evicted_stepped.push(row[0].ch);
						row.len()
					});
				}
				at_once.scroll_up(top, bottom, count, cell('.'), |row, _| {
					evicted_at_once.push(row[0].ch);
					row.len()
				});
				let case = (top, bottom, count);
				assert_eq!(text(&stepped), text(&at_once), "{case:?}");
				assert_eq!(evicted_stepped, evicted_at_once, "{case:?}");
			}
		}
	}

	#[test]
	fn scrolling_up_by_zero_changes_nothing() {
		let mut g = grid(&["aa", "bb"]);
		g.scroll_up(0, 1, 0, cell('.'), |_, _| panic!("no row leaves"));
		assert_eq!(text(&g), ["aa", "bb"]);
	}

	#[test]
	fn scrolling_down_by_more_than_the_region_blanks_it_all() {
		let mut g = grid(&["aa", "bb", "cc"]);
		g.scroll_down(0, 1, 5, cell('.'));
		assert_eq!(text(&g), ["..", "..", "cc"]);
	}

	#[test]
	fn blanked_rows_take_every_field_of_the_blank() {
		let mut g = grid(&["ab", "cd"]);
		let blank = Cell {
			bg: 4,
			attrs: 2,
			..cell(' ')
		};
		g.scroll_up(0, 1, 1, blank, |row, _| row.len());
		assert!(g[1].iter().all(|c| *c == blank));
		assert_eq!(g[0][1].ch, 'd');
	}

	#[test]
	fn only_the_prefix_evict_names_is_blanked() {
		let mut g = grid(&["abc", "def", "ghi"]);
		let mut dirty = [1, 0].into_iter();
		g.scroll_up(0, 2, 2, cell('.'), |_, _| dirty.next().unwrap());
		assert_eq!(text(&g), ["ghi", ".bc", "def"]);
	}

	#[test]
	fn scrolling_down_inside_a_region_leaves_other_rows() {
		let mut g = grid(&["aa", "bb", "cc", "dd"]);
		g.scroll_down(1, 2, 1, cell('.'));
		assert_eq!(text(&g), ["aa", "..", "bb", "dd"]);
	}

	#[test]
	fn resize_keeps_top_left_content() {
		let mut g = grid(&["abc", "def"]);
		g.resize(2, 3, cell('.'));
		assert_eq!(text(&g), ["ab", "de", ".."]);
		assert_eq!((g.cols, g.iter().count()), (2, 3));
	}
}

#[cfg(test)]
#[path = "grid/ring_tests.rs"]
mod ring_tests;
