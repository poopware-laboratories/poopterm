//! Tests of drawing split panes: whole panes reach the pen with no grid
//! rows to keep, partial ones restore exactly the strips they redraw, and
//! the repainted pixel rows are reported.
use super::{
	glyphs::Glyph,
	grid::GridView,
	grid_case_tests::{fonts, frame, view},
	panes::draw_panes,
	pen::{Ink, Pen},
};
use crate::graphics::{Placement, Rect};

/// What a pen was asked to do: grid rows begun and ended, and how many
/// rects, glyphs and images it drew.
#[derive(Default, Debug)]
struct Record {
	rows: Vec<usize>,
	grids: Vec<usize>,
	rects: usize,
	glyphs: usize,
}

impl Pen for Record {
	fn size(&self) -> (usize, usize) {
		(600, 200)
	}
	fn rect(&mut self, _: usize, _: usize, _: usize, _: usize, _: u32) {
		self.rects += 1;
	}
	fn round_rect(&mut self, _: (usize, usize, usize, usize), _: usize, _: u32) {}
	fn glyph(&mut self, _: &Glyph, _: Ink, _: (i32, i32), _: (i32, i32, i32, i32), _: bool, _: u32) {
		self.glyphs += 1;
	}
	fn placement(&mut self, _: Rect, _: &Placement, _: (isize, isize), _: (usize, usize)) {}
	fn begin_row(&mut self, row: usize) {
		self.rows.push(row);
	}
	fn end_grid(&mut self, rows: usize) {
		self.grids.push(rows);
	}
}

fn rect(x: usize, y: usize, width: usize, height: usize) -> Rect {
	Rect {
		x,
		y,
		width,
		height,
	}
}

/// Two 30 by 5 grids of 8 by 16 cells side by side, the left one with
/// 4 pixels of padding above and 12 below.
fn views<'a>(left: &'a crate::term::Frame, right: &'a crate::term::Frame) -> [GridView<'a>; 2] {
	[
		view(left).in_pane(left, rect(4, 10, 240, 80), rect(0, 6, 244, 96), true),
		view(right).in_pane(right, rect(253, 10, 240, 80), rect(253, 10, 240, 80), false),
	]
}

#[test]
fn whole_panes_draw_every_row_but_tell_the_pen_of_none() {
	let (left, right) = (frame("left"), frame("right"));
	let mut pen = Record::default();
	let mut restored = Vec::new();
	let bands = draw_panes(
		&mut pen,
		&mut fonts(),
		&views(&left, &right),
		None,
		|_, strip| restored.push(strip),
	);
	assert!(pen.rows.is_empty(), "rows begun: {:?}", pen.rows);
	assert!(pen.grids.is_empty(), "grids ended: {:?}", pen.grids);
	assert!(pen.glyphs >= "leftright".len(), "text drawn");
	assert!(restored.is_empty(), "a whole frame was cleared already");
	assert!(bands.is_empty());
}

#[test]
fn partial_panes_draw_only_their_changed_rows() {
	let (left, right) = (frame("a\r\nb\r\nc"), frame("d"));
	let mut pen = Record::default();
	let rows = [vec![false, true, false, false, true], vec![false; 5]];
	draw_panes(
		&mut pen,
		&mut fonts(),
		&views(&left, &right),
		Some(&rows),
		|_, _| {},
	);
	// Only the left pane's rows 1 and 4 are drawn; each grid ends.
	assert_eq!(pen.rows, [1, 4]);
	assert_eq!(pen.grids, [5, 5]);
}

#[test]
fn partial_panes_restore_their_padding_and_changed_rows_first() {
	let (left, right) = (frame("a"), frame("b"));
	let rows = [
		vec![true, false, false, false, false],
		vec![false, false, true, false, false],
	];
	let mut restored = Vec::new();
	let bands = draw_panes(
		&mut Record::default(),
		&mut fonts(),
		&views(&left, &right),
		Some(&rows),
		|_, strip| restored.push(strip),
	);
	let expected = [
		// The left pane: 4 pixels above, 12 below, and row 0.
		rect(0, 6, 244, 4),
		rect(0, 90, 244, 12),
		rect(0, 10, 244, 16),
		// The right pane fills its padding: nothing around it, only row 2.
		rect(253, 42, 240, 16),
	];
	assert_eq!(restored, expected);
	let reported: Vec<_> = expected.iter().map(|r| (r.y, r.y + r.height)).collect();
	assert_eq!(bands, reported);
}

#[test]
fn bands_stop_at_the_bottom_of_the_surface() {
	let shown = frame("x");
	let low = [view(&shown).in_pane(&shown, rect(0, 150, 240, 80), rect(0, 150, 240, 80), true)];
	let rows = [vec![false, false, false, true, true]];
	let bands = draw_panes(
		&mut Record::default(),
		&mut fonts(),
		&low,
		Some(&rows),
		|_, _| {},
	);
	// Row 3 starts at 198, row 4 and the padding below past the pen's
	// 200 pixels: they paint nothing, so they are neither restored nor
	// reported, and no band ends before it starts.
	assert_eq!(bands, [(198, 200)]);
}

#[test]
fn panes_without_rows_planned_are_drawn_whole() {
	// A plan for fewer panes than there are draws the rest whole, with no
	// grid rows for the pen to keep.
	let (left, right) = (frame("a"), frame("b"));
	let mut pen = Record::default();
	let rows = [vec![false; 5]];
	draw_panes(
		&mut pen,
		&mut fonts(),
		&views(&left, &right),
		Some(&rows),
		|_, _| {},
	);
	assert_eq!(pen.grids, [5], "only the planned pane's grid");
	assert!(pen.glyphs >= 1, "the other pane is drawn");
}

#[test]
fn a_view_without_padding_restores_around_its_grid_only() {
	let shown = frame("x");
	let bare = [GridView {
		padding: None,
		..view(&shown).in_pane(&shown, rect(10, 20, 240, 80), rect(0, 0, 0, 0), true)
	}];
	let rows = [vec![true, false, false, false, false]];
	let mut restored = Vec::new();
	draw_panes(
		&mut Record::default(),
		&mut fonts(),
		&bare,
		Some(&rows),
		|_, strip| restored.push(strip),
	);
	// The grid fills its own rect: only the row is restored.
	assert_eq!(restored, [rect(10, 20, 240, 16)]);
}
