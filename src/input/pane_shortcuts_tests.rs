//! Tests for the pane shortcuts' defaults, overrides and matching.
use super::{
	keys::Modifiers,
	pane_shortcuts::{PaneCommand, PaneShortcuts},
};
use crate::ui::panes::{Direction, Split};
use winit::keyboard::{Key, NamedKey};

fn chr(value: &str) -> Key {
	Key::Character(value.into())
}

fn mods(ctrl: bool, shift: bool, alt: bool, super_key: bool) -> Modifiers {
	Modifiers {
		ctrl,
		shift,
		alt,
		super_key,
	}
}

const CTRL_SHIFT: Modifiers = Modifiers {
	ctrl: true,
	shift: true,
	alt: false,
	super_key: false,
};

#[test]
fn defaults_elsewhere_split_with_ctrl_shift_and_focus_with_ctrl_alt_arrows() {
	let panes = PaneShortcuts::from_config("", false);
	let ctrl_alt = mods(true, false, true, false);
	assert_eq!(
		panes.command(&chr("D"), CTRL_SHIFT),
		Some(PaneCommand::Split(Split::Right))
	);
	assert_eq!(
		panes.command(&chr("e"), CTRL_SHIFT),
		Some(PaneCommand::Split(Split::Down))
	);
	for (key, direction) in [
		(NamedKey::ArrowLeft, Direction::Left),
		(NamedKey::ArrowRight, Direction::Right),
		(NamedKey::ArrowUp, Direction::Up),
		(NamedKey::ArrowDown, Direction::Down),
	] {
		assert_eq!(
			panes.command(&Key::Named(key), ctrl_alt),
			Some(PaneCommand::Focus(direction))
		);
	}
	assert_eq!(
		panes.command(&chr("]"), ctrl_alt),
		Some(PaneCommand::Cycle(1))
	);
	assert_eq!(
		panes.command(&chr("["), ctrl_alt),
		Some(PaneCommand::Cycle(-1))
	);
}

#[test]
fn macos_defaults_use_the_command_key() {
	let panes = PaneShortcuts::from_config("", true);
	let command = mods(false, false, false, true);
	assert_eq!(
		panes.command(&chr("d"), command),
		Some(PaneCommand::Split(Split::Right))
	);
	assert_eq!(
		panes.command(&chr("D"), mods(false, true, false, true)),
		Some(PaneCommand::Split(Split::Down))
	);
	assert_eq!(
		panes.command(
			&Key::Named(NamedKey::ArrowUp),
			mods(false, false, true, true)
		),
		Some(PaneCommand::Focus(Direction::Up))
	);
	assert_eq!(
		panes.command(&chr("]"), command),
		Some(PaneCommand::Cycle(1))
	);
	assert_eq!(panes.command(&chr("d"), CTRL_SHIFT), None);
}

#[test]
fn other_keys_and_modifiers_run_nothing() {
	let panes = PaneShortcuts::from_config("", false);
	assert_eq!(panes.command(&chr("d"), Modifiers::default()), None);
	assert_eq!(
		panes.command(&chr("d"), mods(true, false, false, false)),
		None
	);
	assert_eq!(panes.command(&chr("x"), CTRL_SHIFT), None);
	assert_eq!(
		panes.command(&Key::Named(NamedKey::ArrowLeft), CTRL_SHIFT),
		None,
		"ctrl+shift+arrows jump between prompts"
	);
}

#[test]
fn the_config_overrides_bindings_and_invalid_ones_keep_their_default() {
	let text = "shortcut_split_right = \"alt+r\"\nshortcut_split_down = \"\"\nshortcut_next_pane = \"ctrl+\"\n";
	let panes = PaneShortcuts::from_config(text, false);
	assert_eq!(
		panes.command(&chr("r"), mods(false, false, true, false)),
		Some(PaneCommand::Split(Split::Right))
	);
	assert_eq!(panes.command(&chr("d"), CTRL_SHIFT), None, "moved away");
	assert_eq!(
		panes.command(&chr("e"), CTRL_SHIFT),
		Some(PaneCommand::Split(Split::Down))
	);
	assert_eq!(
		panes.command(&chr("]"), mods(true, false, true, false)),
		Some(PaneCommand::Cycle(1))
	);
}

#[test]
fn the_shortcut_table_carries_the_pane_shortcuts() {
	let shortcuts = super::shortcuts::Shortcuts::from_config("shortcut_previous_pane = \"alt+p\"");
	assert_eq!(
		shortcuts
			.panes
			.command(&chr("p"), mods(false, false, true, false)),
		Some(PaneCommand::Cycle(-1))
	);
}

