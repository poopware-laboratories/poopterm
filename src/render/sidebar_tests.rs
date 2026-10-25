//! Pixel tests of the sidebar: its background and edge, rows drawn like
//! header tabs, numbered rail tabs, and the buttons.
use super::{
	blend::blend,
	glyphs::FontSet,
	header::{Badge, HeaderButton, HeaderColors, HeaderTab, Icon, Rect, Style, draw_tab},
	label::Label,
	sidebar::{Sidebar, SidebarRow, draw_sidebar, rail_close_rect},
};
use crate::{
	fonts::{BUNDLED, Font},
	render::{
		painter::Painter,
		scene::{Scene, paint},
	},
};

const WIDTH: usize = 220;
const HEIGHT: usize = 200;
const BACKGROUND: u32 = 0x000000;
const COLORS: HeaderColors = HeaderColors {
	bar: 0x202020,
	separator: 0x505050,
	hover: 0x303030,
	raised: 0x404040,
	fg: 0xffffff,
	dim: 0x808080,
};

fn style(scale: usize) -> Style {
	Style {
		scale,
		px: 13.0 * scale as f32,
		colors: COLORS,
	}
}

fn row(number: usize, rect: Rect) -> SidebarRow<'static> {
	SidebarRow {
		number,
		rect,
		tab: HeaderTab {
			x: rect.0,
			width: rect.2,
			label: Label {
				title: "shell",
				status: None,
				colors: (COLORS.fg, COLORS.dim),
			},
			fill: None,
			badge: None,
			progress: None,
			close: None,
		},
	}
}

