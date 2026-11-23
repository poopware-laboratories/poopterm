//! Tests of every Settings page on a phone, through the window as a whole:
//! each control is reached where it is drawn under the list across the top,
//! clears the footer, and leaves its label room; and the keyboard's focus
//! runs from Done through the list to the page.
use super::{
	settings_focus::{center, focus_order},
	settings_hit::{LAYOUT_TOGGLES, SettingsPage, SettingsTarget, font_list_rows},
	settings_nav::ITEMS,
	settings_pen::{footer_rect, segment_rect, switch_hit, switch_rect},
	settings_pen_tests::{record_in, texts},
	settings_rows_tests::editor,
	settings_shape::{Shape, compose_top},
	settings_view::{
		CLIPBOARD_ROWS, FONT_LIST_STRIDE, FONT_LIST_TOP, PADDING_ROW, PANE_ROWS, ROW_HEIGHT,
		SHORTCUT_ROWS, VERTICAL_TABS_ROW, compose_layout_settings, compose_main_settings,
		compose_shortcuts,
	},
};
use crate::{
	render::scene::{Rect, Scene},
	theme::{Theme, ThemeKind},
	ui::{
		chrome_colors::chrome_colors,
		settings_layout::{FOOTER, controls},
	},
};

/// An iPhone 16 Pro at 3×, with its status bar and home indicator.
const PHONE: (usize, usize) = (1206, 2622);
const S: usize = 3;
const TOP: Shape = Shape::Top {
	inset: (186, 66),
	sides: (0, 0),
};

/// The page's rectangle in the phone's window.
fn page() -> Rect {
	TOP.page(PHONE, S)
}

/// The window point of logical `(x, y)` in the page, `x` counted from the
/// page's right edge when `from_right`.
fn at((x, y): (usize, usize), from_right: bool) -> (usize, usize) {
	let (px, py, pw, _) = page();
	let x = if from_right { pw - x * S } else { x * S };
	(px + x, py + y * S)
}

fn tap(point: (usize, usize), page: SettingsPage) -> Option<SettingsTarget> {
	TOP.target(point, PHONE, S, page)
}

/// Where a physical rectangle of the page is in the window.
fn in_window((x, y, w, h): Rect) -> Rect {
	let (px, py, ..) = page();
	(px + x, py + y, w, h)
}

#[test]
fn every_layout_switch_flips_its_setting_on_a_phone() {
	let pw = page().2;
	for (row, target) in LAYOUT_TOGGLES {
		let switch = in_window(switch_hit(pw, row, S));
		assert_eq!(
			tap(center(switch), SettingsPage::Layout),
			Some(target),
			"{row}"
		);
	}
}

#[test]
fn the_tab_position_halves_answer_on_a_phone() {
	let pw = page().2;
	for (index, side) in [(0, false), (1, true)] {
		let segment = in_window(segment_rect(pw, VERTICAL_TABS_ROW, S, index, 2));
		assert_eq!(
			tap(center(segment), SettingsPage::Layout),
			Some(SettingsTarget::SetVerticalTabs(side))
		);
	}
}

#[test]
fn the_padding_stepper_answers_on_a_phone() {
	let y = PADDING_ROW + ROW_HEIGHT / 2;
	let (minus, plus) = (at((150, y), true), at((48, y), true));
	assert_eq!(
		tap(minus, SettingsPage::Layout),
		Some(SettingsTarget::Step(PADDING_ROW, -1))
	);
	assert_eq!(
		tap(plus, SettingsPage::Layout),
		Some(SettingsTarget::Step(PADDING_ROW, 1))
	);
}

#[test]
fn every_shortcut_on_every_shortcut_page_is_reached_on_a_phone() {
	let pages = [
		(SettingsPage::Shortcuts, &SHORTCUT_ROWS[..]),
		(SettingsPage::Panes, &PANE_ROWS[..]),
		(SettingsPage::Clipboard, &CLIPBOARD_ROWS[..]),
	];
	for (page, rows) in pages {
		for &(action, row) in rows {
			let point = at((100, row + ROW_HEIGHT / 2), true);
			assert_eq!(
				tap(point, page),
				Some(SettingsTarget::SetShortcut(action)),
				"{page:?} {action:?}"
			);
		}
	}
}

#[test]
fn every_row_of_every_page_clears_the_footer_on_a_phone() {
	let footer = page().3 / S - FOOTER;
	let last = [
		SHORTCUT_ROWS.last().unwrap().1,
		PANE_ROWS.last().unwrap().1,
		CLIPBOARD_ROWS.last().unwrap().1,
		LAYOUT_TOGGLES.iter().map(|&(row, _)| row).max().unwrap(),
	];
	for row in last {
		assert!(
			row + ROW_HEIGHT <= footer,
			"{row} + {ROW_HEIGHT} > {footer}"
		);
	}
}

#[test]
fn save_is_reached_above_the_home_indicator_on_every_page() {
	let (_, _, pw, ph) = page();
	let save = in_window(footer_rect((pw, ph), 120, S));
	assert!(save.1 + save.3 <= PHONE.1 - 66, "above the home indicator");
	for page in [
		SettingsPage::Main,
		SettingsPage::Layout,
		SettingsPage::Fonts,
		SettingsPage::Shortcuts,
	] {
		assert_eq!(
			tap(center(save), page),
			Some(SettingsTarget::Save),
			"{page:?}"
		);
	}
}

