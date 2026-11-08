//! Drawing a split tab's panes: each pane's grid in its place, whole, or
//! only its changed rows over the background restored under them.
use super::{
	glyphs::FontSet,
	grid::{GridView, draw_grid},
	padding::{extend_above, extend_below},
	pen::{Pen, Untracked},
};
use crate::{graphics::Rect, ui::panes::damaged_strips};

/// Draws the panes `views`, each placed in its pane with its edge
/// backgrounds reaching into its padding: whole when `rows` is `None`,
/// through a pen that keeps no rows of any; otherwise each pane's changed
/// `rows` only, once `restore` has repainted the background under them and
/// around the grid. Returns the pixel rows repainted, as `(start, end)`.
pub(crate) fn draw_panes<P: Pen>(
	p: &mut P,
	fonts: &mut FontSet,
	views: &[GridView],
	rows: Option<&[Vec<bool>]>,
	mut restore: impl FnMut(&mut P, Rect),
) -> Vec<(usize, usize)> {
	let mut bands = Vec::new();
	for (slot, view) in views.iter().enumerate() {
		let Some(rows) = rows.and_then(|rows| rows.get(slot)) else {
			draw_grid(&mut Untracked(&mut *p), fonts, view, None);
			continue;
		};
		let (x, y) = view.origin;
		let grid = Rect {
			x,
			y,
			width: view.frame.cols * view.cell.0,
			height: view.frame.rows * view.cell.1,
		};
		let height = p.size().1;
		for strip in damaged_strips(y, view.padding.unwrap_or(grid), view.cell.1, rows) {
			let band = (strip.y.min(height), (strip.y + strip.height).min(height));
			if band.0 < band.1 {
				restore(p, strip);
				bands.push(band);
			}
		}
		draw_grid(p, fonts, view, Some(rows));
		extend_above(p, view);
		extend_below(p, view);
	}
	bands
}
