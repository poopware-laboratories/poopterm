//! Tests of keeping grid rows between frames: a frame that redraws only
//! the rows that changed, keeping the rest, draws the same quads as one
//! drawing every row, scrolled or not, with chrome around the grid; kept
//! rows keep their glyphs in the atlas; and a frame redraws every row
//! unless the pen holds the last one's.
use super::{
	atlas::Slot,
	quads::{Fill, Quad, QuadPen},
	quads_tests::Gpu,
};
use crate::{
	render::{
		damage::{GridDamage, grid_damage},
		grid::{GridView, draw_grid},
		grid_case_tests::{CELL, SAMPLE, SIZE, assert_paints_like_software, fonts, frame, view},
		pen::Pen,
	},
	term::Frame,
};

const TOP: u32 = 0x11_2233;
const BOTTOM: u32 = 0x44_5566;
/// Lines, each with its own background, five to a screen.
const LINES: [&str; 8] = [
	"\x1b[41mzero\x1b[K",
	"\x1b[42mone\x1b[K",
	"\x1b[43mtwo\x1b[K",
	"\x1b[44mthree\x1b[K",
	"\x1b[45mfour\x1b[K",
	"\x1b[46mfive\x1b[K",
	"\x1b[47msix\x1b[K",
	"\x1b[0;1mseven\x1b[K",
];

fn pen() -> QuadPen {
	QuadPen::new(SIZE, (256, 4))
}

fn plain(view: GridView) -> GridView {
	view
}

/// `view` with the whole window as padding, which the edge rows extend
/// their backgrounds into.
fn padded<'f>(view: GridView<'f>) -> GridView<'f> {
	let whole = crate::graphics::Rect {
		x: 0,
		y: 0,
		width: SIZE.0,
		height: SIZE.1,
	};
	GridView {
		padding: Some(whole),
		..view
	}
}

/// Draws a frame as the app does: chrome, the grid's rows the pen says to
/// draw, and chrome over it.
fn draw(pen: &mut QuadPen, view: &GridView) {
	pen.begin_frame(SIZE);
	pen.rect(0, 0, SIZE.0, 2, TOP);
	let rows = pen.rows_to_draw().map(<[bool]>::to_vec);
	draw_grid(pen, &mut fonts(), view, rows.as_deref());
	pen.rect(0, SIZE.1 - 2, SIZE.0, 2, BOTTOM);
}

/// A pen that drew frames, and the stand-in GPU it sent their glyphs to.
struct Drawn {
	pen: QuadPen,
	gpu: Gpu,
}

impl Drawn {
	fn new() -> Self {
		Self {
			pen: pen(),
			gpu: Gpu::default(),
		}
	}

	fn draw(&mut self, view: &GridView) {
		draw(&mut self.pen, view);
		self.gpu.upload(&self.pen.uploads);
	}
}

/// The quads with the atlas places of their bitmaps left out, as a pen
/// that drew other frames before lays its atlas out otherwise.
fn placed(pen: &QuadPen) -> Vec<Quad> {
	let unplaced = |slot: Slot| Slot {
		page: 0,
		x: 0,
		y: 0,
		..slot
	};
	let quads = pen.quads.iter().map(|quad| Quad {
		fill: match quad.fill {
			Fill::Coverage(slot) => Fill::Coverage(unplaced(slot)),
			Fill::Color(slot) => Fill::Color(unplaced(slot)),
			fill => fill,
		},
		..*quad
	});
	quads.collect()
}

/// Asserts that `drawn` holds the quads a new pen draws for `view` whole,
/// in the same order, and that the GPU paints the same pixels from them.
fn assert_same(drawn: &Drawn, view: &GridView, what: &str) {
	let mut whole = Drawn::new();
	whole.draw(view);
	assert_eq!(placed(&drawn.pen), placed(&whole.pen), "{what}");
	assert!(
		drawn.gpu.draw(&drawn.pen) == whole.gpu.draw(&whole.pen),
		"{what}: pixels differ"
	);
}

/// Draws `prev` whole, then `next` redrawing only the rows that changed.
fn partly(prev: &Frame, next: &Frame, allow_scroll: bool, look: fn(GridView) -> GridView) -> Drawn {
	let mut drawn = Drawn::new();
	drawn.draw(&look(view(prev)));
	assert!(drawn.pen.keeps_rows());
	let damage = grid_damage(prev, next, allow_scroll).expect("same size");
	drawn.pen.plan_rows(Some(damage), CELL.1);
	drawn.draw(&look(view(next)));
	drawn
}

