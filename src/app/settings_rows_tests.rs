//! Tests that the Settings main page's controls are drawn where clicks reach
//! them, and that the rows keep clear of each other and the footer.
use super::{
	SettingsEditor,
	settings_fonts::FontPicker,
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_pen_tests::{buttons, inside, record, texts},
	settings_view::{
		FONT_ROW, MIN_SIZE, OPACITY_ROW, RESTORE_ROW, SCROLLBACK_ROW, STRENGTH_ROW, THEME_ROW,
		compose_layout_settings, compose_main_settings,
	},
};
use crate::{
	config::Preferences, input::shortcuts::Shortcuts, theme::ThemeKind, ui::diagnostics_page::Section,
};

pub(super) fn editor() -> SettingsEditor {
	SettingsEditor {
		theme: ThemeKind::MacOsDark,
		preferences: Preferences::default(),
		wallpaper_path: None,
		wallpaper_strength: 35,
		status: String::new(),
		page: SettingsPage::Main,
		recording_shortcut: None,
		shortcuts: Shortcuts::from_config(""),
		fonts: FontPicker::new(Vec::new(), ""),
		diagnostics_section: Section::Log,
		copied: String::new(),
	}
}

fn main_buttons(scale: usize, size: (usize, usize)) -> Vec<((usize, usize, usize, usize), bool)> {
	let commands = record(scale, |pen, p, style| {
		(p.width, p.height) = (size.0 * scale, size.1 * scale);
		compose_main_settings(&editor(), style, p, pen)
	});
	buttons(&commands)
}

#[test]
fn the_center_of_every_button_drawn_is_a_click_target() {
	for scale in [1, 2] {
		for size in [MIN_SIZE, (760, 640), (1000, 800)] {
			let window = (size.0 * scale, size.1 * scale);
			// The picker's middle box only shows the theme's name; its
			// arrows change it.
			let shows_name =
				|rect: &(usize, usize, usize, usize)| rect.1 == THEME_ROW * scale && rect.2 > 100 * scale;
			for (rect, _) in main_buttons(scale, size) {
				if shows_name(&rect) {
					continue;
				}
				let center = (rect.0 + rect.2 / 2, rect.1 + rect.3 / 2);
				let target = settings_target(center, window, scale, SettingsPage::Main);
				assert!(target.is_some(), "x{scale} {size:?}: nothing at {rect:?}");
			}
		}
	}
}

#[test]
fn no_two_buttons_on_the_main_page_overlap() {
	for scale in [1, 2] {
		let drawn = main_buttons(scale, MIN_SIZE);
		for (index, (a, _)) in drawn.iter().enumerate() {
			for (b, _) in &drawn[index + 1..] {
				let apart = a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1;
				assert!(apart, "x{scale}: {a:?} overlaps {b:?}");
			}
		}
	}
}

#[test]
fn every_button_stays_above_the_footer_in_the_smallest_window() {
	for scale in [1, 2] {
		let footer = (MIN_SIZE.1 - 52) * scale;
		for (rect, _) in main_buttons(scale, MIN_SIZE) {
			assert!(
				rect.1 + rect.3 <= footer,
				"x{scale}: {rect:?} past {footer}"
			);
		}
	}
}

#[test]
fn the_wallpaper_buttons_share_a_row_and_a_height() {
	let drawn = main_buttons(1, MIN_SIZE);
	let wallpaper: Vec<_> = drawn
		.iter()
		.map(|&(rect, _)| rect)
		.filter(|rect| rect.3 == 32)
		.collect();
	assert_eq!(wallpaper.len(), 2);
	assert_eq!(wallpaper[0].1, wallpaper[1].1);
}

#[test]
fn clicking_the_wallpaper_buttons_where_they_are_drawn_opens_and_clears() {
	let drawn = main_buttons(1, MIN_SIZE);
	let rects: Vec<_> = drawn
		.iter()
		.map(|&(rect, _)| rect)
		.filter(|rect| rect.3 == 32)
		.collect();
	let at = |rect: (usize, usize, usize, usize)| {
		settings_target((rect.0 + 4, rect.1 + 4), MIN_SIZE, 1, SettingsPage::Main)
	};
	assert_eq!(at(rects[0]), Some(SettingsTarget::ChooseWallpaper));
	assert_eq!(at(rects[1]), Some(SettingsTarget::ClearWallpaper));
	assert!(inside((rects[0].0, rects[0].1), rects[0]));
}

fn label_row(scale: usize, label: &str) -> usize {
	let commands = record(scale, |pen, p, style| {
		(p.width, p.height) = (MIN_SIZE.0 * scale, MIN_SIZE.1 * scale);
		compose_main_settings(&editor(), style, p, pen)
	});
	let found = texts(&commands).into_iter().find(|(text, _)| text == label);
	found.unwrap_or_else(|| panic!("{label} is not drawn")).1.1
}

#[test]
fn each_label_sits_beside_its_control_at_every_scale() {
	for scale in [1, 2] {
		for (label, row) in [
			("Wallpaper strength", STRENGTH_ROW),
			("Window opacity", OPACITY_ROW),
			("Font size", FONT_ROW),
			("Scrollback lines", SCROLLBACK_ROW),
			("Restore tabs on launch", RESTORE_ROW),
		] {
			// Only macOS draws the opacity stepper.
			if label == "Window opacity" && !cfg!(target_os = "macos") {
				continue;
			}
			assert_eq!(
				label_row(scale, label),
				(row + 8) * scale,
				"{label} at {scale}x"
			);
		}
	}
}

