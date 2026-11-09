//! Tests that the Settings main page's controls are drawn where clicks reach
//! them, and that the rows keep clear of each other and the footer.
use super::{
	SettingsEditor,
	settings_fonts::FontPicker,
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_pen_tests::{buttons, inside, record, texts},
	settings_view::{
		FONT_ROW, MIN_SIZE, OPACITY_ROW, RESTORE_ROW, SCROLLBACK_ROW, STRENGTH_ROW, THEME_ROW,
		compose_main_settings,
	},
};
use crate::{
	config::Preferences, input::shortcuts::Shortcuts, theme::ThemeKind, ui::diagnostics_page::Section,
};

fn editor() -> SettingsEditor {
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
		compose_main_settings(&editor(), style, p, pen, &|_, _, _, _| false)
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
		compose_main_settings(&editor(), style, p, pen, &|_, _, _, _| false)
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