fn lines(range: std::ops::Range<usize>) -> String {
	LINES[range].join("\r\n")
}

#[test]
fn a_frame_redrawing_changed_rows_draws_what_a_whole_frame_draws() {
	let prev = frame(SAMPLE);
	for (what, input) in [
		("unchanged", SAMPLE.to_owned()),
		("one cell", format!("{SAMPLE}\x1b[2;3Hx")),
		("two rows", format!("{SAMPLE}\x1b[1;1H\x1b[7mR\x1b[3;9Hq")),
		("cursor moved", format!("{SAMPLE}\x1b[1;1H")),
	] {
		let next = frame(&input);
		for look in [plain as fn(GridView) -> GridView, padded] {
			let drawn = partly(&prev, &next, true, look);
			assert_same(&drawn, &look(view(&next)), what);
			assert!(drawn.pen.keeps_rows());
		}
	}
}

#[test]
fn a_scrolled_frame_moves_the_kept_rows_up_and_draws_what_a_whole_frame_draws() {
	for scroll in 1..4 {
		let (prev, next) = (frame(&lines(0..5)), frame(&lines(0..5 + scroll)));
		for look in [plain as fn(GridView) -> GridView, padded] {
			let drawn = partly(&prev, &next, true, look);
			let redrawn = drawn
				.pen
				.rows_to_draw()
				.unwrap()
				.iter()
				.filter(|&&r| r)
				.count();
			assert_eq!(redrawn < 5, scroll < 3, "by {scroll}");
			assert_same(&drawn, &look(view(&next)), &format!("by {scroll}"));
		}
	}
}

#[test]
fn a_scroll_redraws_the_first_row_and_the_old_last_as_well() {
	let mut pen = pen();
	pen.plan_rows(None, CELL.1);
	draw(&mut pen, &view(&frame(&lines(0..5))));
	let mut rows = vec![false; 5];
	rows[4] = true;
	pen.plan_rows(Some(GridDamage { scroll: 1, rows }), CELL.1);
	pen.begin_frame(SIZE);
	assert_eq!(
		pen.rows_to_draw(),
		Some(&[true, false, false, true, true][..])
	);
}

#[test]
fn rows_left_alone_show_what_they_showed() {
	let (old, new) = (frame(&lines(0..5)), frame(&lines(2..7)));
	let mut pen = pen();
	draw(&mut pen, &view(&old));
	let mut rows = vec![false; 5];
	rows[2] = true;
	pen.plan_rows(Some(GridDamage { scroll: 0, rows }), CELL.1);
	draw(&mut pen, &view(&new));
	let whole = |frame: &Frame| {
		let mut pen = self::pen();
		draw(&mut pen, &view(frame));
		placed(&pen)
	};
	let (old, new, kept) = (whole(&old), whole(&new), placed(&pen));
	let row = |quads: &[Quad], row: usize| -> Vec<_> {
		let (top, bottom) = ((4 + row * CELL.1) as i32, (4 + (row + 1) * CELL.1) as i32);
		quads
			.iter()
			.filter(|q| q.at.1 >= top && q.at.1 < bottom)
			.copied()
			.collect()
	};
	for at in 0..5 {
		let expected = if at == 2 {
			row(&new, at)
		} else {
			row(&old, at)
		};
		assert!(!expected.is_empty());
		assert_eq!(row(&kept, at), expected, "row {at}");
	}
}

#[test]
fn the_chrome_stays_before_and_after_the_grid_when_no_row_is_redrawn() {
	let frame = frame(SAMPLE);
	let mut drawn = Drawn::new();
	drawn.draw(&view(&frame));
	let rows = vec![false; 5];
	drawn
		.pen
		.plan_rows(Some(GridDamage { scroll: 0, rows }), CELL.1);
	drawn.draw(&view(&frame));
	assert_eq!(drawn.pen.rows_to_draw(), Some(&[false; 5][..]));
	assert_eq!(drawn.pen.quads.first().map(|q| q.color), Some(TOP));
	assert_eq!(drawn.pen.quads.last().map(|q| q.color), Some(BOTTOM));
	assert_same(&drawn, &view(&frame), "nothing redrawn");
}