/// A sidebar 160 wide and 180 tall at `(10, 10)`.
fn sidebar(rail: bool, rows: Vec<SidebarRow<'static>>) -> Sidebar<'static> {
	Sidebar {
		area: (10, 10, 160, 180),
		style: style(1),
		rail,
		rows,
		buttons: Vec::new(),
	}
}

fn fonts() -> FontSet {
	FontSet::new(Font::from_bytes(BUNDLED, 0).unwrap(), Vec::new())
}

struct Drawn(Vec<u32>);

impl Drawn {
	fn at(&self, x: usize, y: usize) -> u32 {
		self.0[y * WIDTH + x]
	}

	/// The columns and rows of `rect` holding text ink, lighter than any
	/// surface.
	fn ink(&self, (x, y, w, h): Rect) -> Option<(Rect, usize)> {
		let light = |x, y| (self.at(x, y) & 0xff) > 0x60;
		let points: Vec<_> = (y..y + h)
			.flat_map(|row| (x..x + w).map(move |column| (column, row)))
			.filter(|&(column, row)| light(column, row))
			.collect();
		let xs = points.iter().map(|point| point.0);
		let ys = points.iter().map(|point| point.1);
		let (left, top) = (xs.clone().min()?, ys.clone().min()?);
		let (right, bottom) = (xs.max()?, ys.max()?);
		Some((
			(left, top, right - left + 1, bottom - top + 1),
			points.len(),
		))
	}
}

fn draw(sidebar: &Sidebar) -> Drawn {
	let mut pixels = vec![BACKGROUND; WIDTH * HEIGHT];
	let p = &mut Painter::new(&mut pixels, WIDTH, HEIGHT);
	paint(p, &mut fonts(), |scene, fonts| {
		draw_sidebar(scene, fonts, sidebar)
	});
	Drawn(pixels)
}

#[test]
fn the_bar_fills_the_area_with_a_separator_down_its_right_edge() {
	let drawn = draw(&sidebar(false, Vec::new()));
	for y in [10, 100, 189] {
		assert_eq!(drawn.at(10, y), COLORS.bar, "left edge, row {y}");
		assert_eq!(drawn.at(168, y), COLORS.bar, "before the edge, row {y}");
		assert_eq!(drawn.at(169, y), COLORS.separator, "edge, row {y}");
		assert_eq!(drawn.at(170, y), BACKGROUND, "past the edge, row {y}");
		assert_eq!(drawn.at(9, y), BACKGROUND);
	}
	assert_eq!(
		(drawn.at(50, 9), drawn.at(50, 190)),
		(BACKGROUND, BACKGROUND)
	);
}

#[test]
fn the_separator_widens_with_the_scale() {
	let wide = Sidebar {
		style: style(2),
		..sidebar(false, Vec::new())
	};
	let drawn = draw(&wide);
	assert_eq!(drawn.at(167, 50), COLORS.bar);
	assert_eq!(
		(drawn.at(168, 50), drawn.at(169, 50)),
		(COLORS.separator, COLORS.separator)
	);
}

#[test]
fn full_rows_are_drawn_exactly_as_header_tabs_on_their_surface() {
	let mut rows = vec![row(1, (16, 18, 148, 28)), row(2, (16, 50, 148, 28))];
	rows[0].tab.fill = Some(COLORS.raised);
	rows[0].tab.close = Some(true);
	rows[1].tab.badge = Some(Badge::Dot(0x00ff00));
	rows[1].tab.progress = Some((Some(50), 0x0000ff));
	let sidebar = sidebar(false, rows);
	let drawn = draw(&sidebar);
	let mut want = vec![BACKGROUND; WIDTH * HEIGHT];
	let mut fonts = fonts();
	let mut p = Scene::new(WIDTH, HEIGHT);
	p.rect(10, 10, 160, 180, COLORS.bar);
	p.rect(169, 10, 1, 180, COLORS.separator);
	for row in &sidebar.rows {
		draw_tab(&mut p, &mut fonts, style(1), row.rect, &row.tab);
	}
	p.replay(&mut Painter::new(&mut want, WIDTH, HEIGHT), &mut fonts);
	assert!(drawn.0 == want, "rows differ from header tabs");
}

/// A rail sidebar holding one row at `RAIL_ROW`.
const RAIL_ROW: Rect = (16, 18, 52, 36);

fn rail(edit: impl FnOnce(&mut SidebarRow)) -> Drawn {
	let mut row = row(7, RAIL_ROW);
	edit(&mut row);
	draw(&sidebar(true, vec![row]))
}

#[test]
fn rail_tabs_show_their_number_centered_instead_of_the_title() {
	let drawn = rail(|_| {});
	let ((x, y, w, h), _) = drawn.ink(RAIL_ROW).expect("the number is drawn");
	let (cx, cy) = (2 * x + w, 2 * y + h);
	// Centered on the row within a pixel, in doubled coordinates.
	assert!(cx.abs_diff(2 * 16 + 52) <= 2, "x {x} w {w}");
	assert!(cy.abs_diff(2 * 18 + 36) <= 3, "y {y} h {h}");
	assert!(w < 12, "one digit, not the title: {w} wide");
}

#[test]
fn two_digit_numbers_stay_centered() {
	let drawn = rail(|row| row.number = 12);
	let ((x, _, w, _), _) = drawn.ink(RAIL_ROW).unwrap();
	assert!((2 * x + w).abs_diff(2 * 16 + 52) <= 2);
	assert!(w > 12, "two digits: {w} wide");
}

#[test]
fn a_rail_tab_fill_is_a_rounded_surface() {
	let drawn = rail(|row| row.tab.fill = Some(COLORS.raised));
	assert_eq!(
		drawn.at(16, 18),
		COLORS.bar,
		"the rounded corner shows the bar"
	);
	assert_eq!(drawn.at(16, 36), COLORS.raised, "the left edge");
	assert_eq!(drawn.at(67, 36), COLORS.raised, "the right edge");
	assert_eq!(drawn.at(30, 53), COLORS.raised, "the bottom row");
	assert_eq!(drawn.at(30, 54), COLORS.bar, "below the row");
	let plain = rail(|_| {});
	assert_eq!(plain.at(30, 20), COLORS.bar);
}

#[test]
fn every_badge_on_the_rail_is_a_dot_left_of_the_number() {
	for badge in [Badge::Dot(0x00ff00), Badge::Bell(0x00ff00)] {
		let drawn = rail(|row| row.tab.badge = Some(badge));
		// A dot 6 wide at 8 from the left, centered down the row.
		assert_eq!(drawn.at(16 + 11, 18 + 18), 0x00ff00, "{badge:?}");
		assert_eq!(drawn.at(16 + 7, 18 + 18), COLORS.bar, "{badge:?}");
		assert_eq!(drawn.at(16 + 11, 18 + 14), COLORS.bar, "{badge:?}");
		let (number, _) = drawn.ink(RAIL_ROW).unwrap();
		assert!(number.0 > 16 + 14, "the dot is clear of the number");
	}
}

#[test]
fn the_rail_close_mark_shows_in_its_corner_only_when_asked() {
	let close = rail_close_rect(RAIL_ROW, 1);
	assert_eq!(close, (16 + 52 - 18, 20, 16, 16));
	assert!(rail(|_| {}).ink(close).is_none());
	let drawn = rail(|row| row.tab.close = Some(false));
	let (mark, _) = drawn.ink(close).expect("the × is drawn");
	let (cx, cy) = (2 * mark.0 + mark.2, 2 * mark.1 + mark.3);
	assert!(cx.abs_diff(2 * close.0 + close.2) <= 2, "{mark:?}");
	assert!(cy.abs_diff(2 * close.1 + close.3) <= 3, "{mark:?}");
	// Without hover, nothing lights up behind it.
	assert_eq!(drawn.at(close.0 + 2, close.1 + 8), COLORS.bar);
}

#[test]
fn a_hovered_rail_close_button_lights_up_from_the_tab_beneath() {
	let drawn = rail(|row| {
		row.tab.fill = Some(COLORS.hover);
		row.tab.close = Some(true);
	});
	let (x, y, size, _) = rail_close_rect(RAIL_ROW, 1);
	let light = blend(COLORS.hover, COLORS.fg, 40);
	assert_eq!(drawn.at(x + 2, y + size / 2), light);
	assert_eq!(drawn.at(x, y), COLORS.hover, "the light is round");
}

#[test]
fn rail_progress_runs_along_the_row_bottom() {
	let drawn = rail(|row| row.tab.progress = Some((Some(50), 0x0000ff)));
	let row = 18 + 36 - 3;
	let track = 52 - 24;
	assert_eq!(drawn.at(16 + 12, row), 0x0000ff);
	assert_eq!(drawn.at(16 + 12 + track / 2 - 1, row), 0x0000ff);
	assert_eq!(
		drawn.at(16 + 12 + track / 2, row),
		blend(COLORS.bar, 0x0000ff, 60)
	);
	assert_eq!(drawn.at(16 + 11, row), COLORS.bar);
}

#[test]
fn the_rail_close_button_doubles_at_twice_the_scale() {
	let row = (12, 16, 104, 72);
	assert_eq!(rail_close_rect(row, 2), (80, 20, 32, 32));
	// It keeps inside a row however narrow.
	assert_eq!(rail_close_rect((0, 0, 10, 10), 1).0, 0);
}

#[test]
fn buttons_are_drawn_over_the_bar() {
	let mut sidebar = sidebar(false, Vec::new());
	sidebar.buttons = vec![
		HeaderButton {
			rect: (40, 150, 32, 28),
			icon: Icon::Plus,
			hovered: true,
			enabled: true,
		},
		HeaderButton {
			rect: (80, 150, 32, 28),
			icon: Icon::Menu,
			hovered: false,
			enabled: true,
		},
	];
	let drawn = draw(&sidebar);
	assert_eq!(
		drawn.at(44, 152),
		COLORS.hover,
		"the hovered button lights up"
	);
	assert_eq!(drawn.at(84, 152), COLORS.bar);
	assert_eq!(drawn.at(56, 164), COLORS.fg, "the plus");
	assert_eq!(drawn.at(96, 159), COLORS.fg, "the menu's top bar");
}

/// A full row of `height` at `y`, with a dot badge and a close button.
fn full_row(y: usize, height: usize) -> SidebarRow<'static> {
	let mut row = row(1, (16, y, 148, height));
	row.tab.badge = Some(Badge::Dot(0x00ff00));
	row.tab.close = Some(false);
	row
}

