//! Tests for grid hit testing and mouse selection drags.
use super::selection::{CellPoint, GridGeometry, Selection};

/// 10 columns by 4 rows of 8x16 cells at (20, 30), with 100 lines of
/// scrollback above and a status bar below the grid's last pixel row.
const GRID: GridGeometry = GridGeometry {
	left: 20,
	top: 30,
	bottom: 110,
	cell: (8, 16),
	cols: 10,
	rows: 4,
	first_row: 100,
};

fn at(row: usize, col: usize) -> CellPoint {
	CellPoint { row, col }
}

#[test]
fn points_inside_the_grid_map_to_cells_below_the_scrollback() {
	assert_eq!(GRID.point_at(20.0, 30.0), Some(at(100, 0)));
	assert_eq!(GRID.point_at(27.9, 45.9), Some(at(100, 0)));
	assert_eq!(GRID.point_at(28.0, 46.0), Some(at(101, 1)));
	assert_eq!(GRID.point_at(99.9, 93.9), Some(at(103, 9)));
}

#[test]
fn points_outside_the_grid_are_no_cell() {
	for (x, y) in [
		(19.9, 50.0),
		(100.0, 50.0),
		(50.0, 29.9),
		(50.0, 110.0),
		(-5.0, -5.0),
	] {
		assert_eq!(GRID.point_at(x, y), None, "({x}, {y})");
	}
}

#[test]
fn the_strip_below_the_last_row_belongs_to_the_last_row() {
	// Rows end at y = 94; the window's leftover pixels up to `bottom` still
	// reach the last row, so clicks there are not lost.
	assert_eq!(GRID.point_at(50.0, 100.0), Some(at(103, 3)));
}

#[test]
fn nearest_clamps_drags_past_every_edge_to_the_closest_cell() {
	assert_eq!(GRID.nearest(-50.0, -50.0), at(100, 0));
	assert_eq!(GRID.nearest(500.0, 50.0), at(101, 9));
	assert_eq!(GRID.nearest(50.0, 900.0), at(103, 3));
	assert_eq!(GRID.nearest(f64::NAN, f64::NAN), at(100, 0));
	assert_eq!(GRID.nearest(50.0, 50.0), GRID.point_at(50.0, 50.0).unwrap());
}

#[test]
fn an_empty_grid_has_no_cells_and_nearest_stays_in_bounds() {
	let empty = GridGeometry {
		cols: 0,
		rows: 0,
		..GRID
	};
	assert_eq!(empty.point_at(30.0, 40.0), None);
	assert_eq!(empty.nearest(30.0, 40.0), at(100, 0));
	let zero_cells = GridGeometry {
		cell: (0, 0),
		..GRID
	};
	// Zero-sized cells (before the font loads) cover nothing and never
	// divide by zero.
	assert_eq!(zero_cells.point_at(30.0, 40.0), None);
	assert_eq!(zero_cells.nearest(30.0, 40.0), at(103, 9));
}

#[test]
fn a_scrolled_view_offsets_rows_but_not_columns() {
	let top_of_history = GridGeometry {
		first_row: 0,
		..GRID
	};
	assert_eq!(top_of_history.point_at(50.0, 50.0), Some(at(1, 3)));
	assert_eq!(top_of_history.nearest(50.0, -1.0), at(0, 3));
	assert_eq!(GRID.point_at(50.0, 50.0), Some(at(101, 3)));
}

#[test]
fn cell_boundaries_fall_on_whole_pixels() {
	// The last pixel of a cell and the first of the next, in both axes.
	assert_eq!(GRID.point_at(35.999, 30.0), Some(at(100, 1)));
	assert_eq!(GRID.point_at(36.0, 30.0), Some(at(100, 2)));
	assert_eq!(GRID.point_at(20.0, 61.999), Some(at(101, 0)));
	assert_eq!(GRID.point_at(20.0, 62.0), Some(at(102, 0)));
}

#[test]
fn point_at_agrees_with_nearest_on_every_pixel_inside() {
	for y in GRID.top..GRID.bottom {
		for x in GRID.left..GRID.left + GRID.cols * GRID.cell.0 {
			let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
			assert_eq!(
				GRID.point_at(fx, fy),
				Some(GRID.nearest(fx, fy)),
				"({x}, {y})"
			);
		}
	}
}

#[test]
fn huge_and_infinite_coordinates_clamp_instead_of_overflowing() {
	assert_eq!(GRID.nearest(f64::INFINITY, f64::INFINITY), at(103, 9));
	assert_eq!(GRID.nearest(f64::NEG_INFINITY, 1e300), at(103, 0));
	assert_eq!(GRID.point_at(f64::INFINITY, 50.0), None);
	assert_eq!(GRID.point_at(f64::NAN, 50.0), None);
}

