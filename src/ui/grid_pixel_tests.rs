//! Pixel positions inside the grid, as SGR-Pixels mouse reports (`?1016`)
//! give them: from 1,1 at the grid's top-left pixel, like cells.
use super::selection::GridGeometry;

/// 10 columns by 4 rows of 8x16 cells at (20, 30): 80x64 pixels, with a
/// status bar from y = 94 to 110 still counted as the last row.
const GRID: GridGeometry = GridGeometry {
	left: 20,
	top: 30,
	bottom: 110,
	cell: (8, 16),
	cols: 10,
	rows: 4,
	first_row: 100,
};

#[test]
fn the_grids_top_left_pixel_is_1_1() {
	assert_eq!(GRID.pixel_at(20.0, 30.0), Some((1, 1)));
}

#[test]
fn the_first_cells_last_pixel_is_the_cell_size() {
	assert_eq!(GRID.pixel_at(27.0, 45.0), Some((8, 16)));
	assert_eq!(
		GRID.pixel_at(28.0, 46.0),
		Some((9, 17)),
		"the next cell's first pixel"
	);
}

#[test]
fn a_position_inside_a_pixel_is_that_pixel() {
	assert_eq!(GRID.pixel_at(20.9, 30.99), Some((1, 1)));
	assert_eq!(GRID.pixel_at(23.5, 33.25), Some((4, 4)));
}

#[test]
fn the_grids_last_pixel_is_its_width_and_height() {
	assert_eq!(GRID.pixel_at(99.0, 93.0), Some((80, 64)));
}

#[test]
fn the_band_below_the_last_row_reports_the_last_pixel_row() {
	// Its cells are the last row's; a program dividing by the cell height
	// must not land on a row past the grid.
	assert_eq!(GRID.pixel_at(20.0, 94.0), Some((1, 64)));
	assert_eq!(GRID.pixel_at(55.0, 109.0), Some((36, 64)));
}

#[test]
fn outside_the_grid_there_is_no_pixel() {
	for (x, y) in [
		(19.0, 30.0),
		(20.0, 29.0),
		(100.0, 30.0),
		(20.0, 110.0),
		(-5.0, -5.0),
	] {
		assert_eq!(GRID.pixel_at(x, y), None, "{x},{y}");
	}
}

#[test]
fn a_grid_without_rows_has_no_pixels() {
	let empty = GridGeometry { rows: 0, ..GRID };
	assert_eq!(empty.pixel_at(20.0, 30.0), None);
}