#[test]
fn a_row_of_any_height_centers_its_title_dot_and_close_mark_in_itself() {
	for (y, height) in [(12, 24), (40, 28), (20, 36), (60, 48)] {
		let drawn = draw(&sidebar(false, vec![full_row(y, height)]));
		let middle = 2 * y + height;
		let title = (40, y, 80, height);
		let ((_, top, _, h), _) = drawn.ink(title).expect("the title is drawn");
		assert!(
			(2 * top + h).abs_diff(middle) <= 4,
			"title in a row {height} tall at {y}"
		);
		let dots: Vec<_> = (y..y + height)
			// Antialiased, the dot's edge pixels blend into the bar.
			.filter(|&row| drawn.at(31, row) != COLORS.bar)
			.collect();
		assert_eq!(dots.len(), 6, "a dot 6 tall in a row {height} tall");
		assert_eq!(dots[0] + dots[5], middle - 1, "the dot is centered");
		let (cx, cy, size, _) = super::header::close_rect(16, 148, (y, height), 1);
		let ((_, top, _, h), _) = drawn.ink((cx, y, size, height)).expect("the × is drawn");
		assert!(
			(2 * top + h).abs_diff(2 * cy + size) <= 3,
			"× in a row {height} tall"
		);
	}
}

#[test]
fn rows_draw_nothing_outside_their_surface() {
	let mut row = full_row(40, 28);
	row.tab.fill = Some(COLORS.raised);
	row.tab.progress = Some((None, 0x0000ff));
	let drawn = draw(&sidebar(false, vec![row]));
	for x in 10..169 {
		for y in (10..40).chain(68..190) {
			assert_eq!(drawn.at(x, y), COLORS.bar, "({x}, {y})");
		}
	}
	for y in 40..68 {
		assert_eq!(
			(drawn.at(15, y), drawn.at(164, y)),
			(COLORS.bar, COLORS.bar)
		);
	}
}

