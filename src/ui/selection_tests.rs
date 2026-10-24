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
	assert!(Selection::dragged(at(5, 3), at(5, 3), None, false).is_none());
}

#[test]
fn reaching_another_cell_selects_from_the_anchor() {
	let selection = Selection::dragged(at(5, 3), at(6, 1), None, false).unwrap();
	assert_eq!((selection.start, selection.end), (at(5, 3), at(6, 1)));
	// The neighbouring cell on the same row is enough.
	assert!(Selection::dragged(at(5, 3), at(5, 4), None, false).is_some());
}

#[test]
fn returning_to_the_anchor_keeps_a_one_cell_selection() {
	// Once shown, coming back selects the pressed cell alone, so a single
	// character can still be selected.
	let shown = Selection::dragged(at(5, 3), at(5, 4), None, false);
	let back = Selection::dragged(at(5, 3), at(5, 3), shown, false).unwrap();
	assert_eq!(back.ordered(), (at(5, 3), at(5, 3)));
}

#[test]
fn the_anchor_stays_put_while_the_end_follows_the_pointer() {
	let shown = Selection::dragged(at(5, 3), at(9, 9), None, false);
	let next = Selection::dragged(at(5, 3), at(7, 0), shown, false).unwrap();
	assert_eq!((next.start, next.end), (at(5, 3), at(7, 0)));
}

