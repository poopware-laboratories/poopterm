//! Pixel equivalence of split tabs: every frame drawn in part, as the
//! canvas plans it, matches the same frame drawn whole, through typing,
//! scrolling, colored screens, highlights and moving focus.
use super::{Canvas, DrawKey, Plan, pane_keys};
use crate::{
	graphics::Rect,
	parser::Parser,
	render::{
		background::fill_inside, glyphs::FontSet, grid::GridView, painter::Painter, panes::draw_panes,
	},
	term::{Frame, Term},
	theme::{Theme, ThemeKind},
	ui::{
		panes::{Panes, Place, Split, pane_padding},
		selection::{CellPoint, Selection},
	},
};

const CELL: (usize, usize) = (10, 20);
const WINDOW: (usize, usize) = (420, 260);
/// The theme's background, which cells showing it leave to the window.
fn bg() -> u32 {
	Theme::from_kind(ThemeKind::Poop).bg
}
/// The grid area, 6 pixels in from the window padding.
const AREA: Rect = Rect {
	x: 8,
	y: 8,
	width: 404,
	height: 244,
};
const PADDING: Rect = Rect {
	x: 2,
	y: 2,
	width: 416,
	height: 256,
};

fn fonts() -> FontSet {
	FontSet::new(
		crate::fonts::Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap(),
		Vec::new(),
	)
}

/// A split tab: its layout, a terminal per pane, and a parser for each.
struct Tab {
	panes: Panes,
	terms: Vec<(u64, Term, Parser)>,
}

impl Tab {
	/// Panes 1 and 2 side by side, or with `three`, 2 split down into 3.
	fn new(three: bool) -> Self {
		let mut panes = Panes::new(1);
		panes.split(Split::Right, 2);
		if three {
			panes.split(Split::Down, 3);
		}
		let places = panes.places(AREA, CELL, 9);
		let terms = (places.iter())
			.map(|place| {
				let (cols, rows) = place.grid;
				let theme = Theme::from_kind(ThemeKind::Poop);
				(
					place.id,
					Term::new(cols, rows, theme, "/".into()),
					Parser::new(),
				)
			})
			.collect();
		Self { panes, terms }
	}

	fn places(&self) -> Vec<Place> {
		self.panes.places(AREA, CELL, 9)
	}

	fn feed(&mut self, id: u64, input: &str) {
		let (_, term, parser) = self
			.terms
			.iter_mut()
			.find(|(pane, ..)| *pane == id)
			.unwrap();
		parser.advance(term, input.as_bytes());
	}

	fn frames(&self) -> Vec<Frame> {
		(self.places().iter())
			.map(|place| {
				let (_, term, _) = self.terms.iter().find(|(id, ..)| *id == place.id).unwrap();
				let mut frame = Frame::default();
				term.snapshot(&mut frame);
				frame
			})
			.collect()
	}
}

fn views<'a>(places: &[Place], frames: &'a [Frame], key: DrawKey) -> Vec<GridView<'a>> {
	(places.iter().zip(frames))
		.map(|(place, frame)| {
			let pad = pane_padding(place.rect, AREA, PADDING);
			let window = GridView {
				frame,
				origin: (AREA.x, AREA.y),
				cell: CELL,
				px: 16.0,
				ascent: 15,
				scale: 1,
				default_bg: bg(),
				link_color: 0x12_3456,
				selection_colors: (0x11_1111, 0xff_ffff),
				match_colors: (0, 0xff_ff00),
				selection: key.selection,
				search_match: key.search_match,
				focused: key.focused,
				padding: None,
				dim: None,
			};
			window.in_pane(frame, place.rect, pad, place.id == key.session)
		})
		.collect()
}

/// Draws `views` into `pixels`: whole after clearing, or only `rows`.
fn paint(pixels: &mut [u32], fonts: &mut FontSet, views: &[GridView], rows: Option<&[Vec<bool>]>) {
	let mut painter = Painter::new(pixels, WINDOW.0, WINDOW.1);
	if rows.is_none() {
		painter.rect(0, 0, WINDOW.0, WINDOW.1, bg());
	}
	let restore = |p: &mut Painter, rect| fill_inside(p, rect, 0, (bg(), None));
	draw_panes(&mut painter, fonts, views, rows, restore);
}