#[test]
fn defaults_do_not_take_the_existing_shortcuts() {
	use super::shortcuts::{ShortcutAction, Shortcuts};
	let panes = PaneShortcuts::from_config("", cfg!(target_os = "macos"));
	let table = Shortcuts::from_config("");
	for action in ShortcutAction::ALL {
		let binding = table.get(action).binding();
		let mut parts: Vec<&str> = binding.split('+').collect();
		let key = match parts.pop().unwrap() {
			"up" => Key::Named(NamedKey::ArrowUp),
			"down" => Key::Named(NamedKey::ArrowDown),
			"plus" => chr("+"),
			key => chr(key),
		};
		let has = |name| parts.contains(&name);
		let held = mods(has("ctrl"), has("shift"), has("alt"), has("super"));
		assert_eq!(panes.command(&key, held), None, "{action:?} {binding}");
	}
}

/// Every binding: its config key, what it runs, and an unused chord.
const EVERY: [(&str, PaneCommand); 8] = [
	("shortcut_split_right", PaneCommand::Split(Split::Right)),
	("shortcut_split_down", PaneCommand::Split(Split::Down)),
	(
		"shortcut_focus_pane_left",
		PaneCommand::Focus(Direction::Left),
	),
	(
		"shortcut_focus_pane_right",
		PaneCommand::Focus(Direction::Right),
	),
	("shortcut_focus_pane_up", PaneCommand::Focus(Direction::Up)),
	(
		"shortcut_focus_pane_down",
		PaneCommand::Focus(Direction::Down),
	),
	("shortcut_next_pane", PaneCommand::Cycle(1)),
	("shortcut_previous_pane", PaneCommand::Cycle(-1)),
];

#[test]
fn every_binding_reads_its_own_config_key() {
	let alt_shift = mods(false, true, true, false);
	for mac in [false, true] {
		for (n, (key, command)) in EVERY.into_iter().enumerate() {
			// Bind it to alt+shift and a digit no default uses.
			let text = format!("{key} = \"alt+shift+{n}\"\n");
			let panes = PaneShortcuts::from_config(&text, mac);
			let digit = n.to_string();
			assert_eq!(
				panes.command(&chr(&digit), alt_shift),
				Some(command),
				"{key}"
			);
			// The others keep their defaults.
			let defaults = PaneShortcuts::from_config("", mac);
			for (other, other_command) in EVERY {
				if other != key {
					let found = (0..10)
						.map(|n| n.to_string())
						.any(|digit| panes.command(&chr(&digit), alt_shift) == Some(other_command));
					assert!(!found, "{other} moved with {key}");
				}
			}
			assert_ne!(panes, defaults, "{key} changed nothing");
		}
	}
}

#[test]
fn no_two_default_bindings_share_a_chord() {
	use super::shortcuts::Shortcut;
	for mac in [false, true] {
		let panes = PaneShortcuts::from_config("", mac);
		// Each default chord runs exactly its own command.
		for (key, command) in EVERY {
			let found: Vec<_> = (["d", "e", "[", "]"].iter().map(|key| chr(key)))
				.chain(
					[
						NamedKey::ArrowLeft,
						NamedKey::ArrowRight,
						NamedKey::ArrowUp,
						NamedKey::ArrowDown,
					]
					.map(Key::Named),
				)
				.flat_map(|pressed| {
					(0..16).map(move |bits| {
						let held = mods(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
						(pressed.clone(), held)
					})
				})
				.filter(|(pressed, held)| panes.command(pressed, *held) == Some(command))
				.collect();
			assert!(!found.is_empty(), "{key} has no default chord");
			let chords: Vec<_> = (found.iter())
				.filter_map(|(pressed, held)| Shortcut::from_event(pressed, *held))
				.map(|shortcut| shortcut.binding())
				.collect();
			let mut unique = chords.clone();
			unique.dedup();
			assert_eq!(unique.len(), 1, "{key}: {chords:?}");
		}
	}
}

#[test]
fn the_other_platforms_defaults_written_as_config_read_back_the_same() {
	let mac = [
		"super+d",
		"super+shift+d",
		"super+alt+left",
		"super+alt+right",
		"super+alt+up",
		"super+alt+down",
		"super+]",
		"super+[",
	];
	let elsewhere = [
		"ctrl+shift+d",
		"ctrl+shift+e",
		"ctrl+alt+left",
		"ctrl+alt+right",
		"ctrl+alt+up",
		"ctrl+alt+down",
		"ctrl+alt+]",
		"ctrl+alt+[",
	];
	for (on_mac, chords) in [(true, mac), (false, elsewhere)] {
		let text: String = (EVERY.iter().zip(chords))
			.map(|((key, _), chord)| format!("{key} = \"{chord}\"\n"))
			.collect();
		// Config naming every chord overrides the platform's defaults.
		let written = PaneShortcuts::from_config(&text, !on_mac);
		assert_eq!(written, PaneShortcuts::from_config("", on_mac));
		assert_ne!(written, PaneShortcuts::from_config("", !on_mac));
	}
}
