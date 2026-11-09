//! Tests of the shortcut actions Settings edits: the tab and window ones and
//! the pane ones together, each with its own label and config key, read,
//! changed and checked for conflicts through one table.
use super::{
	keys::Modifiers,
	pane_shortcuts::{COMMANDS, PaneCommand},
	shortcuts::{Shortcut, ShortcutAction, Shortcuts},
};
use crate::ui::panes::{Direction, Split};
use winit::keyboard::Key;

#[test]
fn every_action_is_listed_once_tabs_first_then_panes() {
	let all = ShortcutAction::ALL;
	assert_eq!(all[..10], ShortcutAction::TABS);
	assert_eq!(all[10..], ShortcutAction::PANES);
	for (i, action) in all.iter().enumerate() {
		assert!(!all[i + 1..].contains(action), "{action:?} twice");
	}
	let panes: Vec<_> = COMMANDS
		.iter()
		.map(|&command| ShortcutAction::Pane(command))
		.collect();
	assert_eq!(ShortcutAction::PANES[..], panes[..]);
}

#[test]
fn labels_and_config_keys_are_unique() {
	let all = ShortcutAction::ALL;
	for (i, a) in all.iter().enumerate() {
		for b in &all[i + 1..] {
			assert_ne!(a.label(), b.label(), "{a:?} {b:?}");
			assert_ne!(a.config_key(), b.config_key(), "{a:?} {b:?}");
		}
		assert!(a.config_key().starts_with("shortcut_"), "{a:?}");
	}
}

#[test]
fn pane_actions_name_their_commands() {
	let named = [
		(
			PaneCommand::Split(Split::Right),
			"Split right",
			"shortcut_split_right",
		),
		(
			PaneCommand::Split(Split::Down),
			"Split down",
			"shortcut_split_down",
		),
		(
			PaneCommand::Focus(Direction::Left),
			"Focus pane left",
			"shortcut_focus_pane_left",
		),
		(
			PaneCommand::Focus(Direction::Right),
			"Focus pane right",
			"shortcut_focus_pane_right",
		),
		(
			PaneCommand::Focus(Direction::Up),
			"Focus pane above",
			"shortcut_focus_pane_up",
		),
		(
			PaneCommand::Focus(Direction::Down),
			"Focus pane below",
			"shortcut_focus_pane_down",
		),
		(PaneCommand::Cycle(1), "Next pane", "shortcut_next_pane"),
		(
			PaneCommand::Cycle(-1),
			"Previous pane",
			"shortcut_previous_pane",
		),
	];
	for (command, label, key) in named {
		let action = ShortcutAction::Pane(command);
		assert_eq!(action.label(), label);
		assert_eq!(action.config_key(), key);
		assert_eq!(command.label(), label);
		assert_eq!(command.config_key(), key);
	}
}

#[test]
fn a_pane_shortcut_changed_in_the_table_is_the_one_that_runs() {
	let mut shortcuts = Shortcuts::from_config("");
	let split = ShortcutAction::Pane(PaneCommand::Split(Split::Right));
	let chord = Shortcut::parse("alt+shift+v").unwrap();
	shortcuts.set(split, chord.clone());
	assert_eq!(shortcuts.get(split), &chord);
	let alt_shift = Modifiers {
		alt: true,
		shift: true,
		..Default::default()
	};
	let pressed = Key::Character("V".into());
	assert_eq!(
		shortcuts.panes.command(&pressed, alt_shift),
		Some(PaneCommand::Split(Split::Right))
	);
	// The others keep theirs.
	let down = ShortcutAction::Pane(PaneCommand::Split(Split::Down));
	assert_eq!(shortcuts.get(down), Shortcuts::from_config("").get(down));
}

#[test]
fn reading_a_pane_shortcut_gives_what_the_config_set() {
	let shortcuts = Shortcuts::from_config("shortcut_next_pane = \"alt+n\"\n");
	let next = ShortcutAction::Pane(PaneCommand::Cycle(1));
	assert_eq!(shortcuts.get(next).binding(), "alt+n");
}

#[test]
fn every_action_written_back_as_config_reads_the_same_table() {
	let mut shortcuts = Shortcuts::from_config("");
	for (n, action) in ShortcutAction::ALL.into_iter().enumerate() {
		let letter = (b'a' + n as u8) as char;
		shortcuts.set(
			action,
			Shortcut::parse(&format!("ctrl+alt+{letter}")).unwrap(),
		);
	}
	// As saving does: each action's key with its binding.
	let text: String = (ShortcutAction::ALL.iter())
		.map(|action| {
			format!(
				"{} = \"{}\"\n",
				action.config_key(),
				shortcuts.get(*action).binding()
			)
		})
		.collect();
	let read = Shortcuts::from_config(&text);
	for action in ShortcutAction::ALL {
		assert_eq!(read.get(action), shortcuts.get(action), "{action:?}");
	}
}

#[test]
fn conflicts_are_found_between_tab_and_pane_shortcuts() {
	let shortcuts = Shortcuts::from_config("");
	let split = ShortcutAction::Pane(PaneCommand::Split(Split::Right));
	let taken = shortcuts.get(split).clone();
	assert_eq!(
		shortcuts.conflict(ShortcutAction::NewTab, &taken),
		Some(split)
	);
	let search = shortcuts.get(ShortcutAction::Search).clone();
	assert_eq!(
		shortcuts.conflict(split, &search),
		Some(ShortcutAction::Search)
	);
	let next = ShortcutAction::Pane(PaneCommand::Cycle(1));
	let previous = shortcuts
		.get(ShortcutAction::Pane(PaneCommand::Cycle(-1)))
		.clone();
	assert_eq!(
		shortcuts.conflict(next, &previous),
		Some(ShortcutAction::Pane(PaneCommand::Cycle(-1)))
	);
	assert_eq!(shortcuts.conflict(split, &taken), None, "its own binding");
	let free = Shortcut::parse("ctrl+alt+shift+q").unwrap();
	for action in ShortcutAction::ALL {
		assert_eq!(shortcuts.conflict(action, &free), None, "{action:?}");
	}
}

#[test]
fn no_two_default_shortcuts_conflict_on_either_platform() {
	let shortcuts = Shortcuts::from_config("");
	for action in ShortcutAction::ALL {
		let own = shortcuts.get(action).clone();
		assert_eq!(
			shortcuts.conflict(action, &own),
			None,
			"{action:?} {}",
			own.binding()
		);
	}
}
