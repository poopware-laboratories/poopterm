//! The ceiling: the memory traffic no terminal can avoid for a stream, as
//! fast as this machine moves it. An untimed model counts what the stream
//! needs, tokenized by our `vt` parser: one 16-byte cell per character, and
//! the used cells of each line that scrolls off into history. The timed part
//! then only reads every byte once, writes that many cells and copies that
//! many into history. A terminal in safe code cannot beat this on the same
//! machine, so `parse` against `ceiling` shows what is left.
use std::{
	simd::{cmp::SimdPartialOrd, u8x32},
	time::Instant,
};

use super::corpus::Size;
use crate::{
	term::Cell,
	vt::{Handler, Params, Parser, Text},
};

/// Lines of history the timed copy writes into, as a full history would.
const HISTORY: usize = 10_000;

const LANES: usize = 32;

/// What a stream needs from any terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
	/// Cells written: one per character.
	pub(crate) cells: usize,
	/// Cells moved into history: the used cells of each scrolled line.
	pub(crate) kept: usize,
	/// Lines scrolled into history.
	pub(crate) lines: usize,
}

/// A screen that only counts: text wraps at the last column, line feeds
/// move down and scroll at the bottom, carriage returns go to the first
/// column, and every sequence is ignored.
struct Model {
	cols: usize,
	/// Cells used in each screen row, a ring starting at `top`.
	used: Vec<usize>,
	top: usize,
	x: usize,
	y: usize,
	counts: Counts,
}

impl Model {
	fn text(&mut self, mut left: usize) {
		self.counts.cells += left;
		while left > 0 {
			if self.x == self.cols {
				self.x = 0;
				self.line_feed();
			}
			let n = left.min(self.cols - self.x);
			(self.x, left) = (self.x + n, left - n);
			let rows = self.used.len();
			let row = &mut self.used[(self.top + self.y) % rows];
			*row = (*row).max(self.x);
		}
	}

	fn line_feed(&mut self) {
		if self.y + 1 < self.used.len() {
			self.y += 1;
			return;
		}
		self.counts.kept += std::mem::take(&mut self.used[self.top]);
		self.counts.lines += 1;
		self.top = (self.top + 1) % self.used.len();
	}
}

impl Handler for Model {
	fn print_ascii(&mut self, text: &[u8]) {
		self.text(text.len());
	}
	fn print(&mut self, text: Text<'_>) {
		// Every byte but a continuation byte starts a character.
		let chars = text.as_bytes().iter().filter(|&&byte| byte & 0xc0 != 0x80);
		self.text(chars.count());
	}
	fn execute(&mut self, byte: u8) {
		match byte {
			b'\n' | 0x0b | 0x0c => self.line_feed(),
			b'\r' => self.x = 0,
			_ => {}
		}
	}
	fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: bool, _: u8) {}
	fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}
	fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}
	fn hook(&mut self, _: &Params, _: &[u8], _: bool, _: u8) {}
	fn put(&mut self, _: &[u8]) {}
	fn unhook(&mut self) {}
}

/// Counts what `data` needs on a screen of `size`.
pub(crate) fn count(data: &[u8], Size { cols, rows }: Size) -> Counts {
	let mut model = Model {
		cols: cols.max(1),
		used: vec![0; rows.max(1)],
		top: 0,
		x: 0,
		y: 0,
		counts: Counts::default(),
	};
	Parser::new().advance(&mut model, data);
	model.counts
}

/// Bytes below 0x20 in `bytes`: a pass that reads every byte once.
pub(crate) fn count_controls(bytes: &[u8]) -> usize {
	let (chunks, rest) = bytes.as_chunks::<LANES>();
	let mut count = 0;
	for &chunk in chunks {
		let v = u8x32::from_array(chunk);
		count += v.simd_lt(u8x32::splat(0x20)).to_bitmask().count_ones() as usize;
	}
	count + rest.iter().filter(|&&b| b < 0x20).count()
}

/// The unavoidable traffic for `data` needing `counts`: every byte read,
/// the cells written into `grid`, the kept ones copied from it into
/// `history`, in blocks as large as the grid, which stream into memory
/// without reading the destination first, as the real history does.
pub(crate) fn traffic(
	data: &[u8],
	counts: Counts,
	grid: &mut [Cell],
	history: &mut [Cell],
) -> usize {
	let controls = count_controls(data);
	let mut left = counts.cells;
	while left > 0 {
		let n = left.min(grid.len());
		grid[..n].fill(Cell::default());
		left -= n;
	}
	let (mut left, mut at) = (counts.kept, 0);
	while left > 0 {
		let n = left.min(grid.len()).min(history.len());
		if at + n > history.len() {
			at = 0;
		}
		history[at..at + n].copy_from_slice(&grid[..n]);
		(left, at) = (left - n, at + n);
	}
	controls
}

/// MB/s of the ceiling on `data` for a screen of `size`.
pub(crate) fn rate(data: &[u8], size: Size) -> f64 {
	let counts = count(data, size);
	let cols = size.cols.max(1);
	let mut grid = vec![Cell::default(); cols * size.rows.max(1)];
	let mut history = vec![Cell::default(); cols * HISTORY];
	let start = Instant::now();
	let controls = traffic(data, counts, &mut grid, &mut history);
	let elapsed = start.elapsed();
	std::hint::black_box((controls, &grid, &history));
	data.len() as f64 / elapsed.as_secs_f64() / 1e6
}
