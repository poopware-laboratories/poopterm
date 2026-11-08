//! Tests of the untracked pen: it draws what the pen under it would, and
//! hides where grid rows begin and end, so a GPU pen keeps no rows of a
//! split tab's grids.
use super::{
	glyphs::Glyph,
	grid::draw_grid,
	grid_case_tests::{BG, SIZE, assert_paints_like_software, cases, fonts},
	painter::Painter,
	pen::{Ink, Pen, Untracked},
};
use crate::graphics::{Placement, Rect};

/// Counts the calls a pen gets, of each kind.
#[derive(Default, Debug, PartialEq)]
struct Calls {
	rects: usize,
	round_rects: usize,
	glyphs: usize,
	placements: usize,
	rows: Vec<usize>,
	grids: Vec<usize>,
}

impl Pen for Calls {
	fn size(&self) -> (usize, usize) {
		(17, 9)
	}
	fn rect(&mut self, _: usize, _: usize, _: usize, _: usize, _: u32) {
		self.rects += 1;
	}
	fn round_rect(&mut self, _: (usize, usize, usize, usize), _: usize, _: u32) {
		self.round_rects += 1;
	}
	fn glyph(&mut self, _: &Glyph, _: Ink, _: (i32, i32), _: (i32, i32, i32, i32), _: bool, _: u32) {
		self.glyphs += 1;
	}
	fn placement(&mut self, _: Rect, _: &Placement, _: (isize, isize), _: (usize, usize)) {
		self.placements += 1;
	}
	fn begin_row(&mut self, row: usize) {
		self.rows.push(row);
	}
	fn end_grid(&mut self, rows: usize) {
		self.grids.push(rows);
	}
}

#[test]
fn the_grid_through_an_untracked_pen_paints_the_same_pixels() {
	for case in cases() {
		let view = case.view();
		let mut pixels = vec![BG; SIZE.0 * SIZE.1];
		let mut painter = Painter::new(&mut pixels, SIZE.0, SIZE.1);
		draw_grid(&mut Untracked(&mut painter), &mut fonts(), &view, None);
		assert_paints_like_software(&view, &pixels, &case.name);
	}
}

#[test]
fn rows_and_grid_ends_do_not_reach_the_pen_but_drawing_does() {
	for case in cases() {
		let view = case.view();
		let mut tracked = Calls::default();
		draw_grid(&mut tracked, &mut fonts(), &view, None);
		assert_eq!(tracked.grids, [view.frame.rows], "{}", case.name);
		assert_eq!(tracked.rows.len(), view.frame.rows, "{}", case.name);
		let mut calls = Calls::default();
		draw_grid(&mut Untracked(&mut calls), &mut fonts(), &view, None);
		let expected = Calls {
			rows: Vec::new(),
			grids: Vec::new(),
			..tracked
		};
		assert_eq!(calls, expected, "{}", case.name);
	}
}

#[test]
fn every_kind_of_call_is_forwarded_with_the_size() {
	let mut calls = Calls::default();
	let mut pen = Untracked(&mut calls);
	assert_eq!(pen.size(), (17, 9));
	pen.rect(1, 2, 3, 4, 5);
	pen.round_rect((1, 2, 3, 4), 2, 5);
	pen.begin_row(3);
	pen.end_grid(4);
	assert_eq!(
		calls,
		Calls {
			rects: 1,
			round_rects: 1,
			..Calls::default()
		}
	);
}