#[test]
fn a_single_cell_grid_maps_everything_to_its_cell() {
	let one = GridGeometry {
		cols: 1,
		rows: 1,
		bottom: 46,
		..GRID
	};
	assert_eq!(one.point_at(27.0, 45.0), Some(at(100, 0)));
	assert_eq!(one.point_at(28.0, 45.0), None);
	assert_eq!(one.nearest(1000.0, 1000.0), at(100, 0));
}

#[test]
fn a_press_alone_selects_nothing() {
	assert!(Selection::dragged(at(5, 3), at(5, 3), None).is_none());
}

#[test]
fn reaching_another_cell_selects_from_the_anchor() {
	let selection = Selection::dragged(at(5, 3), at(6, 1), None).unwrap();
	assert_eq!((selection.start, selection.end), (at(5, 3), at(6, 1)));
	// The neighbouring cell on the same row is enough.
	assert!(Selection::dragged(at(5, 3), at(5, 4), None).is_some());
}

#[test]
fn returning_to_the_anchor_keeps_a_one_cell_selection() {
	// Once shown, coming back selects the pressed cell alone, so a single
	// character can still be selected.
	let shown = Selection::dragged(at(5, 3), at(5, 4), None);
	let back = Selection::dragged(at(5, 3), at(5, 3), shown).unwrap();
	assert_eq!(back.ordered(), (at(5, 3), at(5, 3)));
}

#[test]
fn the_anchor_stays_put_while_the_end_follows_the_pointer() {
	let shown = Selection::dragged(at(5, 3), at(9, 9), None);
	let next = Selection::dragged(at(5, 3), at(7, 0), shown).unwrap();
	assert_eq!((next.start, next.end), (at(5, 3), at(7, 0)));
}

#[test]
fn dragging_backwards_orders_the_selection() {
	let selection = Selection::dragged(at(5, 3), at(2, 7), None).unwrap();
	assert_eq!(selection.ordered(), (at(2, 7), at(5, 3)));
	let same_row = Selection::dragged(at(5, 3), at(5, 0), None).unwrap();
	assert_eq!(same_row.ordered(), (at(5, 0), at(5, 3)));
}

#[test]
fn a_grid_without_rows_has_no_cells_even_where_columns_are() {
	let no_rows = GridGeometry { rows: 0, ..GRID };
	assert_eq!(no_rows.point_at(50.0, 40.0), None);
}

/// `App::point_at` before hit testing moved into `GridGeometry`, kept as
/// the oracle the move must not change.
fn old_point_at(g: &GridGeometry, x: f64, y: f64) -> Option<CellPoint> {
	let (cw, ch) = g.cell;
	if x < g.left as f64
		|| x >= (g.left + g.cols * cw) as f64
		|| y < g.top as f64
		|| y >= g.bottom as f64
	{
		return None;
	}
	let col = ((x as usize - g.left) / cw.max(1)).min(g.cols - 1);
	let row = ((y as usize - g.top) / ch.max(1)).min(g.rows - 1);
	Some(at(g.first_row + row, col))
}

#[test]
fn hit_testing_matches_the_old_app_formula() {
	let shapes = [
		GRID,
		GridGeometry {
			left: 0,
			top: 0,
			bottom: 47,
			cell: (7, 15),
			cols: 3,
			rows: 3,
			first_row: 0,
		},
		// A vertical tab sidebar and HiDPI-sized cells.
		GridGeometry {
			left: 268,
			top: 92,
			bottom: 600,
			cell: (19, 41),
			cols: 12,
			rows: 12,
			first_row: 5000,
		},
	];
	for g in shapes {
		let right = g.left + g.cols * g.cell.0;
		for y in (0..g.bottom + 20).map(|y| y as f64 + 0.25) {
			for x in (0..right + 20).step_by(3).map(|x| x as f64 + 0.75) {
				assert_eq!(
					g.point_at(x, y),
					old_point_at(&g, x, y),
					"{g:?} at ({x}, {y})"
				);
			}
		}
	}
}

#[test]
fn dragging_further_never_moves_the_end_back() {
	let mut previous = GRID.nearest(-100.0, 50.0);
	for x in (-100..300).map(f64::from) {
		let point = GRID.nearest(x, 50.0);
		assert!(
			point.col >= previous.col && point.row == previous.row,
			"x = {x}"
		);
		previous = point;
	}
	let mut previous = GRID.nearest(50.0, -100.0);
	for y in (-100..300).map(f64::from) {
		let point = GRID.nearest(50.0, y);
		assert!(
			point.row >= previous.row && point.col == previous.col,
			"y = {y}"
		);
		previous = point;
	}
}
