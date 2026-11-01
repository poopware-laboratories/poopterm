//! Tests of how a frame's kept rows follow the plan over many frames: a
//! chain of partial frames, plans that do not fit the grid, a frame whose
//! grid never ends, the rows a scroll adds to those redrawn, and rows
//! showing images, drawn again with their frame's images.
use super::{
	quads::{Fill, QuadPen},
	rows_tests::{Drawn, assert_same, draw, lines, padded, pen, plain},
};
use crate::{
	graphics::{Bitmap, Placement},
	render::{
		damage::{GridDamage, grid_damage},
		grid::GridView,
		grid_case_tests::{CELL, SAMPLE, SIZE, frame, view},
	},
	term::Frame,
};
use std::sync::Arc;

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

/// `SAMPLE` with a small image in grid row `row`, at column 2.
fn pictured(row: isize) -> Frame {
	let mut sample = frame(SAMPLE);
	let bitmap = Arc::new(Bitmap::new(4, 4, vec![0xff_30_60_90; 16]));
	sample.images.push(Placement::natural(bitmap, 2, row));
	sample
}

/// The bitmap each image quad of `pen` shows, in order; `None` for an
/// index the frame's images lack.
fn shown(pen: &QuadPen) -> Vec<Option<*const Bitmap>> {
	let quads = pen.quads.iter().filter_map(|quad| match quad.fill {
		Fill::Image(index, _) => Some(index),
		_ => None,
	});
	let bitmap = |index: usize| {
		pen
			.images
			.get(index)
			.map(|image| Arc::as_ptr(&image.placement.bitmap))
	};
	quads.map(bitmap).collect()
}

#[test]
fn rows_showing_images_are_drawn_again_so_the_frame_lists_their_images() {
	// The frame's images are listed anew each frame, so a kept row's
	// image quad would point past them, or at another image.
	for row in [0, 1, 4] {
		let sample = pictured(row);
		let mut drawn = Drawn::new();
		drawn.draw(&view(&sample));
		let shows = shown(&drawn.pen);
		assert_eq!(shows.len(), 1, "row {row}");
		drawn.pen.plan_rows(Some(unchanged(sample.rows)), CELL.1);
		drawn.draw(&view(&sample));
		let mut redrawn = vec![false; sample.rows];
		redrawn[row as usize] = true;
		assert_eq!(drawn.pen.rows_to_draw(), Some(&redrawn[..]), "row {row}");
		assert_eq!(shown(&drawn.pen), shows, "row {row}");
		assert_same(&drawn, &view(&sample), &format!("image in row {row}"));
	}
}

#[test]
fn a_row_whose_image_scrolled_away_is_kept_again() {
	let sample = pictured(1);
	// Scrolled below the view: the grid still draws in layers, as it does
	// while a frame has images, but no row shows this one.
	let plain = pictured(sample.rows as isize + 2);
	let mut drawn = Drawn::new();
	drawn.draw(&view(&sample));
	let damage = grid_damage(&sample, &plain, true).expect("same size");
	drawn.pen.plan_rows(Some(damage), CELL.1);
	drawn.draw(&view(&plain));
	assert!(drawn.pen.images.is_empty());
	drawn.pen.plan_rows(Some(unchanged(plain.rows)), CELL.1);
	drawn.draw(&view(&plain));
	let kept = vec![false; plain.rows];
	assert_eq!(drawn.pen.rows_to_draw(), Some(&kept[..]));
	assert_same(&drawn, &view(&plain), "image gone");
	assert!(drawn.pen.images.is_empty());
}
