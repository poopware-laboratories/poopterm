//! Tests of how the Settings window is shared between the list of pages and
//! the open page: beside it on a wide window, above it on a phone, clear of
//! the status bar and the home indicator, and where taps land in each.
use super::{
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_nav::{ITEMS, NAV_WIDTH, item_rect, window_target},
	settings_pen_tests::{inside, record, texts},
	settings_shape::{BAR, HEAD, Shape, compose_top, shape},
	settings_view::{FONT_ROW, MIN_SIZE, RESTORE_ROW, ROW_HEIGHT, THEME_ROW},
};
use crate::{
	render::scene::{Command, Rect},
	theme::{Theme, ThemeKind},
	ui::{chrome_colors::chrome_colors, settings_layout::FOOTER},
};

/// An iPhone 16 Pro: 402×874 points at 3×, 62 points of status bar and 22
/// of home indicator.
const PHONE: (usize, usize) = (1206, 2622);
const PHONE_SCALE: usize = 3;
const PHONE_INSET: (usize, usize) = (186, 66);
const TOP: Shape = Shape::Top { inset: PHONE_INSET };

fn center((x, y, w, h): Rect) -> (usize, usize) {
	(x + w / 2, y + h / 2)
}

#[test]
fn a_window_wide_enough_for_the_page_beside_the_list_keeps_the_sidebar() {
	let wide = MIN_SIZE.0 + NAV_WIDTH;
	for scale in [1, 2, 3] {
		assert_eq!(shape(wide * scale, scale, (0, 0)), Shape::Side, "{scale}x");
		assert_eq!(
			shape(wide * scale, scale, (40, 20)),
			Shape::Side,
			"{scale}x"
		);
		assert_eq!(
			shape((wide - 1) * scale, scale, (40, 20)),
			Shape::Top { inset: (40, 20) },
			"one logical pixel too narrow at {scale}x"
		);
	}
}

#[test]
fn a_phone_gets_the_list_on_top_with_the_systems_insets() {
	assert_eq!(shape(PHONE.0, PHONE_SCALE, PHONE_INSET), TOP);
	assert_eq!(
		shape(400, 0, PHONE_INSET),
		Shape::Top { inset: PHONE_INSET },
		"a scale of nought counts as one"
	);
}

#[test]
fn beside_the_list_the_page_is_everything_right_of_it() {
	for scale in [1, 2] {
		let size = (948 * scale, 640 * scale);
		let nav = NAV_WIDTH * scale;
		assert_eq!(
			Shape::Side.page(size, scale),
			(nav, 0, size.0 - nav, size.1)
		);
	}
	assert_eq!(Shape::Side.page((10, 10), 1), (NAV_WIDTH, 0, 0, 10));
}

#[test]
fn on_a_phone_the_page_runs_from_under_the_list_to_the_home_indicator() {
	let (x, y, w, h) = TOP.page(PHONE, PHONE_SCALE);
	assert_eq!((x, w), (0, PHONE.0), "the whole width");
	assert_eq!(y, PHONE_INSET.0 + HEAD * PHONE_SCALE);
	assert_eq!(y + h, PHONE.1 - PHONE_INSET.1);
	assert!(
		h / PHONE_SCALE >= MIN_SIZE.1,
		"as tall as the smallest desktop page: {}",
		h / PHONE_SCALE
	);
}

#[test]
fn a_window_shorter_than_its_insets_leaves_the_page_no_room() {
	let shape = Shape::Top { inset: (300, 300) };
	let (_, y, _, h) = shape.page((400, 500), 1);
	assert_eq!((y, h), (424, 0));
	let (_, y, _, h) = shape.page((400, 100), 1);
	assert_eq!((y, h), (100, 0), "clamped to the window");
}

#[test]
fn the_list_on_top_is_two_rows_of_four_inside_the_head() {
	let rects: Vec<_> = (0..ITEMS.len())
		.map(|index| TOP.item(index, PHONE.0, PHONE_SCALE))
		.collect();
	let page_top = TOP.page(PHONE, PHONE_SCALE).1;
	for (index, &(x, y, w, h)) in rects.iter().enumerate() {
		assert!(x + w <= PHONE.0, "{index} inside the width");
		assert!(
			y >= PHONE_INSET.0 + BAR * PHONE_SCALE,
			"{index} under the bar"
		);
		assert!(y + h <= page_top, "{index} above the page");
		assert!(w / PHONE_SCALE >= 80, "{index} wide enough to tap: {w}");
		assert!(h / PHONE_SCALE >= 32, "{index} tall enough to tap: {h}");
	}
	let rows: Vec<_> = rects.iter().map(|rect| rect.1).collect();
	assert_eq!(rows[..4], [rows[0]; 4]);
	assert_eq!(rows[4..], [rows[4]; 4]);
	assert!(rows[4] > rows[0]);
	for row in rects.chunks(4) {
		for pair in row.windows(2) {
			assert!(pair[0].0 + pair[0].2 < pair[1].0, "a gap between {pair:?}");
		}
	}
}

