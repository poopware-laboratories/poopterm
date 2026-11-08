//! Tests of a split tab's grids on the GPU pen: grids drawn through an
//! untracked pen draw the quads a tracked grid draws, and leave the pen
//! keeping no rows, so the frame after draws every row again.
use super::{
	quads::QuadPen,
	rows_tests::{draw, lines, pen},
};
use crate::render::{
	damage::grid_damage,
	grid::{GridView, draw_grid},
	grid_case_tests::{CELL, SAMPLE, SIZE, fonts, frame, view},
	pen::Untracked,
};

/// A frame of `views`, each grid drawn through an untracked pen.
fn untracked(pen: &mut QuadPen, views: &[GridView]) {
	pen.begin_frame(SIZE);
	for view in views {
		draw_grid(&mut Untracked(&mut *pen), &mut fonts(), view, None);
	}
}

#[test]
fn an_untracked_grid_draws_the_quads_a_tracked_grid_draws() {
	for input in [SAMPLE.to_owned(), lines(0..5), lines(3..8)] {
		let shown = frame(&input);
		let mut tracked = pen();
		tracked.begin_frame(SIZE);
		draw_grid(&mut tracked, &mut fonts(), &view(&shown), None);
		let mut plain = pen();
		untracked(&mut plain, &[view(&shown)]);
		assert_eq!(plain.quads, tracked.quads, "{input:?}");
		assert!(!plain.quads.is_empty());
	}
}

#[test]
fn two_untracked_grids_draw_both_in_order() {
	let (left, right) = (frame(&lines(0..5)), frame(SAMPLE));
	let moved = GridView {
		origin: (CELL.0, CELL.1),
		..view(&right)
	};
	let mut both = pen();
	untracked(&mut both, &[view(&left), moved]);
	let (mut first, mut second) = (pen(), pen());
	untracked(&mut first, &[view(&left)]);
	untracked(&mut second, &[moved]);
	// The first grid's quads, then as many as the second draws alone.
	let start = first.quads.len();
	assert_eq!(both.quads[..start], first.quads[..]);
	assert_eq!(both.quads.len(), start + second.quads.len());
	let at = |quads: &[super::quads::Quad]| quads.iter().map(|quad| quad.at).collect::<Vec<_>>();
	assert_eq!(at(&both.quads[start..]), at(&second.quads));
}

#[test]
fn untracked_grids_leave_no_rows_to_keep() {
	let shown = frame(SAMPLE);
	let mut pen = pen();
	draw(&mut pen, &view(&shown));
	assert!(pen.keeps_rows(), "a tracked grid's rows are kept");
	untracked(&mut pen, &[view(&shown), view(&shown)]);
	assert!(!pen.keeps_rows(), "no grid ended in this frame");
	// The next frame, planned in part, draws every row.
	let damage = grid_damage(&shown, &shown, true).expect("same size");
	pen.plan_rows(Some(damage), CELL.1);
	pen.begin_frame(SIZE);
	assert_eq!(pen.rows_to_draw(), None);
}

#[test]
fn a_tracked_grid_after_untracked_ones_keeps_its_rows_again() {
	let shown = frame(&lines(0..5));
	let mut pen = pen();
	untracked(&mut pen, &[view(&shown)]);
	assert!(!pen.keeps_rows());
	// A tab back to one pane draws its grid tracked: rows are kept again.
	draw(&mut pen, &view(&shown));
	assert!(pen.keeps_rows());
	let next = frame(&format!("{}\x1b[2;2Hx", lines(0..5)));
	let damage = grid_damage(&shown, &next, true).expect("same size");
	pen.plan_rows(Some(damage), CELL.1);
	pen.begin_frame(SIZE);
	// The changed row, and the cursor's old row at the end of the text.
	let rows = pen.rows_to_draw().map(<[bool]>::to_vec);
	assert_eq!(rows, Some(vec![false, true, false, false, true]));
}
