//! Tests of the Settings window's Panes page: every pane shortcut has a
//! row that records a new binding, all of them clear the footer at the
//! smallest window size, and the Shortcuts page keeps only its own.
use super::{
	SettingsEditor,
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_pen_tests::{buttons, record, texts},
	settings_rows_tests::editor,
	settings_view::{MIN_SIZE, PANE_ROWS, ROW_HEIGHT, SHORTCUT_ROWS, compose_shortcuts},
};
use crate::{
	input::{
		pane_shortcuts::PaneCommand,
		shortcuts::{Shortcut, ShortcutAction},
	},
	render::scene::Command,
	ui::panes::{Direction, Split},
};

const SIZE: (usize, usize) = (760, 640);

fn target(
	page: SettingsPage,
	(x, y): (usize, usize),
	size: (usize, usize),
	scale: usize,
) -> Option<SettingsTarget> {
	settings_target(
		(x * scale, y * scale),
		(size.0 * scale, size.1 * scale),
		scale,
		page,
	)
}

#[test]
fn every_pane_shortcut_has_a_row_that_records_it() {
	let actions: Vec<_> = PANE_ROWS.iter().map(|&(action, _)| action).collect();
	assert_eq!(actions, ShortcutAction::PANES);
	for scale in [1, 2] {
		for (i, &(action, top)) in PANE_ROWS.iter().enumerate() {
			assert_eq!(top, 104 + 44 * i, "rows sit as the Shortcuts page's");
			let hit = |y| target(SettingsPage::Panes, (600, y), SIZE, scale);
			assert_eq!(hit(top + 1), Some(SettingsTarget::SetShortcut(action)));
			assert_eq!(
				hit(top + ROW_HEIGHT - 1),
				Some(SettingsTarget::SetShortcut(action))
			);
			assert_eq!(hit(top + ROW_HEIGHT + 2), None, "gap below {action:?}");
		}
	}
}

#[test]
fn the_pane_rows_clear_the_save_button_in_the_smallest_window() {
	let (_, last) = PANE_ROWS[PANE_ROWS.len() - 1];
	assert!(
		last + ROW_HEIGHT <= MIN_SIZE.1 - 52,
		"clear of the Save button"
	);
	let (_, top) = PANE_ROWS[7];
	assert_eq!(
		target(SettingsPage::Panes, (500, top + 5), MIN_SIZE, 1),
		Some(SettingsTarget::SetShortcut(ShortcutAction::Pane(
			PaneCommand::Cycle(-1)
		)))
	);
}

#[test]
fn the_pages_keep_their_own_shortcuts() {
	for (&(tab, top), &(pane, _)) in SHORTCUT_ROWS.iter().zip(&PANE_ROWS) {
		let on = |page| target(page, (600, top + 1), SIZE, 1);
		assert_eq!(
			on(SettingsPage::Shortcuts),
			Some(SettingsTarget::SetShortcut(tab))
		);
		assert_eq!(
			on(SettingsPage::Panes),
			Some(SettingsTarget::SetShortcut(pane))
		);
	}
	// The Shortcuts page's last two rows have no pane row beside them.
	for &(_, top) in &SHORTCUT_ROWS[PANE_ROWS.len()..] {
		assert_eq!(target(SettingsPage::Panes, (600, top + 1), SIZE, 1), None);
	}
	assert!(
		PANE_ROWS
			.iter()
			.all(|(action, _)| matches!(action, ShortcutAction::Pane(_)))
	);
	assert!(
		SHORTCUT_ROWS
			.iter()
			.all(|(action, _)| !matches!(action, ShortcutAction::Pane(_)))
	);
}

#[test]
fn the_panes_page_lists_splits_then_focus_moves() {
	let commands: Vec<_> = (PANE_ROWS.iter())
		.map(|(action, _)| match action {
			ShortcutAction::Pane(command) => *command,
			other => panic!("{other:?} on the Panes page"),
		})
		.collect();
	assert_eq!(
		commands,
		[
			PaneCommand::Split(Split::Right),
			PaneCommand::Split(Split::Down),
			PaneCommand::Focus(Direction::Left),
			PaneCommand::Focus(Direction::Right),
			PaneCommand::Focus(Direction::Up),
			PaneCommand::Focus(Direction::Down),
			PaneCommand::Cycle(1),
			PaneCommand::Cycle(-1),
		]
	);
}