#[test]
fn beside_the_list_items_and_done_are_the_sidebars() {
	for index in 0..ITEMS.len() {
		assert_eq!(Shape::Side.item(index, 948, 1), item_rect(index, 1));
	}
	assert_eq!(
		Shape::Side.done(948, 1),
		None,
		"the window has its own close"
	);
}

#[test]
fn done_sits_at_the_right_of_the_bar_under_the_status_bar() {
	let (x, y, w, h) = TOP.done(PHONE.0, PHONE_SCALE).unwrap();
	assert!(x + w < PHONE.0 && x + w > PHONE.0 - 24 * PHONE_SCALE);
	assert!(y >= PHONE_INSET.0, "below the status bar");
	assert!(y + h <= PHONE_INSET.0 + BAR * PHONE_SCALE, "inside the bar");
	assert!(h / PHONE_SCALE >= 32 && w / PHONE_SCALE >= 60);
}

#[test]
fn a_tap_on_an_item_opens_its_page_and_on_done_closes() {
	for (index, &(page, _)) in ITEMS.iter().enumerate() {
		let at = center(TOP.item(index, PHONE.0, PHONE_SCALE));
		let target = TOP.target(at, PHONE, PHONE_SCALE, SettingsPage::About);
		assert_eq!(target, Some(SettingsTarget::Open(page)), "{page:?}");
	}
	let done = center(TOP.done(PHONE.0, PHONE_SCALE).unwrap());
	for page in [SettingsPage::Main, SettingsPage::Fonts] {
		let target = TOP.target(done, PHONE, PHONE_SCALE, page);
		assert_eq!(target, Some(SettingsTarget::Done));
	}
}

#[test]
fn taps_on_the_status_bar_the_bars_title_or_the_home_indicator_do_nothing() {
	let page = SettingsPage::Main;
	for at in [
		(PHONE.0 / 2, PHONE_INSET.0 / 2),
		(40, PHONE_INSET.0 + 20),
		(PHONE.0 / 2, PHONE.1 - PHONE_INSET.1 / 2),
		(PHONE.0 / 2, PHONE.1 + 10),
	] {
		assert_eq!(TOP.target(at, PHONE, PHONE_SCALE, page), None, "{at:?}");
	}
}

#[test]
fn a_tap_in_the_page_reaches_it_as_if_the_page_stood_alone() {
	let (px, py, pw, ph) = TOP.page(PHONE, PHONE_SCALE);
	for page in [
		SettingsPage::Main,
		SettingsPage::Layout,
		SettingsPage::Shortcuts,
	] {
		for y in (0..ph).step_by(29) {
			for x in (0..pw).step_by(31) {
				let alone = settings_target((x, y), (pw, ph), PHONE_SCALE, page);
				let tapped = TOP.target((px + x, py + y), PHONE, PHONE_SCALE, page);
				assert_eq!(tapped, alone, "{page:?} at {x},{y}");
			}
		}
	}
}

#[test]
fn beside_the_list_taps_land_where_the_sidebar_sends_them() {
	let size = (948, 640);
	for page in [SettingsPage::Main, SettingsPage::Clipboard] {
		for y in (0..size.1).step_by(23) {
			for x in (0..size.0).step_by(37) {
				let side = Shape::Side.target((x, y), size, 1, page);
				assert_eq!(side, window_target((x, y), size, 1, page), "{x},{y}");
			}
		}
	}
}

