//! Tests of the Settings window's Clipboard page: each clipboard and tab
//! cycling shortcut has a row that records a new binding, the rows clear
//! the footer at the smallest window size, the sidebar reaches the page,
//! and a saved binding comes back from the config.
use super::{
	SettingsEditor,
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_pen_tests::{record, texts},
	settings_rows_tests::editor,
	settings_view::{CLIPBOARD_ROWS, MIN_SIZE, PANE_ROWS, ROW_HEIGHT, compose_shortcuts},
};
use crate::{
	config::{quote_config_string, updated},
	input::shortcuts::{Shortcut, ShortcutAction, Shortcuts},
	render::scene::Command,
};

const SIZE: (usize, usize) = (760, 640);

fn target(page: SettingsPage, (x, y): (usize, usize), scale: usize) -> Option<SettingsTarget> {
	let size = (SIZE.0 * scale, SIZE.1 * scale);
	settings_target((x * scale, y * scale), size, scale, page)
}

#[test]
fn every_clipboard_shortcut_has_a_row_that_records_it() {
	let actions: Vec<_> = CLIPBOARD_ROWS.iter().map(|&(action, _)| action).collect();
	assert_eq!(actions, ShortcutAction::CLIPBOARD);
	for scale in [1, 2] {
		for (i, &(action, top)) in CLIPBOARD_ROWS.iter().enumerate() {
			assert_eq!(top, 104 + 44 * i, "rows sit as the other shortcut pages'");
			let hit = |y| target(SettingsPage::Clipboard, (600, y), scale);
			assert_eq!(hit(top + 1), Some(SettingsTarget::SetShortcut(action)));
			assert_eq!(
				hit(top + ROW_HEIGHT - 1),
				Some(SettingsTarget::SetShortcut(action))
			);
			assert_eq!(hit(top + ROW_HEIGHT + 2), None, "gap below {action:?}");
		}
	}
	let (_, last) = CLIPBOARD_ROWS[CLIPBOARD_ROWS.len() - 1];
	assert!(
		last + ROW_HEIGHT <= MIN_SIZE.1 - 52,
		"clear of the Save button"
	);
}

#[test]
fn the_page_keeps_its_own_shortcuts_beside_the_panes_page() {
	for (&(clipboard, top), &(pane, _)) in CLIPBOARD_ROWS.iter().zip(&PANE_ROWS) {
		let on = |page| target(page, (600, top + 1), 1);
		assert_eq!(
			on(SettingsPage::Clipboard),
			Some(SettingsTarget::SetShortcut(clipboard))
		);
		assert_eq!(
			on(SettingsPage::Panes),
			Some(SettingsTarget::SetShortcut(pane))
		);
	}
	// Below its four rows the page has nothing to click.
	for &(_, top) in &PANE_ROWS[CLIPBOARD_ROWS.len()..] {
		assert_eq!(target(SettingsPage::Clipboard, (600, top + 1), 1), None);
	}
	for &(_, top) in &CLIPBOARD_ROWS {
		assert_eq!(
			target(SettingsPage::Clipboard, (40, top + 5), 1),
			None,
			"left of the button"
		);
	}
}

/// The Clipboard page drawn for `editor`, the pointer away from it.
fn drawn(editor: &SettingsEditor) -> Vec<String> {
	let commands: Vec<Command> = record(1, |pen, p, style| {
		let page = (
			"Clipboard and tab shortcuts",
			"Copy and paste.",
			&CLIPBOARD_ROWS[..],
		);
		compose_shortcuts(editor, page, style, p, pen, &|_, _, _, _| false)
	});
	texts(&commands).into_iter().map(|(text, _)| text).collect()
}

#[test]
fn the_page_shows_each_action_with_its_binding() {
	let mut editor = editor();
	let mac = cfg!(target_os = "macos");
	editor.shortcuts.set(
		ShortcutAction::NextTab,
		Shortcut::parse("alt+right").unwrap(),
	);
	let shown = drawn(&editor);
	assert_eq!(
		shown[..2],
		["Clipboard and tab shortcuts", "Copy and paste."]
	);
	let pairs: Vec<_> = shown[2..]
		.chunks(2)
		.map(|pair| (pair[0].as_str(), pair[1].clone()))
		.collect();
	let binding = |action| editor.shortcuts.get(action).display(mac);
	assert_eq!(
		pairs,
		[
			("Copy", binding(ShortcutAction::Copy)),
			("Paste", binding(ShortcutAction::Paste)),
			("Previous tab", binding(ShortcutAction::PreviousTab)),
			(
				"Next tab",
				Shortcut::parse("alt+right").unwrap().display(mac)
			),
		]
	);
}

#[test]
fn the_sidebar_reaches_every_row_at_every_scale() {
	use super::settings_nav::{NAV_WIDTH, window_target};
	for scale in [1, 2, 3] {
		let window = ((MIN_SIZE.0 + NAV_WIDTH) * scale, MIN_SIZE.1 * scale);
		for &(action, top) in &CLIPBOARD_ROWS {
			let at = ((NAV_WIDTH + 500) * scale, (top + 20) * scale);
			assert_eq!(
				window_target(at, window, scale, SettingsPage::Clipboard),
				Some(SettingsTarget::SetShortcut(action)),
				"{action:?} at {scale}x"
			);
		}
	}
}

#[test]
fn bindings_saved_from_the_page_are_read_back_from_the_config() {
	// As saving does: every action's key with its quoted binding.
	let mut shortcuts = Shortcuts::from_config("");
	for (action, chord) in [
		(ShortcutAction::Copy, "ctrl+alt+c"),
		(ShortcutAction::Paste, "shift+insert"),
		(ShortcutAction::PreviousTab, "ctrl+pageup"),
		(ShortcutAction::NextTab, "ctrl+pagedown"),
	] {
		shortcuts.set(action, Shortcut::parse(chord).unwrap());
	}
	let values: Vec<_> = ShortcutAction::ALL
		.into_iter()
		.map(|action| {
			(
				action.config_key(),
				quote_config_string(&shortcuts.get(action).binding()),
			)
		})
		.collect();
	let read = Shortcuts::from_config(&updated("font_size = 14\n", &values));
	for action in ShortcutAction::CLIPBOARD {
		assert_eq!(read.get(action), shortcuts.get(action), "{action:?}");
	}
}
