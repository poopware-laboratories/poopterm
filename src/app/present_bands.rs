//! The pixel rows a GPU frame changed since the last one shown, for the
//! present to name: the bands the software path repaints, as plain data.
use super::canvas::merge_bands;
use crate::render::damage::GridDamage;

/// Where the window's parts are, in pixel rows.
#[derive(Clone, Copy, Debug)]
pub(super) struct Layout {
	/// The grid's first row and its cell height and row count.
	pub top: usize,
	pub ch: usize,
	pub rows: usize,
	/// The window's height and the status bar's top.
	pub height: usize,
	pub status_top: usize,
	/// The padding around the grid: its top and bottom.
	pub padding: (usize, usize),
}

impl Layout {
	/// The grid's top, cell height and rows, the window's `height`, the
	/// status bar's top and the padding.
	pub(super) fn new(
		[top, ch, rows]: [usize; 3],
		height: usize,
		status_top: usize,
		padding: (usize, usize),
	) -> Self {
		Self {
			top,
			ch,
			rows,
			height,
			status_top,
			padding,
		}
	}
}

/// The bands `damage` changed in a window laid out as `layout`, with the
/// bars and padding always among them, merged and sorted.
pub(super) fn changed_bands(damage: &GridDamage, layout: &Layout) -> Vec<(usize, usize)> {
	let Layout {
		top,
		ch,
		rows,
		height,
		status_top,
		padding,
	} = *layout;
	let grid_bottom = (top + rows * ch).min(height);
	let mut bands = vec![(status_top, height), (0, top), (padding.0, top)];
	bands.push((grid_bottom, padding.1.max(grid_bottom)));
	if damage.scroll > 0 {
		bands.push((top, grid_bottom));
	}
	let damaged = damage
		.rows
		.iter()
		.enumerate()
		.filter(|(_, damaged)| **damaged);
	bands.extend(damaged.map(|(row, _)| (top + row * ch, top + (row + 1) * ch)));
	let mut bands: Vec<_> = (bands.into_iter())
		.map(|(start, end)| (start, end.min(height)))
		.filter(|(start, end)| start < end)
		.collect();
	merge_bands(&mut bands);
	bands
}