#[test]
fn clicks_left_of_the_buttons_record_nothing() {
	for &(_, top) in &PANE_ROWS {
		assert_eq!(target(SettingsPage::Panes, (40, top + 5), SIZE, 1), None);
	}
}

/// The Panes page drawn for `editor` at `scale`, the pointer at `cursor`
/// in physical pixels.
fn drawn(editor: &SettingsEditor, scale: usize, cursor: (usize, usize)) -> Vec<Command> {
	let hovered = |x: usize, y: usize, w: usize, h: usize| {
		(x..x + w).contains(&cursor.0) && (y..y + h).contains(&cursor.1)
	};
	record(scale, |pen, p, style| {
		let page = ("Pane shortcuts", "Split a tab into panes.", &PANE_ROWS[..]);
		compose_shortcuts(editor, page, style, p, pen, &hovered)
	})
}

const AWAY: (usize, usize) = (0, 0);

#[test]
fn the_page_shows_each_pane_action_with_its_binding() {
	let editor = editor();
	let mac = cfg!(target_os = "macos");
	for scale in [1, 2] {
		let shown: Vec<String> = texts(&drawn(&editor, scale, AWAY))
			.into_iter()
			.map(|(text, _)| text)
			.collect();
		assert_eq!(shown[..2], ["Pane shortcuts", "Split a tab into panes."]);
		for (i, &(action, _)) in PANE_ROWS.iter().enumerate() {
			// Each row draws its label, then its binding on the button.
			assert_eq!(shown[2 + 2 * i], action.label(), "{action:?}");
			let binding = editor.shortcuts.get(action).display(mac);
			assert_eq!(shown[3 + 2 * i], binding, "{action:?}");
		}
		assert_eq!(shown.len(), 2 + 2 * PANE_ROWS.len());
	}
}

#[test]
fn each_row_has_one_button_on_its_line_lit_only_under_the_pointer() {
	let editor = editor();
	let quiet = buttons(&drawn(&editor, 1, AWAY));
	assert_eq!(quiet.len(), PANE_ROWS.len());
	for ((rect, lit), &(_, top)) in quiet.iter().zip(&PANE_ROWS) {
		assert_eq!(rect.1, top, "on its row");
		assert_eq!(rect.3, ROW_HEIGHT);
		assert!(!lit);
	}
	let (rect, _) = quiet[3];
	let over = buttons(&drawn(&editor, 1, (rect.0 + 4, rect.1 + 4)));
	let lit: Vec<_> = over.iter().map(|(_, lit)| *lit).collect();
	assert_eq!(lit, [false, false, false, true, false, false, false, false]);
}

#[test]
fn a_row_being_recorded_asks_for_keys_and_stays_lit() {
	let mut editor = editor();
	let down = ShortcutAction::Pane(PaneCommand::Split(Split::Down));
	editor.recording_shortcut = Some(down);
	let commands = drawn(&editor, 1, AWAY);
	let shown: Vec<String> = texts(&commands).into_iter().map(|(text, _)| text).collect();
	assert_eq!(shown[5], "Press keys…");
	assert_eq!(
		shown.iter().filter(|text| *text == "Press keys…").count(),
		1
	);
	let lit: Vec<_> = buttons(&commands).iter().map(|(_, lit)| *lit).collect();
	assert_eq!(lit, [false, true, false, false, false, false, false, false]);
}

#[test]
fn a_changed_binding_shows_on_its_row() {
	let mut editor = editor();
	let next = ShortcutAction::Pane(PaneCommand::Cycle(1));
	editor
		.shortcuts
		.set(next, Shortcut::parse("ctrl+alt+n").unwrap());
	let shown: Vec<String> = texts(&drawn(&editor, 1, AWAY))
		.into_iter()
		.map(|(text, _)| text)
		.collect();
	let mac = cfg!(target_os = "macos");
	let expected = Shortcut::parse("ctrl+alt+n").unwrap().display(mac);
	// Next pane is the seventh row.
	assert_eq!(shown[2 + 2 * 6], "Next pane");
	assert_eq!(shown[3 + 2 * 6], expected);
}
