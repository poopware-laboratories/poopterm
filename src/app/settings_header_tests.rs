//! Tests of opening a Settings page, which clears Copy report's word.
use super::{SettingsEditor, settings_fonts::FontPicker, settings_hit::SettingsPage};
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
		copied: "Copied the report.".into(),
	}
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
