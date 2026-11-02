//! Tests of the Settings window's main page header, where Diagnostics
//! joined the page buttons: what is drawn is what a click lands on, and the
//! title still fits; and of opening a page, which clears Copy report's word.
use super::{
	SettingsEditor,
	settings_fonts::FontPicker,
	settings_hit::{HEADER_BUTTONS, SettingsPage, SettingsTarget, settings_target},
	settings_pen_tests::{buttons, inside, record, texts},
	settings_view::{MIN_SIZE, compose_main_settings},
};
use crate::{
	config::Preferences,
	fonts::{BUNDLED, Font},
	input::shortcuts::Shortcuts,
	theme::ThemeKind,
	ui::diagnostics_page::Section,
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
		copied: "Copied the report.".into(),
	}
}

#[test]
fn every_header_button_drawn_opens_the_page_a_click_there_opens() {
	let pages = HEADER_BUTTONS.map(|(page, ..)| page);
	assert_eq!(
		pages,
		[
			SettingsPage::Fonts,
			SettingsPage::Layout,
			SettingsPage::Shortcuts,
			SettingsPage::Diagnostics,
			SettingsPage::About,
		]
	);
	for s in [1, 2] {
		for (width, height) in [MIN_SIZE, (760, 640), (1300, 900)] {
			let size = (width * s, height * s);
			let commands = record(s, |pen, p, style| {
				(p.width, p.height) = size;
				compose_main_settings(&editor(), style, p, pen, &|_, _, _, _| false)
			});
			let header: Vec<_> = buttons(&commands)
				.into_iter()
				.map(|(rect, _)| rect)
				.filter(|rect| rect.1 == 20 * s)
				.collect();
			assert_eq!(header.len(), pages.len(), "x{s} {size:?}");
			let labels = texts(&commands);
			for (rect, (_, .., label)) in header.iter().zip(HEADER_BUTTONS) {
				let (_, at) = labels.iter().find(|(text, _)| text == label).unwrap();
				assert!(inside(*at, *rect), "{label} outside {rect:?}");
			}
			for y in (20 * s..56 * s).step_by(2) {
				for x in (0..size.0).step_by(2) {
					let under = header.iter().position(|&rect| inside((x, y), rect));
					assert_eq!(
						settings_target((x, y), size, s, SettingsPage::Main),
						under.map(|index| SettingsTarget::Open(pages[index])),
						"x{s} {size:?} at ({x}, {y})"
					);
				}
			}
		}
	}
}

#[test]
fn the_title_ends_before_the_first_header_button_in_the_smallest_window() {
	let commands = record(1, |pen, p, style| {
		(p.width, p.height) = MIN_SIZE;
		compose_main_settings(&editor(), style, p, pen, &|_, _, _, _| false)
	});
	let (title, (x, _)) = texts(&commands).swap_remove(0);
	assert_eq!(title, "Settings");
	let advance = Font::from_bytes(BUNDLED, 0)
		.unwrap()
		.metrics('M', 18.0)
		.advance_width;
	let end = x + (title.chars().count() as f32 * advance).ceil() as usize;
	let first = buttons(&commands)[0].0;
	assert!(
		end + 16 <= first.0,
		"title ends at {end}, Fonts starts at {}",
		first.0
	);
}

#[test]
fn opening_a_page_forgets_what_copy_report_said_and_keeps_the_section() {
	for page in [
		SettingsPage::Diagnostics,
		SettingsPage::About,
		SettingsPage::Main,
	] {
		let mut editor = editor();
		editor.open(page);
		assert_eq!(editor.page, page);
		assert!(editor.copied.is_empty(), "{page:?}");
		assert_eq!(editor.diagnostics_section, Section::Log, "{page:?}");
	}
}