#[test]
fn the_labels_run_down_the_page_in_the_order_the_settings_are_grouped() {
	let order = [
		"Color theme",
		"Background wallpaper",
		"Wallpaper strength",
		"Window opacity",
		"Font size",
		"Scrollback lines",
		"Restore tabs on launch",
	];
	// Only macOS draws the opacity stepper.
	let shown = order
		.iter()
		.filter(|&&label| label != "Window opacity" || cfg!(target_os = "macos"));
	let rows: Vec<_> = shown.map(|label| label_row(1, label)).collect();
	assert!(rows.windows(2).all(|pair| pair[0] < pair[1]), "{rows:?}");
	assert_eq!(
		label_row(1, "Color theme"),
		crate::ui::settings_layout::MAIN.theme_label
	);
}

fn layout_page(vertical: bool, scale: usize) -> Vec<crate::render::scene::Command> {
	let mut editor = editor();
	editor.preferences.vertical_tabs = vertical;
	record(scale, |pen, p, style| {
		(p.width, p.height) = (MIN_SIZE.0 * scale, MIN_SIZE.1 * scale);
		compose_layout_settings(&editor, style, p, pen)
	})
}

fn round_rects(commands: &[crate::render::scene::Command]) -> Vec<(usize, usize, usize, usize)> {
	commands
		.iter()
		.filter_map(|command| match command {
			crate::render::scene::Command::RoundRect(rect, ..) => Some(*rect),
			_ => None,
		})
		.collect()
}

#[test]
fn the_layout_page_labels_its_rows_and_has_no_vertical_tabs_switch() {
	let labels: Vec<_> = texts(&layout_page(false, 1))
		.into_iter()
		.map(|(text, _)| text)
		.collect();
	for wanted in ["Show tabs", "Tab position", "Top", "Side", "Padding"] {
		assert!(labels.contains(&wanted.to_owned()), "{wanted}");
	}
	assert!(!labels.iter().any(|label| label == "Vertical tabs"));
}

#[test]
fn the_selected_segment_follows_the_tab_position_preference() {
	let selected = |vertical| {
		let fills = round_rects(&layout_page(vertical, 1));
		// The track is the control's own rectangle; the filled segment follows it.
		let track = fills
			.iter()
			.position(|rect| rect.2 == 156)
			.expect("the track");
		fills[track + 1]
	};
	let (top, side) = (selected(false), selected(true));
	assert!(top.0 < side.0, "{top:?} against {side:?}");
	assert_eq!((top.1, top.2, top.3), (side.1, side.2, side.3));
}

#[test]
fn the_tab_position_row_sits_between_the_switch_rows_in_the_order_it_was_listed() {
	let rows = texts(&layout_page(false, 1));
	let row = |label: &str| rows.iter().find(|(text, _)| text == label).unwrap().1.1;
	assert!(row("Show tabs") < row("Tab position"));
	assert!(row("Tab position") < row("Show settings button"));
	assert!(row("Show bottom bar") < row("Padding"));
}

#[test]
fn the_layout_page_draws_six_switches_and_one_control_with_a_stepper_at_every_scale() {
	for scale in [1, 2] {
		let commands = layout_page(true, scale);
		let pills = round_rects(&commands);
		// Six switches draw a track and a knob each, the segmented control a
		// track and a filled segment.
		assert_eq!(pills.len(), 6 * 2 + 2, "x{scale}");
		assert_eq!(
			buttons(&commands).len(),
			2,
			"the padding stepper's two buttons, x{scale}"
		);
	}
}

#[test]
fn nothing_on_the_layout_page_reaches_the_footer_in_the_smallest_window() {
	for scale in [1, 2] {
		let footer = (MIN_SIZE.1 - 52) * scale;
		for (rect, _) in buttons(&layout_page(false, scale)) {
			assert!(rect.1 + rect.3 <= footer, "x{scale}");
		}
		for rect in round_rects(&layout_page(false, scale)) {
			assert!(rect.1 + rect.3 <= footer, "x{scale}: {rect:?}");
		}
	}
}

fn frame_switch_knob(frameless: bool) -> (usize, usize, usize, usize) {
	let mut editor = editor();
	editor.preferences.window_frame = if frameless {
		crate::ui::frame::WindowFrame::Frameless
	} else {
		crate::ui::frame::WindowFrame::Normal
	};
	let commands = record(1, |pen, p, style| {
		(p.width, p.height) = MIN_SIZE;
		compose_layout_settings(&editor, style, p, pen)
	});
	// The knob is the last pill drawn inside the frame switch's row.
	let row = super::settings_view::FRAME_ROW..super::settings_view::FRAME_ROW + 36;
	let pills = round_rects(&commands);
	*pills
		.iter()
		.rfind(|rect| row.contains(&rect.1))
		.expect("a knob")
}

#[test]
fn the_layout_page_has_a_square_window_switch_below_the_padding_stepper() {
	let rows = texts(&layout_page(false, 1));
	let row = |label: &str| rows.iter().find(|(text, _)| text == label).unwrap().1.1;
	assert!(row("Padding") < row("Square window (no title bar)"));
}

#[test]
fn the_square_window_knob_sits_right_when_on_and_left_when_off() {
	let (off, on) = (frame_switch_knob(false), frame_switch_knob(true));
	assert!(on.0 > off.0, "{on:?} against {off:?}");
	assert_eq!((on.1, on.2, on.3), (off.1, off.2, off.3));
}