#[test]
fn dragging_backwards_orders_the_selection() {
	let selection = Selection::dragged(at(5, 3), at(2, 7), None, false).unwrap();
	assert_eq!(selection.ordered(), (at(2, 7), at(5, 3)));
	let same_row = Selection::dragged(at(5, 3), at(5, 0), None, false).unwrap();
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

fn term_with_history_limit(limit: usize) -> crate::term::Term {
	use crate::theme::{Theme, ThemeKind};
	let mut term = crate::term::Term::new(8, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	term.scrollback_limit = limit;
	term
}

fn feed(term: &mut crate::term::Term, bytes: &[u8]) {
	crate::parser::Parser::new().advance(term, bytes);
}

/// The text a selection in lines covers on its first row, read now.
fn selected_row(term: &crate::term::Term, selection: Selection) -> Option<String> {
	let rows = selection.in_rows(term.history_base)?;
	let cells = term.row_cells(rows.start.row)?;
	Some(
		cells
			.iter()
			.map(|cell| cell.ch)
			.collect::<String>()
			.trim_end()
			.to_owned(),
	)
}

#[test]
fn a_selection_keeps_its_text_when_full_history_evicts_lines() {
	// The reported bug: once history is full, new output used to slide the
	// text out from under a selection made on the bottom rows.
	let mut term = term_with_history_limit(3);
	feed(&mut term, b"1\r\n2\r\n3\r\n4\r\ntarget");
	assert_eq!(term.scrollback.len(), 3, "history is full");
	let bottom = term.history_base + term.scrollback.len() + 1;
	let selection = Selection {
		start: at(bottom, 0),
		end: at(bottom, 5),
		block: false,
	};
	assert_eq!(selected_row(&term, selection).as_deref(), Some("target"));
	feed(&mut term, b"\r\nnext\r\nmore");
	assert_eq!(term.scrollback.len(), 3, "still full: lines were evicted");
	assert_eq!(selected_row(&term, selection).as_deref(), Some("target"));
}

#[test]
fn a_selection_keeps_its_text_when_history_is_cleared() {
	let mut term = term_with_history_limit(100);
	feed(&mut term, b"a\r\nb\r\nc\r\nkeep");
	let line = term.history_base + term.scrollback.len() + 1;
	let selection = Selection {
		start: at(line, 0),
		end: at(line, 3),
		block: false,
	};
	feed(&mut term, b"\x1b[3J");
	assert!(term.scrollback.is_empty());
	assert_eq!(selected_row(&term, selection).as_deref(), Some("keep"));
}

#[test]
fn lines_become_rows_by_subtracting_the_evicted_count() {
	let selection = Selection {
		start: at(12, 4),
		end: at(10, 1),
		block: false,
	};
	let rows = selection.in_rows(10).unwrap();
	assert_eq!((rows.start, rows.end), (at(0, 1), at(2, 4)), "and ordered");
	assert_eq!(
		selection.in_rows(0).unwrap().ordered(),
		(at(10, 1), at(12, 4))
	);
}

#[test]
fn a_partly_evicted_selection_starts_at_the_oldest_row() {
	let selection = Selection {
		start: at(3, 6),
		end: at(7, 2),
		block: false,
	};
	let rows = selection.in_rows(5).unwrap();
	assert_eq!((rows.start, rows.end), (at(0, 0), at(2, 2)));
	// Evicted up to the end's own line: only that line remains.
	assert_eq!(
		selection.in_rows(7).unwrap().ordered(),
		(at(0, 0), at(0, 2))
	);
}

#[test]
fn a_fully_evicted_selection_is_gone() {
	let selection = Selection {
		start: at(3, 6),
		end: at(7, 2),
		block: false,
	};
	assert!(selection.in_rows(8).is_none());
	assert!(selection.in_rows(usize::MAX).is_none());
}

#[test]
fn without_history_a_selection_follows_its_line_up_then_disappears() {
	let mut term = term_with_history_limit(0);
	feed(&mut term, b"a\r\ntarget");
	let line = term.history_base + 1;
	let selection = Selection {
		start: at(line, 0),
		end: at(line, 5),
		block: false,
	};
	feed(&mut term, b"\r\nnext");
	assert_eq!(selection.in_rows(term.history_base).unwrap().start.row, 0);
	assert_eq!(selected_row(&term, selection).as_deref(), Some("target"));
	feed(&mut term, b"\r\nmore");
	assert!(
		selection.in_rows(term.history_base).is_none(),
		"not moved onto `next`"
	);
}

#[test]
fn lowering_the_history_limit_keeps_the_selection_on_a_surviving_line() {
	let mut term = term_with_history_limit(10);
	feed(&mut term, b"1\r\n2\r\nkeep\r\n4\r\n5\r\nend");
	let line = term.history_base + 2;
	let selection = Selection {
		start: at(line, 0),
		end: at(line, 3),
		block: false,
	};
	term.set_scrollback_limit(3);
	assert_eq!(selected_row(&term, selection).as_deref(), Some("keep"));
}

fn linear(start: CellPoint, end: CellPoint) -> Selection {
	Selection {
		start,
		end,
		block: false,
	}
}

fn block(start: CellPoint, end: CellPoint) -> Selection {
	Selection {
		start,
		end,
		block: true,
	}
}

#[test]
fn a_linear_selection_runs_from_the_start_to_the_end_of_each_line() {
	let selection = linear(at(2, 5), at(4, 3));
	assert_eq!(selection.span(1, 10), None);
	assert_eq!(selection.span(2, 10), Some((5, 9)));
	assert_eq!(selection.span(3, 10), Some((0, 9)));
	assert_eq!(selection.span(4, 10), Some((0, 3)));
	assert_eq!(selection.span(5, 10), None);
	assert_eq!(
		linear(at(2, 8), at(2, 3)).span(2, 10),
		Some((3, 8)),
		"dragged backwards"
	);
}

#[test]
fn a_block_selection_takes_the_same_columns_on_every_row() {
	let selection = block(at(2, 5), at(4, 3));
	assert_eq!(selection.span(1, 10), None);
	for row in 2..=4 {
		assert_eq!(selection.span(row, 10), Some((3, 5)), "row {row}");
	}
	assert_eq!(selection.span(5, 10), None);
}

#[test]
fn a_block_has_the_same_corners_whichever_way_it_was_dragged() {
	let corners = (at(2, 3), at(4, 5));
	for (from, to) in [
		(at(2, 3), at(4, 5)),
		(at(4, 5), at(2, 3)),
		(at(2, 5), at(4, 3)),
		(at(4, 3), at(2, 5)),
	] {
		assert_eq!(block(from, to).ordered(), corners, "{from:?} to {to:?}");
	}
	// A linear selection keeps the text order instead.
	assert_eq!(linear(at(2, 5), at(4, 3)).ordered(), (at(2, 5), at(4, 3)));
}

#[test]
fn spans_stop_at_the_last_column_and_an_empty_grid_has_none() {
	assert_eq!(linear(at(0, 2), at(0, 40)).span(0, 10), Some((2, 9)));
	assert_eq!(block(at(0, 2), at(3, 40)).span(1, 10), Some((2, 9)));
	assert_eq!(
		block(at(0, 12), at(3, 40)).span(1, 10),
		None,
		"right of the grid"
	);
	assert_eq!(linear(at(0, 0), at(0, 3)).span(0, 0), None);
}

#[test]
fn contains_agrees_with_span() {
	for selection in [linear(at(1, 6), at(3, 2)), block(at(1, 6), at(3, 2))] {
		for row in 0..5 {
			for col in 0..10 {
				let inside = selection
					.span(row, 10)
					.is_some_and(|(l, r)| (l..=r).contains(&col));
				assert_eq!(
					selection.contains(at(row, col)),
					inside,
					"{selection:?} at ({row}, {col})"
				);
			}
		}
	}
	assert!(!block(at(1, 6), at(3, 2)).contains(at(2, 1)));
	assert!(linear(at(1, 6), at(3, 2)).contains(at(2, 1)));
}

#[test]
fn an_alt_drag_makes_a_block_and_a_press_alone_still_selects_nothing() {
	assert!(Selection::dragged(at(5, 3), at(5, 3), None, true).is_none());
	let selection = Selection::dragged(at(5, 3), at(7, 1), None, true).unwrap();
	assert!(selection.block);
	assert_eq!(selection.span(6, 10), Some((1, 3)));
	assert!(
		!Selection::dragged(at(5, 3), at(7, 1), None, false)
			.unwrap()
			.block
	);
}

#[test]
fn a_partly_evicted_block_keeps_its_columns() {
	let rows = block(at(3, 6), at(7, 2)).in_rows(5).unwrap();
	assert!(rows.block);
	assert_eq!(rows.ordered(), (at(0, 2), at(2, 6)));
	assert!(block(at(3, 6), at(7, 2)).in_rows(8).is_none());
}
