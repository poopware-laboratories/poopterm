//! Tests of the Settings window's Panes page: every pane shortcut has a
//! row that records a new binding, all of them clear the footer at the
//! smallest window size, and the Shortcuts page keeps only its own.
use super::{
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_view::{MIN_SIZE, PANE_ROWS, ROW_HEIGHT, SHORTCUT_ROWS},
};
use crate::input::{pane_shortcuts::PaneCommand, shortcuts::ShortcutAction};
use crate::ui::panes::{Direction, Split};

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