/// Draws the tab's next frame as the canvas plans it, checks it against a
/// whole drawing, and returns the plan.
fn step(canvas: &mut Canvas, fonts: &mut FontSet, tab: &Tab, key: DrawKey) -> Plan {
	let (places, frames) = (tab.places(), tab.frames());
	let keys = pane_keys(&places, key);
	let plan = canvas.plan_panes(&frames, &keys, &places, WINDOW);
	let rows = match &plan {
		Plan::Full => None,
		Plan::Panes(rows) => Some(rows.as_slice()),
		Plan::Partial(_) => unreachable!("panes plan no single grid"),
	};
	let views = views(&places, &frames, key);
	paint(&mut canvas.pixels, fonts, &views, rows);
	let mut whole = vec![0; WINDOW.0 * WINDOW.1];
	paint(&mut whole, fonts, &views, None);
	let differ = (canvas.pixels.iter().zip(&whole))
		.filter(|(a, b)| a != b)
		.count();
	assert_eq!(differ, 0, "{differ} pixels differ after {plan:?}");
	canvas.remember_panes(&mut frames.clone(), keys, places);
	plan
}

fn focused(session: u64) -> DrawKey {
	DrawKey {
		session,
		focused: true,
		selection: None,
		search_match: None,
		overlay: false,
	}
}

fn partial(plan: &Plan) -> bool {
	matches!(plan, Plan::Panes(_))
}

#[test]
fn typing_in_one_pane_redraws_only_its_rows_and_matches_a_whole_frame() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(false));
	assert!(!partial(&step(&mut canvas, &mut fonts, &tab, focused(2))));
	for (id, input) in [
		(2, "$ ls"),
		(1, "left\r\n"),
		(2, "\r\nfile"),
		(1, "more text"),
	] {
		tab.feed(id, input);
		let plan = step(&mut canvas, &mut fonts, &tab, focused(2));
		let Plan::Panes(rows) = plan else {
			panic!("{input:?} drew everything");
		};
		let other = if id == 1 { 1 } else { 0 };
		assert!(
			rows[other].iter().all(|row| !row),
			"{input:?} touched the other pane"
		);
		assert!(
			rows[1 - other].iter().any(|row| *row),
			"{input:?} drew nothing"
		);
	}
}

#[test]
fn scrolling_panes_redraw_their_rows_and_match_a_whole_frame() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(true));
	step(&mut canvas, &mut fonts, &tab, focused(3));
	let lines: String = (0..30).map(|n| format!("line {n}\r\n")).collect();
	for id in [3, 1, 2] {
		tab.feed(id, &lines);
		assert!(partial(&step(&mut canvas, &mut fonts, &tab, focused(3))));
	}
}

#[test]
fn colored_screens_extend_into_the_padding_in_part_as_in_whole() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(true));
	step(&mut canvas, &mut fonts, &tab, focused(1));
	// A full-screen program paints its background, then lines change.
	tab.feed(1, "\x1b[44m\x1b[2J\x1b[Hblue");
	step(&mut canvas, &mut fonts, &tab, focused(1));
	tab.feed(3, "\x1b[42m\x1b[2J\x1b[5;1Hgreen");
	step(&mut canvas, &mut fonts, &tab, focused(1));
	tab.feed(1, "\x1b[3;3Hmore\x1b[0m\x1b[K");
	assert!(partial(&step(&mut canvas, &mut fonts, &tab, focused(1))));
	tab.feed(3, "\x1b[0m\x1b[2J");
	assert!(partial(&step(&mut canvas, &mut fonts, &tab, focused(1))));
}

#[test]
fn selecting_in_the_focused_pane_redraws_only_the_rows_it_covers() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(false));
	tab.feed(1, "one\r\ntwo\r\nthree");
	tab.feed(2, "four\r\nfive");
	step(&mut canvas, &mut fonts, &tab, focused(1));
	let selected = DrawKey {
		selection: Some(Selection {
			start: CellPoint { row: 1, col: 0 },
			end: CellPoint { row: 2, col: 2 },
			block: false,
		}),
		..focused(1)
	};
	let Plan::Panes(rows) = step(&mut canvas, &mut fonts, &tab, selected) else {
		panic!("a selection drew everything");
	};
	assert_eq!(&rows[0][..4], [false, true, true, false]);
	assert!(rows[1].iter().all(|row| !row));
	assert!(partial(&step(&mut canvas, &mut fonts, &tab, focused(1))));
}