#[test]
fn the_font_list_shows_a_phones_worth_of_rows_and_each_is_tapped() {
	let rows = font_list_rows(page().3, S);
	assert!(rows >= 10, "{rows} rows");
	for row in 0..rows {
		let point = at((60, FONT_LIST_TOP + row * FONT_LIST_STRIDE + 20), false);
		assert_eq!(
			tap(point, SettingsPage::Fonts),
			Some(SettingsTarget::FontRow(row))
		);
	}
}

/// The texts `draw` puts on a phone's page, each with where it ends.
fn text_ends(
	draw: impl FnOnce(&mut super::settings_pen::Pen, &mut Scene, crate::render::painter::ButtonStyle),
) -> Vec<(String, (usize, usize), usize)> {
	let size = (page().2 / S, page().3 / S);
	let commands = record_in(size, 1, draw);
	let mut ends = Vec::new();
	record_in(size, 1, |pen, _, _| {
		for (text, at) in texts(&commands) {
			let end = at.0 + pen.width(&text, 0.82);
			ends.push((text, at, end));
		}
	});
	ends
}

#[test]
fn layout_labels_end_before_the_control_on_their_row_on_a_phone() {
	let pw = page().2 / S;
	let ends = text_ends(|pen, p, style| compose_layout_settings(&editor(), style, p, pen));
	let label = |name: &str| {
		let found = ends.iter().find(|(text, ..)| text == name);
		found.unwrap_or_else(|| panic!("{name}")).2
	};
	let switch = switch_rect(pw, 0, 1).0;
	for name in [
		"Show tabs",
		"Show settings button",
		"Square window (no title bar)",
	] {
		assert!(label(name) < switch, "{name} ends at {}", label(name));
	}
	let segment = segment_rect(pw, VERTICAL_TABS_ROW, 1, 0, 2).0;
	assert!(label("Tab position") < segment);
	assert!(label("Padding") < pw - controls(pw));
}

#[test]
fn shortcut_labels_end_before_their_buttons_on_a_phone() {
	let pw = page().2 / S;
	let mut editor = editor();
	editor.page = SettingsPage::Shortcuts;
	let page = ("Keyboard shortcuts", "Click a binding.", &SHORTCUT_ROWS[..]);
	let ends =
		text_ends(|pen, p, style| compose_shortcuts(&editor, page, style, p, pen, &|_, _, _, _| false));
	for &(action, _) in &SHORTCUT_ROWS {
		let (_, _, end) = ends
			.iter()
			.find(|(text, ..)| text == action.label())
			.unwrap();
		assert!(*end < pw - controls(pw), "{action:?} ends at {end}");
	}
}

/// The phone's whole window with `page` open, and its focus order, each
/// with what a tap at its center does.
fn order(page: SettingsPage) -> Vec<(Rect, Option<SettingsTarget>)> {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let colors = chrome_colors(&theme);
	let (px, py, pw, ph) = self::page();
	let mut window = Scene::new(PHONE.0, PHONE.1);
	window.commands = record_in((PHONE.0 / S, PHONE.1 / S), S, |pen, p, style| {
		compose_top(
			pen,
			p,
			TOP,
			(page, (-1.0, -1.0)),
			(&colors, theme.fg, style),
		)
	});
	let mut content = Scene::new(pw, ph);
	content.commands = record_in((pw / S, ph / S), S, |pen, p, style| {
		compose_main_settings(&editor(), style, p, pen)
	});
	window.commands.extend(content.moved((px, py)).commands);
	let reaches = |point| tap(point, page);
	focus_order(&window, 0, |point| reaches(point).is_some())
		.into_iter()
		.map(|rect| (rect, reaches(center(rect))))
		.collect()
}

#[test]
fn the_keyboard_focuses_done_then_the_list_then_the_page_down_a_phone() {
	let found = order(SettingsPage::Main);
	let targets: Vec<_> = found.iter().map(|(_, target)| target.unwrap()).collect();
	assert_eq!(targets[0], SettingsTarget::Done);
	let opened: Vec<_> = targets[1..=ITEMS.len()].to_vec();
	let pages: Vec<_> = ITEMS
		.iter()
		.map(|&(page, _)| SettingsTarget::Open(page))
		.collect();
	assert_eq!(opened, pages, "the list in reading order");
	let rest = &targets[ITEMS.len() + 1..];
	assert_eq!(rest[0], SettingsTarget::Theme(-1));
	assert!(rest.contains(&SettingsTarget::ToggleRestore));
	assert!(
		rest
			.iter()
			.all(|t| !matches!(t, SettingsTarget::Open(_) | SettingsTarget::Done)),
		"{rest:?}"
	);
}

#[test]
fn nothing_focused_on_a_phone_sits_under_the_status_bar() {
	for (rect, _) in order(SettingsPage::Main) {
		assert!(rect.1 >= 186, "{rect:?}");
		assert!(rect.1 + rect.3 <= PHONE.1 - 66, "{rect:?}");
	}
}
