//! Tests of Settings pages on a phone's narrow screen: the controls at the
//! right of each row move closer to the edge so labels keep their room, and
//! text that would run past the edge or into a button is cut to fit.
use super::{
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_pen_tests::{buttons, inside, record_in, texts},
	settings_rows_tests::editor,
	settings_view::{
		FONT_ROW, PADDING_ROW, ROW_HEIGHT, SCROLLBACK_ROW, SHORTCUT_ROWS, STRENGTH_ROW,
		compose_main_settings,
	},
};
use crate::{
	render::scene::Command,
	ui::settings_layout::{NARROW, controls},
};

/// A phone's page: 402 points wide, as an iPhone 16 Pro.
const PHONE: (usize, usize) = (402, 666);

#[test]
fn the_controls_start_240_from_the_right_on_a_wide_page_and_176_on_a_narrow_one() {
	assert_eq!(controls(680), 240);
	assert_eq!(controls(NARROW), 240);
	assert_eq!(controls(NARROW - 1), 176);
	assert_eq!(controls(PHONE.0), 176);
	assert_eq!(controls(0), 176);
}

#[test]
fn a_narrow_stepper_keeps_its_value_between_minus_and_plus() {
	for scale in [1, 3] {
		let s = scale;
		let commands = record_in(PHONE, scale, |pen, p, style| {
			pen.stepper(p, style, 300, "5000")
		});
		let row = (300 * s, 48 * s, ROW_HEIGHT * s);
		let minus = ((PHONE.0 - 176) * s, row.0, row.1, row.2);
		let plus = ((PHONE.0 - 72) * s, row.0, row.1, row.2);
		assert_eq!(buttons(&commands), [(minus, false), (plus, false)]);
		let texts = texts(&commands);
		let value_x = texts[2].1.0;
		assert!(
			minus.0 + minus.2 <= value_x && value_x < plus.0,
			"x{s}: {texts:?}"
		);
	}
}

#[test]
fn a_narrow_wide_button_ends_where_a_wide_one_does() {
	let commands = record_in(PHONE, 1, |pen, p, style| {
		pen.wide_button(p, style, 120, "Ctrl+Shift+T", false, &|_, _, _, _| false)
	});
	let rect = (PHONE.0 - 176, 120, 152, ROW_HEIGHT);
	assert_eq!(buttons(&commands), [(rect, false)]);
	assert!(inside(texts(&commands)[0].1, rect));
}

#[test]
fn taps_on_a_narrow_page_reach_the_steppers_where_they_are_drawn() {
	for scale in [1, 3] {
		let s = scale;
		let size = (PHONE.0 * s, PHONE.1 * s);
		let x = |from_right: usize| (PHONE.0 - from_right) * s;
		for row in [FONT_ROW, SCROLLBACK_ROW, STRENGTH_ROW] {
			let y = (row + 20) * s;
			let at = |x| settings_target((x, y), size, s, SettingsPage::Main);
			assert_eq!(at(x(176)), Some(SettingsTarget::Step(row, -1)), "{row}");
			assert_eq!(at(x(129)), Some(SettingsTarget::Step(row, -1)), "{row}");
			assert_eq!(at(x(127)), None, "the value between the buttons");
			assert_eq!(at(x(200)), None, "where a wide page's minus was");
			assert_eq!(at(x(60)), Some(SettingsTarget::Step(row, 1)), "{row}");
		}
		let y = (PADDING_ROW + 20) * s;
		let padding = |x| settings_target((x, y), size, s, SettingsPage::Layout);
		assert_eq!(padding(x(170)), Some(SettingsTarget::Step(PADDING_ROW, -1)));
		assert_eq!(padding(x(220)), None);
	}
}

#[test]
fn taps_on_a_narrow_shortcut_page_reach_its_buttons_and_not_the_labels() {
	let size = PHONE;
	for &(action, row) in &SHORTCUT_ROWS {
		let at = |x| settings_target((x, row + 20), size, 1, SettingsPage::Shortcuts);
		assert_eq!(at(PHONE.0 - 170), Some(SettingsTarget::SetShortcut(action)));
		assert_eq!(at(PHONE.0 - 30), Some(SettingsTarget::SetShortcut(action)));
		assert_eq!(at(PHONE.0 - 200), None, "{action:?}: the label's room");
	}
}

#[test]
fn every_stepper_label_ends_before_its_minus_button_on_a_phone() {
	let commands = record_in(PHONE, 1, |pen, p, style| {
		compose_main_settings(&editor(), style, p, pen)
	});
	let minus = PHONE.0 - 176;
	let labels = ["Wallpaper strength", "Font size", "Scrollback lines"];
	let mut measured = 0;
	record_in(PHONE, 1, |pen, _, _| {
		for (text, at) in texts(&commands) {
			if labels.contains(&text.as_str()) {
				let end = at.0 + pen.width(&text, 0.82);
				assert!(end + 4 <= minus, "{text} ends at {end}");
				measured += 1;
			}
		}
	});
	assert_eq!(measured, labels.len());
}

#[test]
fn no_text_on_the_main_page_runs_past_a_phones_edge() {
	let mut wide_name = editor();
	wide_name.wallpaper_path = Some("/a/very-long-wallpaper-file-name-indeed.png".into());
	for editor in [editor(), wide_name] {
		let commands = record_in(PHONE, 1, |pen, p, style| {
			compose_main_settings(&editor, style, p, pen)
		});
		record_in(PHONE, 1, |pen, _, _| {
			for command in &commands {
				let Command::Text { text, at, px, .. } = command else {
					continue;
				};
				let end = at.0 + pen.width(text, px / 18.0);
				assert!(end <= PHONE.0 - 16, "{text:?} ends at {end}");
			}
		});
	}
}