#[test]
fn unknown_progress_fills_the_track_dimmer_than_known_progress() {
	let track_row = 18 + 36 - 3;
	let unknown = rail(|row| row.tab.progress = Some((None, 0x0000ff)));
	let full = rail(|row| row.tab.progress = Some((Some(100), 0x0000ff)));
	let dim = blend(COLORS.bar, 0x0000ff, 140);
	for x in 28..56 {
		assert_eq!(unknown.at(x, track_row), dim, "column {x}");
		assert_eq!(full.at(x, track_row), 0x0000ff, "column {x}");
	}
	let none = rail(|row| row.tab.progress = Some((Some(0), 0x0000ff)));
	assert_eq!(none.at(28, track_row), blend(COLORS.bar, 0x0000ff, 60));
	assert_eq!(none.at(28, track_row + 1), blend(COLORS.bar, 0x0000ff, 60));
	assert_eq!(none.at(28, track_row + 2), COLORS.bar, "two rows thick");
}

#[test]
fn rail_progress_sits_on_the_tab_fill() {
	let drawn = rail(|row| {
		row.tab.fill = Some(COLORS.raised);
		row.tab.progress = Some((Some(0), 0x0000ff));
	});
	assert_eq!(drawn.at(30, 18 + 33), blend(COLORS.raised, 0x0000ff, 60));
}

#[test]
fn the_rail_doubles_at_twice_the_scale() {
	let rect = (12, 16, 104, 72);
	let mut row = row(3, rect);
	row.tab.badge = Some(Badge::Dot(0x00ff00));
	row.tab.close = Some(true);
	row.tab.fill = Some(COLORS.hover);
	let sidebar = Sidebar {
		area: (0, 0, 128, 200),
		style: style(2),
		..sidebar(true, vec![row])
	};
	let drawn = draw(&sidebar);
	// The dot is 12 wide at 16 from the left, centered down the row.
	let dot: Vec<_> = (12..44)
		.filter(|&x| drawn.at(x, 52) != COLORS.hover)
		.collect();
	assert_eq!((dot[0], dot.len()), (28, 12));
	assert_eq!(drawn.at(34, 52), 0x00ff00);
	let (x, y, size, _) = rail_close_rect(rect, 2);
	assert_eq!(
		drawn.at(x + 4, y + size / 2),
		blend(COLORS.hover, COLORS.fg, 40)
	);
	let number = (40, 30, 40, 44);
	let ((left, top, w, h), _) = drawn.ink(number).expect("the number is drawn");
	assert!((2 * left + w).abs_diff(2 * 12 + 104) <= 2);
	assert!((2 * top + h).abs_diff(2 * 16 + 72) <= 4);
	assert!(h > 12, "the number is drawn at the larger size: {h} tall");
}

#[test]
fn the_rail_close_mark_is_smaller_than_a_full_row_s() {
	let rail_drawn = rail(|row| row.tab.close = Some(false));
	let (_, rail_ink) = rail_drawn.ink(rail_close_rect(RAIL_ROW, 1)).unwrap();
	let full = draw(&sidebar(false, vec![full_row(18, 28)]));
	let (cx, cy, size, _) = super::header::close_rect(16, 148, (18, 28), 1);
	let (_, full_ink) = full.ink((cx, cy, size, size)).unwrap();
	assert!(rail_ink < full_ink, "{rail_ink} against {full_ink} pixels");
}

#[test]
fn rows_are_drawn_in_order_so_each_shows_its_own_number() {
	let rows = vec![row(1, (16, 18, 52, 36)), row(2, (16, 58, 52, 36))];
	let drawn = draw(&sidebar(true, rows));
	let (_, one) = drawn.ink((16, 18, 52, 36)).unwrap();
	let (_, two) = drawn.ink((16, 58, 52, 36)).unwrap();
	// "1" takes fewer pixels than "2".
	assert!(one < two, "{one} and {two}");
}
