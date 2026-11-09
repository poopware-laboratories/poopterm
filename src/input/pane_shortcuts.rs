//! Shortcuts that split a tab into panes and move focus between them: their
//! config keys, labels and defaults, and which one a key press runs.
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

type Binding = (
	&'static str,
	&'static str,
	PaneCommand,
	&'static str,
	&'static str,
);

/// Each binding's config key, Settings label, what it does, and its macOS
/// and other default. Elsewhere brackets go with ctrl+alt: shift makes them
/// braces.
#[rustfmt::skip]
const BINDINGS: [Binding; 8] = [
	("shortcut_split_right", "Split right", PaneCommand::Split(Split::Right), "super+d", "ctrl+shift+d"),
	("shortcut_split_down", "Split down", PaneCommand::Split(Split::Down), "super+shift+d", "ctrl+shift+e"),
	("shortcut_focus_pane_left", "Focus pane left", PaneCommand::Focus(Direction::Left), "super+alt+left", "ctrl+alt+left"),
	("shortcut_focus_pane_right", "Focus pane right", PaneCommand::Focus(Direction::Right), "super+alt+right", "ctrl+alt+right"),
	("shortcut_focus_pane_up", "Focus pane above", PaneCommand::Focus(Direction::Up), "super+alt+up", "ctrl+alt+up"),
	("shortcut_focus_pane_down", "Focus pane below", PaneCommand::Focus(Direction::Down), "super+alt+down", "ctrl+alt+down"),
	("shortcut_next_pane", "Next pane", PaneCommand::Cycle(1), "super+]", "ctrl+alt+]"),
	("shortcut_previous_pane", "Previous pane", PaneCommand::Cycle(-1), "super+[", "ctrl+alt+["),
];

/// Every pane command, in the order Settings lists them.
pub(crate) const COMMANDS: [PaneCommand; 8] = {
	let mut commands = [PaneCommand::Cycle(0); 8];
	let mut i = 0;
	while i < BINDINGS.len() {
		commands[i] = BINDINGS[i].2;
		i += 1;
	}
	commands
};

/// `command`'s binding row.
fn binding(command: PaneCommand) -> &'static Binding {
	(BINDINGS.iter())
		.find(|binding| binding.2 == command)
		.expect("every command has a binding")
}

impl PaneCommand {
	/// The command's name in Settings.
	pub(crate) fn label(self) -> &'static str {
		binding(self).1
	}

	/// The config key holding the command's shortcut.
	pub(crate) fn config_key(self) -> &'static str {
		binding(self).0
	}
}

/// The pane shortcuts, read from the config with the others.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaneShortcuts {
	bindings: Vec<(Shortcut, PaneCommand)>,
}

impl PaneShortcuts {
	/// The bindings in config `text`, the platform's defaults for missing
	/// or invalid ones; `mac` picks macOS's defaults.
	pub(crate) fn from_config(text: &str, mac: bool) -> Self {
		let bindings = BINDINGS
			.iter()
			.map(|&(key, _, command, on_mac, elsewhere)| {
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

	/// The shortcut that runs `command`.
	pub(crate) fn get(&self, command: PaneCommand) -> &Shortcut {
		let bound = self.bindings.iter().find(|(_, bound)| *bound == command);
		&bound.expect("every command is bound").0
	}

	/// Binds `command` to `shortcut` instead.
	pub(crate) fn set(&mut self, command: PaneCommand, shortcut: Shortcut) {
		if let Some(bound) = self
			.bindings
			.iter_mut()
			.find(|(_, bound)| *bound == command)
		{
			bound.0 = shortcut;
		}
	}

	/// The command `key` pressed with `mods` runs, if any.
	pub(crate) fn command(&self, key: &Key, mods: Modifiers) -> Option<PaneCommand> {
		(self.bindings.iter())
			.find(|(shortcut, _)| shortcut.matches(key, mods))
			.map(|&(_, command)| command)
	}
}