#[test]
fn the_appearance_pages_controls_fit_a_phone_above_the_save_button() {
	let (px, py, pw, ph) = TOP.page(PHONE, PHONE_SCALE);
	let s = PHONE_SCALE;
	let row = |row: usize| py + (row + ROW_HEIGHT / 2) * s;
	let page = SettingsPage::Main;
	let at = |x, y| TOP.target((x, y), PHONE, s, page);
	assert_eq!(
		at(px + 30 * s, row(THEME_ROW)),
		Some(SettingsTarget::Theme(-1))
	);
	assert_eq!(
		at(pw - 30 * s, row(THEME_ROW)),
		Some(SettingsTarget::Theme(1))
	);
	assert_eq!(
		at(pw - 50 * s, row(FONT_ROW)),
		Some(SettingsTarget::Step(FONT_ROW, 1))
	);
	assert_eq!(
		at(pw - 45 * s, row(RESTORE_ROW)),
		Some(SettingsTarget::ToggleRestore)
	);
	assert!(
		(RESTORE_ROW + ROW_HEIGHT) * s <= ph - FOOTER * s,
		"the last row clears the footer"
	);
}

fn top_drawn(scale: usize, page: SettingsPage, cursor: (f64, f64)) -> Vec<Command> {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let colors = chrome_colors(&theme);
	let shape = Shape::Top {
		inset: (20 * scale, 10 * scale),
	};
	record(scale, |pen, p, style| {
		compose_top(pen, p, shape, (page, cursor), (&colors, theme.fg, style))
	})
}

fn round_fills(commands: &[Command]) -> Vec<Rect> {
	let fill = |command: &Command| match command {
		Command::RoundRect(rect, ..) => Some(*rect),
		_ => None,
	};
	commands.iter().filter_map(fill).collect()
}

const AWAY: (f64, f64) = (-1.0, -1.0);

#[test]
fn the_top_list_fills_the_open_page_and_lights_the_one_under_the_finger() {
	let shape = Shape::Top { inset: (20, 10) };
	for (index, &(page, _)) in ITEMS.iter().enumerate() {
		let filled = round_fills(&top_drawn(1, page, AWAY));
		assert_eq!(filled, [shape.item(index, 900, 1)], "{page:?}");
	}
	let (x, y, ..) = shape.item(5, 900, 1);
	let over = (x as f64 + 3.0, y as f64 + 3.0);
	let lit = round_fills(&top_drawn(1, SettingsPage::Main, over));
	assert_eq!(lit, [shape.item(0, 900, 1), shape.item(5, 900, 1)]);
}

#[test]
fn the_top_list_names_every_page_inside_its_item_and_offers_done() {
	for scale in [1, 2] {
		let shape = Shape::Top {
			inset: (20 * scale, 10 * scale),
		};
		let commands = top_drawn(scale, SettingsPage::Main, AWAY);
		let labels = texts(&commands);
		let names: Vec<_> = labels.iter().map(|(text, _)| text.as_str()).collect();
		let mut expected = vec!["Settings", "Done"];
		expected.extend(ITEMS.iter().map(|&(_, label)| label));
		assert_eq!(names, expected, "{scale}x");
		for (index, (_, at)) in labels[2..].iter().enumerate() {
			let rect = shape.item(index, 900 * scale, scale);
			assert!(inside(*at, rect), "{index} at {scale}x: {at:?} in {rect:?}");
		}
		let done = shape.done(900 * scale, scale).unwrap();
		assert!(inside(labels[1].1, done), "Done's label on its button");
		let title = labels[0].1;
		assert!(title.1 >= 20 * scale, "the title is under the status bar");
	}
}

#[test]
fn every_item_and_done_can_take_the_keyboards_focus() {
	let shape = Shape::Top { inset: (20, 10) };
	let focusable: Vec<_> = top_drawn(1, SettingsPage::Main, AWAY)
		.iter()
		.filter_map(|command| match command {
			Command::Focusable(rect) => Some(*rect),
			_ => None,
		})
		.collect();
	let mut expected = vec![shape.done(900, 1).unwrap()];
	expected.extend((0..ITEMS.len()).map(|index| shape.item(index, 900, 1)));
	assert_eq!(focusable, expected);
}

#[test]
fn the_head_is_a_panel_from_the_top_edge_to_the_page_with_a_line_under_it() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let colors = chrome_colors(&theme);
	let rects: Vec<_> = top_drawn(1, SettingsPage::Main, AWAY)
		.iter()
		.filter_map(|command| match command {
			Command::Rect(rect, color) => Some((*rect, *color)),
			_ => None,
		})
		.collect();
	let page_top = Shape::Top { inset: (20, 10) }.page((900, 600), 1).1;
	assert_eq!(
		rects,
		[
			((0, 0, 900, page_top), colors.chrome),
			((0, page_top - 1, 900, 1), colors.hairline)
		]
	);
}