#[test]
fn a_long_status_is_cut_before_the_save_button() {
	let status = "Press a key chord for Previous tab. Esc cancels.";
	for scale in [1, 3] {
		let s = scale;
		let commands = record_in(PHONE, scale, |pen, p, style| {
			pen.footer(p, style, ("Save", 120), status, &|_, _, _, _| false)
		});
		let save = (PHONE.0 - 140) * s;
		let texts = texts(&commands);
		let (shown, at) = &texts[1];
		assert!(
			shown.ends_with('…') && shown.len() < status.len(),
			"{shown}"
		);
		record_in(PHONE, scale, |pen, _, _| {
			let end = at.0 + pen.width(shown, 0.68);
			assert!(end + 8 * s <= save, "x{s}: {shown} ends at {end}");
		});
	}
}

#[test]
fn a_short_status_is_shown_whole_on_a_phone() {
	let commands = record_in(PHONE, 1, |pen, p, style| {
		pen.footer(p, style, ("Save", 120), "Saved", &|_, _, _, _| false)
	});
	assert_eq!(texts(&commands)[1].0, "Saved");
}

/// The lines a page's hint is drawn as on a page `size` large, with where.
fn hint_lines(size: (usize, usize), scale: usize, hint: &str) -> Vec<(String, (usize, usize))> {
	texts(&record_in(size, scale, |pen, p, _| pen.hint(p, hint)))
}

const LAYOUT_HINT: &str = "Tabs, toolbar buttons, the bottom bar, and the window frame.";

#[test]
fn a_hint_that_fits_is_one_line_under_the_title() {
	for scale in [1, 2] {
		let lines = hint_lines((680, 600), scale, LAYOUT_HINT);
		assert_eq!(lines, [(LAYOUT_HINT.to_owned(), (24 * scale, 68 * scale))]);
	}
}

#[test]
fn a_hint_too_wide_for_a_phone_wraps_at_a_word_onto_a_second_line() {
	for scale in [1, 3] {
		let s = scale;
		let lines = hint_lines(PHONE, scale, LAYOUT_HINT);
		assert_eq!(lines.len(), 2, "{lines:?}");
		assert_eq!(lines[0].1, (24 * s, 68 * s));
		assert_eq!(lines[1].1, (24 * s, 85 * s));
		assert_eq!(format!("{} {}", lines[0].0, lines[1].0), LAYOUT_HINT);
		record_in(PHONE, scale, |pen, _, _| {
			for (text, at) in &lines {
				let end = at.0 + pen.width(text, 0.72);
				assert!(end <= (PHONE.0 - 24) * s, "x{s}: {text} ends at {end}");
			}
		});
	}
}

#[test]
fn a_hint_too_long_for_two_lines_ends_the_second_with_an_ellipsis() {
	let hint = "word ".repeat(40);
	let lines = hint_lines(PHONE, 1, hint.trim());
	assert_eq!(lines.len(), 2);
	assert!(lines[1].0.ends_with('…'), "{:?}", lines[1].0);
	assert!(lines[0].0.starts_with("word word"));
}

#[test]
fn a_hint_of_one_long_word_is_cut_on_its_first_line() {
	let hint = "x".repeat(200);
	let lines = hint_lines(PHONE, 1, &hint);
	assert_eq!(lines.len(), 1, "{lines:?}");
	assert!(lines[0].0.ends_with('…'));
}

#[test]
fn every_page_draws_into_a_window_too_small_for_it_without_panicking() {
	// A window is drawn once before iOS gives it its size.
	use super::settings_view::{compose_about, compose_layout_settings, compose_shortcuts};
	for size in [(1, 1), (10, 10), (40, 30), (100, 60)] {
		for scale in [1, 3] {
			record_in(size, scale, |pen, p, style| {
				compose_main_settings(&editor(), style, p, pen);
				compose_layout_settings(&editor(), style, p, pen);
				let page = ("Shortcuts", "A hint", &SHORTCUT_ROWS[..]);
				compose_shortcuts(&editor(), page, style, p, pen, &|_, _, _, _| false);
				compose_about(pen, p, (0xffffff, 0x888888));
				pen.footer(p, style, ("Save", 120), "status", &|_, _, _, _| false);
				pen.hint(p, "a hint that has to wrap somewhere");
			});
		}
	}
}

#[test]
fn taps_on_a_window_too_small_for_its_page_never_panic() {
	use super::settings_shape::{Shape, shape};
	let pages = [
		SettingsPage::Main,
		SettingsPage::Layout,
		SettingsPage::Fonts,
		SettingsPage::Shortcuts,
		SettingsPage::Panes,
		SettingsPage::Clipboard,
		SettingsPage::Diagnostics,
		SettingsPage::About,
	];
	for size in [(1, 1), (10, 10), (40, 30), (100, 60), (300, 900)] {
		for scale in [1, 3] {
			for closable in [true, false] {
				let shape: Shape = shape(size.0, scale, (6, 3), closable);
				for page in pages {
					for y in (0..size.1 + 5).step_by(3) {
						for x in (0..size.0 + 5).step_by(3) {
							let _ = shape.target((x, y), size, scale, (page, 0));
							let _ = settings_target((x, y), size, scale, page);
						}
					}
				}
			}
		}
	}
}
