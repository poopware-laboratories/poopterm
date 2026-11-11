//! What the pointer is over in a terminal: the link target under it and
//! the shape the program asked for, read from one view of the terminal so
//! the UI takes its lock once.
use crate::{term::Term, ui::selection::GridGeometry};

/// What the pointer is over.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Hover {
	/// The OSC 8 target of the cell under the pointer.
	pub(crate) link: Option<String>,
	/// The pointer shape the program set with OSC 22.
	pub(crate) shape: Option<&'static str>,
}

/// What the pointer at `(x, y)` is over in `term`, drawn at `place`.
pub(crate) fn hover(term: &Term, place: GridPlace, (x, y): (f64, f64)) -> Hover {
	let link = place.geometry(term).point_at(x, y).and_then(|point| {
		let cell = term.row_cells(point.row)?.get(point.col)?;
		term.hyperlink_uri(cell).map(str::to_owned)
	});
	Hover {
		link,
		shape: term.pointer_shape(),
	}
}

/// Where a terminal's grid sits in the window, in physical pixels: what
/// the window decides, without what the terminal does.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GridPlace {
	pub(crate) left: usize,
	pub(crate) top: usize,
	/// End of the grid's area.
	pub(crate) bottom: usize,
	pub(crate) cell: (usize, usize),
}

impl GridPlace {
	/// The grid of `term` at this place, scrolled as `term` is.
	pub(crate) fn geometry(self, term: &Term) -> GridGeometry {
		GridGeometry {
			left: self.left,
			top: self.top,
			bottom: self.bottom,
			cell: self.cell,
			cols: term.cols,
			rows: term.rows,
			first_row: term.scrollback.len().saturating_sub(term.scroll_offset),
		}
	}
}
