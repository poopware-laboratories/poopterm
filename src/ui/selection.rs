//! Grid coordinates, hit testing and text selections.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CellPoint {
	pub(crate) row: usize,
	pub(crate) col: usize,
}

impl CellPoint {
	pub(crate) const fn new(row: usize, col: usize) -> Self {
		Self { row, col }
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
	pub(crate) start: CellPoint,
	pub(crate) end: CellPoint,
	/// A rectangle (Alt-drag): the same columns on every row, instead of a
	/// run of text from `start` to `end`.
	pub(crate) block: bool,
}

impl Selection {
	/// The first and last cells: in text order, or a block's top-left and
	/// bottom-right corners.
	pub(crate) fn ordered(self) -> (CellPoint, CellPoint) {
		let (start, end) = (self.start.min(self.end), self.start.max(self.end));
		if !self.block {
			return (start, end);
		}
		let cols = (
			self.start.col.min(self.end.col),
			self.start.col.max(self.end.col),
		);
		(
			CellPoint::new(start.row, cols.0),
			CellPoint::new(end.row, cols.1),
		)
	}

	/// The first and last selected columns of `row` in a grid `cols` wide.
	pub(crate) fn span(self, row: usize, cols: usize) -> Option<(usize, usize)> {
		let (start, end) = self.ordered();
		if row < start.row || row > end.row || cols == 0 {
			return None;
		}
		// A block's edges are columns; a run of text wraps between rows.
		let left = if self.block || row == start.row {
			start.col
		} else {
			0
		};
		let right = if self.block || row == end.row {
			end.col
		} else {
			cols
		};
		let right = right.min(cols - 1);
		(left <= right).then_some((left, right))
	}

	pub(crate) fn contains(self, point: CellPoint) -> bool {
		self
			.span(point.row, usize::MAX)
			.is_some_and(|(left, right)| (left..=right).contains(&point.col))
	}

	/// Converts a selection in lines, which count every line since the
	/// terminal started and so keep naming the same text while full history
	/// evicts lines, into rows of the current scrollback and screen, ordered.
	/// `None` once all of it was evicted; a partly evicted one starts at the
	/// oldest row, in its own columns for a block.
	pub(crate) fn in_rows(self, history_base: usize) -> Option<Selection> {
		let (start, end) = self.ordered();
		let end = CellPoint::new(end.row.checked_sub(history_base)?, end.col);
		let start = match start.row.checked_sub(history_base) {
			Some(row) => CellPoint::new(row, start.col),
			None => CellPoint::new(0, if self.block { start.col } else { 0 }),
		};
		Some(Selection {
			start,
			end,
			block: self.block,
		})
	}

	/// The selection while dragging from `anchor` to `point`, given the one
	/// `shown` so far: nothing until the pointer leaves the pressed cell, so
	/// a click selects nothing, but once shown it may shrink to one cell.
	pub(crate) fn dragged(
		anchor: CellPoint,
		point: CellPoint,
		shown: Option<Selection>,
		block: bool,
	) -> Option<Selection> {
		(shown.is_some() || point != anchor).then_some(Selection {
			start: anchor,
			end: point,
			block,
		})
	}
}

/// Where the grid sits in the window, in physical pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GridGeometry {
	pub(crate) left: usize,
	pub(crate) top: usize,
	/// End of the grid's area; pixels between the last row and here still
	/// belong to the last row.
	pub(crate) bottom: usize,
	pub(crate) cell: (usize, usize),
	pub(crate) cols: usize,
	pub(crate) rows: usize,
	/// Row number, counting scrollback, of the first visible line.
	pub(crate) first_row: usize,
}

impl GridGeometry {
	/// The cell under `(x, y)`, or `None` outside the grid.
	pub(crate) fn point_at(&self, x: f64, y: f64) -> Option<CellPoint> {
		let right = self.left + self.cols * self.cell.0;
		let inside =
			x >= self.left as f64 && x < right as f64 && y >= self.top as f64 && y < self.bottom as f64;
		(inside && self.rows > 0).then(|| self.nearest(x, y))
	}

	/// The cell closest to `(x, y)`, so a drag past an edge keeps reaching
	/// the cells along it.
	pub(crate) fn nearest(&self, x: f64, y: f64) -> CellPoint {
		// `as usize` saturates: negative and NaN offsets become 0.
		let col = (x - self.left as f64) as usize / self.cell.0.max(1);
		let row = (y - self.top as f64) as usize / self.cell.1.max(1);
		CellPoint {
			row: self.first_row + row.min(self.rows.saturating_sub(1)),
			col: col.min(self.cols.saturating_sub(1)),
		}
	}
}