#[test]
fn kept_rows_paint_what_software_paints() {
	let (mut pen, mut gpu) = (pen(), Gpu::default());
	let (prev, next) = (frame(SAMPLE), frame(&format!("{SAMPLE}\x1b[2;1Hx")));
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts(), &view(&prev), None);
	gpu.upload(&pen.uploads);
	pen.plan_rows(grid_damage(&prev, &next, true), CELL.1);
	pen.begin_frame(SIZE);
	let rows = pen.rows_to_draw().map(<[bool]>::to_vec);
	draw_grid(&mut pen, &mut fonts(), &view(&next), rows.as_deref());
	gpu.upload(&pen.uploads);
	assert!(pen.complete);
	assert_paints_like_software(&view(&next), &gpu.draw(&pen), "kept rows");
}

#[test]
fn the_pages_of_kept_glyphs_are_never_emptied_for_new_ones() {
	// One 16-pixel page holds two letters. The two kept ones fill it, so
	// two more are refused rather than evicting them, which an emptied
	// page would hold.
	let mut pen = QuadPen::new(SIZE, (16, 1));
	let (prev, next) = (frame("\x1b[?25lab"), frame("\x1b[?25lab\r\ncd"));
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts(), &view(&prev), None);
	assert!(pen.complete && pen.keeps_rows());
	pen.plan_rows(grid_damage(&prev, &next, true), CELL.1);
	pen.begin_frame(SIZE);
	let rows = pen.rows_to_draw().map(<[bool]>::to_vec);
	assert_eq!(rows.as_deref().map(|rows| rows[0]), Some(false));
	draw_grid(&mut pen, &mut fonts(), &view(&next), rows.as_deref());
	assert!(!pen.complete, "the kept letters' page was emptied");
}

#[test]
fn a_redrawn_row_lets_the_page_of_its_old_glyphs_go() {
	// The letters a redrawn row showed are not kept: their full page is
	// emptied for its new ones.
	let mut pen = QuadPen::new(SIZE, (16, 1));
	let (prev, next) = (frame("\x1b[?25lab"), frame("\x1b[?25lcd"));
	pen.begin_frame(SIZE);
	draw_grid(&mut pen, &mut fonts(), &view(&prev), None);
	pen.plan_rows(grid_damage(&prev, &next, true), CELL.1);
	pen.begin_frame(SIZE);
	let rows = pen.rows_to_draw().map(<[bool]>::to_vec);
	assert_eq!(rows.as_deref().map(|rows| rows[0]), Some(true));
	draw_grid(&mut pen, &mut fonts(), &view(&next), rows.as_deref());
	assert!(pen.complete);
}

#[test]
fn a_frame_redraws_every_row_unless_the_pen_holds_the_last() {
	let frame = frame(SAMPLE);
	let damage = || {
		Some(GridDamage {
			scroll: 0,
			rows: vec![false; 5],
		})
	};
	// A new pen holds no rows.
	let mut pen = pen();
	assert!(!pen.keeps_rows());
	pen.plan_rows(damage(), CELL.1);
	pen.begin_frame(SIZE);
	assert_eq!(pen.rows_to_draw(), None);
	// A frame left without its grid holds none either.
	draw(&mut pen, &view(&frame));
	pen.plan_rows(damage(), CELL.1);
	pen.begin_frame(SIZE);
	assert!(!pen.keeps_rows());
	pen.plan_rows(damage(), CELL.1);
	pen.begin_frame(SIZE);
	assert_eq!(pen.rows_to_draw(), None);
	// Nor does one planned for another number of rows.
	draw(&mut pen, &view(&frame));
	pen.plan_rows(
		Some(GridDamage {
			scroll: 0,
			rows: vec![false; 4],
		}),
		CELL.1,
	);
	pen.begin_frame(SIZE);
	assert_eq!(pen.rows_to_draw(), None);
	// No plan redraws every row, and a plan holds for one frame.
	draw(&mut pen, &view(&frame));
	pen.plan_rows(None, CELL.1);
	pen.begin_frame(SIZE);
	assert_eq!(pen.rows_to_draw(), None);
	draw_grid(&mut pen, &mut fonts(), &view(&frame), None);
	pen.plan_rows(damage(), CELL.1);
	draw(&mut pen, &view(&frame));
	assert!(pen.rows_to_draw().is_some());
	draw(&mut pen, &view(&frame));
	assert_eq!(pen.rows_to_draw(), None);
}
