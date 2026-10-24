//! Grid coordinates, hit testing and text selections.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CellPoint {
	pub(crate) row: usize,
	pub(crate) col: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Selection {
	pub(crate) start: CellPoint,
	pub(crate) end: CellPoint,
}

impl Selection {
	pub(crate) fn ordered(self) -> (CellPoint, CellPoint) {
		if self.start <= self.end {
			(self.start, self.end)
		} else {
			(self.end, self.start)
		}
	}

	/// The selection while dragging from `anchor` to `point`, given the one
	/// `shown` so far: nothing until the pointer leaves the pressed cell, so
	/// a click selects nothing, but once shown it may shrink to one cell.
	pub(crate) fn dragged(
		anchor: CellPoint,
		point: CellPoint,
		shown: Option<Selection>,
	) -> Option<Selection> {
		(shown.is_some() || point != anchor).then_some(Selection {
			start: anchor,
			end: point,
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
