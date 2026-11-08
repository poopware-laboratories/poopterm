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