#[test]
fn moving_focus_draws_a_whole_frame_and_then_parts_again() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(true));
	tab.feed(2, "top right");
	step(&mut canvas, &mut fonts, &tab, focused(3));
	assert!(!partial(&step(&mut canvas, &mut fonts, &tab, focused(2))));
	tab.feed(2, " typed");
	assert!(partial(&step(&mut canvas, &mut fonts, &tab, focused(2))));
	let unfocused_window = DrawKey {
		focused: false,
		..focused(2)
	};
	assert!(!partial(&step(
		&mut canvas,
		&mut fonts,
		&tab,
		unfocused_window
	)));
}

#[test]
fn closing_a_pane_draws_the_rest_whole() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(true));
	tab.feed(1, "big");
	step(&mut canvas, &mut fonts, &tab, focused(3));
	tab.panes.close(3);
	// Pane 2 grows into 3's room: its terminal is resized.
	let grid = tab
		.places()
		.iter()
		.find(|place| place.id == 2)
		.unwrap()
		.grid;
	let (_, term, _) = tab.terms.iter_mut().find(|(id, ..)| *id == 2).unwrap();
	term.resize(grid.0, grid.1);
	assert!(!partial(&step(&mut canvas, &mut fonts, &tab, focused(2))));
	tab.feed(2, "grown");
	assert!(partial(&step(&mut canvas, &mut fonts, &tab, focused(2))));
}

#[test]
fn erasing_and_rewriting_lines_matches_a_whole_frame() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(false));
	tab.feed(1, "first line\r\nsecond line\r\nthird line");
	tab.feed(2, "right side");
	step(&mut canvas, &mut fonts, &tab, focused(1));
	for input in [
		"\x1b[2;1H\x1b[2K",
		"\x1b[1;7Hthing",
		"\x1b[3;1H\x1b[1K",
		"\x1b[H\x1b[J",
		"again",
	] {
		tab.feed(1, input);
		assert!(
			partial(&step(&mut canvas, &mut fonts, &tab, focused(1))),
			"{input:?}"
		);
	}
}

#[test]
fn scroll_regions_and_inserted_lines_match_a_whole_frame() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(true));
	let lines: String = (0..12).map(|n| format!("row {n}\r\n")).collect();
	tab.feed(2, &lines);
	step(&mut canvas, &mut fonts, &tab, focused(2));
	for input in [
		"\x1b[2;4r\x1b[4;1H\n\n",
		"\x1b[r\x1b[2;1H\x1b[2L",
		"\x1b[3;1H\x1b[M",
	] {
		tab.feed(2, input);
		step(&mut canvas, &mut fonts, &tab, focused(2));
	}
}

#[test]
fn the_alternate_screen_and_back_match_a_whole_frame() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(false));
	tab.feed(2, "$ prompt");
	step(&mut canvas, &mut fonts, &tab, focused(2));
	tab.feed(2, "\x1b[?1049h\x1b[41m\x1b[2J\x1b[Hfull screen");
	step(&mut canvas, &mut fonts, &tab, focused(2));
	tab.feed(2, "\x1b[0m\x1b[?1049l");
	step(&mut canvas, &mut fonts, &tab, focused(2));
	tab.feed(1, "still here");
	assert!(partial(&step(&mut canvas, &mut fonts, &tab, focused(2))));
}

#[test]
fn wide_characters_and_cursor_moves_match_a_whole_frame() {
	let (mut canvas, mut fonts, mut tab) = (Canvas::new(), fonts(), Tab::new(true));
	step(&mut canvas, &mut fonts, &tab, focused(1));
	for (id, input) in [
		(1, "日本語のテキスト"),
		(3, "\x1b[5;5H*\x1b[1;1H"),
		(1, "\x1b[1;3Hx"),
		(2, "\x1b[?25l"),
		(2, "\x1b[?25h\x1b[3;3H"),
	] {
		tab.feed(id, input);
		step(&mut canvas, &mut fonts, &tab, focused(1));
	}
}
