//! Shortcuts that split a tab into panes and move focus between them, set
//! in the config file only (Settings lists the tab and window shortcuts).
use winit::keyboard::Key;

use super::{keys::Modifiers, shortcuts::Shortcut};
use crate::{
	config,
	ui::panes::{Direction, Split},
};

/// What a pane shortcut does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneCommand {
	Split(Split),
	/// Focus the nearest pane that way.
	Focus(Direction),
	/// Focus the next (1) or previous (-1) pane.
	Cycle(isize),
}

/// The config keys, what each does, and its macOS and other default.
/// Elsewhere brackets go with ctrl+alt: shift turns them into braces.
const BINDINGS: [(&str, PaneCommand, &str, &str); 8] = [
	(
		"shortcut_split_right",
		PaneCommand::Split(Split::Right),
		"super+d",
		"ctrl+shift+d",
	),
	(
		"shortcut_split_down",
		PaneCommand::Split(Split::Down),
		"super+shift+d",
		"ctrl+shift+e",
	),
	(
		"shortcut_focus_pane_left",
		PaneCommand::Focus(Direction::Left),
		"super+alt+left",
		"ctrl+alt+left",
	),
	(
		"shortcut_focus_pane_right",
		PaneCommand::Focus(Direction::Right),
		"super+alt+right",
		"ctrl+alt+right",
	),
	(
		"shortcut_focus_pane_up",
		PaneCommand::Focus(Direction::Up),
		"super+alt+up",
		"ctrl+alt+up",
	),
	(
		"shortcut_focus_pane_down",
		PaneCommand::Focus(Direction::Down),
		"super+alt+down",
		"ctrl+alt+down",
	),
	(
		"shortcut_next_pane",
		PaneCommand::Cycle(1),
		"super+]",
		"ctrl+alt+]",
	),
	(
		"shortcut_previous_pane",
		PaneCommand::Cycle(-1),
		"super+[",
		"ctrl+alt+[",
	),
];

/// The pane shortcuts, read from the config with the others.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaneShortcuts {
	bindings: Vec<(Shortcut, PaneCommand)>,
}

impl PaneShortcuts {
	/// The bindings in config `text`, the platform's defaults for missing
	/// or invalid ones; `mac` picks macOS's defaults.
	pub(crate) fn from_config(text: &str, mac: bool) -> Self {
		let bindings = BINDINGS.iter().map(|&(key, command, on_mac, elsewhere)| {
			let shortcut = config::value(text, key)
				.and_then(|binding| Shortcut::parse(&binding))
				.or_else(|| Shortcut::parse(if mac { on_mac } else { elsewhere }))
				.expect("default shortcut is valid");
			(shortcut, command)
		});
		Self {
			bindings: bindings.collect(),
		}
	}

	/// The command `key` pressed with `mods` runs, if any.
	pub(crate) fn command(&self, key: &Key, mods: Modifiers) -> Option<PaneCommand> {
		(self.bindings.iter())
			.find(|(shortcut, _)| shortcut.matches(key, mods))
			.map(|&(_, command)| command)
	}
}
