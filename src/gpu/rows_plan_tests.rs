//! Tests of how a frame's kept rows follow the plan over many frames: a
//! chain of partial frames, plans that do not fit the grid, a frame whose
//! grid never ends, and the rows a scroll adds to those redrawn.
use super::rows_tests::{Drawn, assert_same, draw, lines, padded, pen, plain};
use crate::render::{
	damage::{GridDamage, grid_damage},
	grid::GridView,
	grid_case_tests::{CELL, SAMPLE, SIZE, frame, view},
};

/// The rows the pen redraws when planned `damage` after a whole frame of
/// `SAMPLE`, as `plan_rows` adds those a scroll leaves wrong.
fn redrawn(damage: GridDamage) -> Option<Vec<bool>> {
	let mut pen = pen();
	draw(&mut pen, &view(&frame(SAMPLE)));
	pen.plan_rows(Some(damage), CELL.1);
	pen.begin_frame(SIZE);
	pen.rows_to_draw().map(<[bool]>::to_vec)
}

/// A plan redrawing none of `rows` rows.
fn unchanged(rows: usize) -> GridDamage {
	GridDamage {
		scroll: 0,
		rows: vec![false; rows],
	}
}

#[test]
fn partial_frames_one_after_another_each_draw_what_a_whole_frame_draws() {
	// Each frame keeps rows the frame before kept or redrew in part.
	let steps = [
		SAMPLE.to_owned(),
		format!("{SAMPLE}\x1b[2;3Hx"),
		format!("{SAMPLE}\x1b[2;3Hy\x1b[4;1Hz"),
		format!("{SAMPLE}\x1b[2;3Hy\x1b[4;1Hz\x1b[1;5Hw"),
		format!("{SAMPLE}\x1b[2;3Hy\x1b[4;1Hz\x1b[1;5Hw"),
	];
	for look in [plain as fn(GridView) -> GridView, padded] {
		let mut drawn = Drawn::new();
		let mut prev = frame(&steps[0]);
		drawn.draw(&look(view(&prev)));
		for (at, input) in steps.iter().enumerate().skip(1) {
			let next = frame(input);
			let damage = grid_damage(&prev, &next, true).expect("same size");
			drawn.pen.plan_rows(Some(damage), CELL.1);
			drawn.draw(&look(view(&next)));
			assert_same(&drawn, &look(view(&next)), &format!("step {at}"));
			prev = next;
		}
	}
}

#[test]
fn scrolls_one_after_another_each_draw_what_a_whole_frame_draws() {
	let mut drawn = Drawn::new();
	let mut prev = frame(&lines(0..5));
	drawn.draw(&padded(view(&prev)));
	for end in 6..=8 {
		let next = frame(&lines(0..end));
		let damage = grid_damage(&prev, &next, true).expect("same size");
		assert!(damage.scroll > 0, "line {end} scrolls");
		drawn.pen.plan_rows(Some(damage), CELL.1);
		drawn.draw(&padded(view(&next)));
		assert_same(&drawn, &padded(view(&next)), &format!("{end} lines"));
		prev = next;
	}
}

#[test]
fn a_plan_for_another_number_of_rows_redraws_every_row() {
	let rows = frame(SAMPLE).rows;
	for other in [0, 1, rows - 1, rows + 1] {
		let damage = GridDamage {
			scroll: 0,
			rows: vec![false; other],
		};
		assert_eq!(redrawn(damage), None, "{other} rows planned for {rows}");
	}
}

#[test]
fn a_plan_that_does_not_fit_still_draws_what_a_whole_frame_draws() {
	let next = frame(&format!("{SAMPLE}\x1b[2;3Hx"));
	let mut drawn = Drawn::new();
	drawn.draw(&view(&frame(SAMPLE)));
	let damage = GridDamage {
		scroll: 0,
		rows: vec![false; next.rows + 1],
	};
	drawn.pen.plan_rows(Some(damage), CELL.1);
	drawn.draw(&view(&next));
	assert_same(&drawn, &view(&next), "plan of the wrong size");
	assert!(drawn.pen.keeps_rows());
}

#[test]
fn a_frame_whose_grid_never_ends_leaves_the_next_to_redraw_every_row() {
	let sample = frame(SAMPLE);
	let rows = sample.rows;
	let mut drawn = Drawn::new();
	drawn.draw(&view(&sample));
	drawn.pen.plan_rows(Some(unchanged(rows)), CELL.1);
	// Chrome alone: the frame is abandoned before its grid.
	drawn.pen.begin_frame(SIZE);
	assert!(!drawn.pen.keeps_rows());
	drawn.pen.plan_rows(Some(unchanged(rows)), CELL.1);
	drawn.draw(&view(&sample));
	assert_eq!(drawn.pen.rows_to_draw(), None);
	assert_same(&drawn, &view(&sample), "after an abandoned frame");
}

#[test]
fn an_unplanned_frame_after_kept_rows_redraws_every_row() {
	let sample = frame(SAMPLE);
	let mut drawn = Drawn::new();
	drawn.draw(&view(&sample));
	drawn.draw(&view(&sample));
	assert_eq!(drawn.pen.rows_to_draw(), None);
	assert_same(&drawn, &view(&sample), "unplanned");
}

#[test]
fn a_scroll_also_redraws_the_first_row_and_the_old_last_one() {
	let rows = frame(SAMPLE).rows;
	let marked = |marks: &[usize]| {
		let mut redraw = vec![false; rows];
		marks.iter().for_each(|&row| redraw[row] = true);
		Some(redraw)
	};
	for (scroll, expected) in [
		(0, marked(&[])),
		(1, marked(&[0, rows - 2])),
		(2, marked(&[0, rows - 3])),
		(rows - 1, marked(&[0])),
		// Every row scrolled off: none is left to carry the padding below.
		(rows, marked(&[0])),
	] {
		let damage = GridDamage {
			scroll,
			rows: vec![false; rows],
		};
		assert_eq!(redrawn(damage), expected, "scroll {scroll}");
	}
}

#[test]
fn a_scroll_keeps_the_rows_it_was_told_to_redraw() {
	let rows = frame(SAMPLE).rows;
	let mut damage = GridDamage {
		scroll: 1,
		rows: vec![false; rows],
	};
	damage.rows[2] = true;
	let redraw = redrawn(damage).expect("planned");
	assert!(redraw[2]);
	assert_eq!(redraw.iter().filter(|&&row| row).count(), 3);
}
