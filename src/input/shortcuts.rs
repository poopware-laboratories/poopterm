//! Configurable application shortcuts such as `ctrl+shift+p`.
use winit::keyboard::{Key, NamedKey};

use super::keys::Modifiers;
use crate::config;

/// One key chord: exact modifier set plus a character key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shortcut {
	mods: Modifiers,
	key: String,
}

impl Shortcut {
	/// Parses `"ctrl+shift+p"`. Modifier names are `ctrl`, `shift`, `alt`, `super`.
	pub(crate) fn parse(binding: &str) -> Option<Self> {
		let binding = binding.to_ascii_lowercase();
		let mut parts: Vec<&str> = binding.split('+').map(str::trim).collect();
		let key = parts.pop().filter(|key| !key.is_empty())?.to_owned();
		let has = |name: &str| parts.contains(&name);
		Some(Self {
			mods: Modifiers {
				ctrl: has("ctrl"),
				shift: has("shift"),
				alt: has("alt"),
				super_key: has("super"),
			},
			key,
		})
	}

	pub(crate) fn matches(&self, key: &Key, mods: Modifiers) -> bool {
		mods == self.mods
			&& match key {
				Key::Character(value) if self.key == "plus" => value == "+",
				Key::Character(value) => value.eq_ignore_ascii_case(&self.key),
				Key::Named(named) => named_key(&self.key) == Some(*named),
				_ => false,
			}
	}

	/// Captures one modified character or supported named key.
	pub(crate) fn from_event(key: &Key, mods: Modifiers) -> Option<Self> {
		if !(mods.ctrl || mods.shift || mods.alt || mods.super_key) {
			return None;
		}
		let key = match key {
			Key::Character(value)
				if value.is_ascii()
					&& value.chars().count() == 1
					&& value.as_bytes()[0].is_ascii_graphic() =>
			{
				if value == "+" {
					"plus".to_owned()
				} else {
					value.to_ascii_lowercase()
				}
			}
			Key::Named(named) => named_key_name(*named)?.to_owned(),
			_ => return None,
		};
		Some(Self { mods, key })
	}

	/// Serializes this chord using the syntax accepted by the config file.
	pub(crate) fn binding(&self) -> String {
		let mut parts = Vec::with_capacity(5);
		for (name, enabled) in [
			("ctrl", self.mods.ctrl),
			("shift", self.mods.shift),
			("alt", self.mods.alt),
			("super", self.mods.super_key),
		] {
			if enabled {
				parts.push(name);
			}
		}
		parts.push(&self.key);
		parts.join("+")
	}

	/// Formats a chord for the Settings window.
	pub(crate) fn display(&self, mac: bool) -> String {
		let mut parts = Vec::with_capacity(5);
		for (name, enabled) in [
			(if mac { "⌃" } else { "Ctrl" }, self.mods.ctrl),
			(if mac { "⌥" } else { "Alt" }, self.mods.alt),
			(if mac { "⇧" } else { "Shift" }, self.mods.shift),
			(if mac { "⌘" } else { "Super" }, self.mods.super_key),
		] {
			if enabled {
				parts.push(name.to_owned());
			}
		}
		parts.push(match self.key.as_str() {
			"plus" => "+".to_owned(),
			"up" => "↑".to_owned(),
			"down" => "↓".to_owned(),
			"left" => "←".to_owned(),
			"right" => "→".to_owned(),
			"pageup" => "PageUp".to_owned(),
			"pagedown" => "PageDown".to_owned(),
			"home" => "Home".to_owned(),
			"end" => "End".to_owned(),
			"tab" => "Tab".to_owned(),
			"enter" => "Enter".to_owned(),
			"space" => "Space".to_owned(),
			key => key.to_ascii_uppercase(),
		});
		parts.join(if mac { "" } else { "+" })
	}
}

/// Named keys accepted in bindings, e.g. `ctrl+shift+up`.
fn named_key(name: &str) -> Option<NamedKey> {
	Some(match name {
		"up" => NamedKey::ArrowUp,
		"down" => NamedKey::ArrowDown,
		"left" => NamedKey::ArrowLeft,
		"right" => NamedKey::ArrowRight,
		"pageup" => NamedKey::PageUp,
		"pagedown" => NamedKey::PageDown,
		"home" => NamedKey::Home,
		"end" => NamedKey::End,
		"tab" => NamedKey::Tab,
		"enter" => NamedKey::Enter,
		"space" => NamedKey::Space,
		_ => return None,
	})
}

fn named_key_name(key: NamedKey) -> Option<&'static str> {
	Some(match key {
		NamedKey::ArrowUp => "up",
		NamedKey::ArrowDown => "down",
		NamedKey::ArrowLeft => "left",
		NamedKey::ArrowRight => "right",
		NamedKey::PageUp => "pageup",
		NamedKey::PageDown => "pagedown",
		NamedKey::Home => "home",
		NamedKey::End => "end",
		NamedKey::Tab => "tab",
		NamedKey::Enter => "enter",
		NamedKey::Space => "space",
		_ => return None,
	})
}

/// Actions with configurable keyboard shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShortcutAction {
	NewTab,
	CommandPalette,
	CycleTheme,
	Search,
	CloseTab,
	PreviousPrompt,
	NextPrompt,
	ZoomIn,
	ZoomOut,
	ZoomReset,
}

impl ShortcutAction {
	pub(crate) const ALL: [Self; 10] = [
		Self::NewTab,
		Self::CommandPalette,
		Self::CycleTheme,
		Self::Search,
		Self::CloseTab,
		Self::PreviousPrompt,
		Self::NextPrompt,
		Self::ZoomIn,
		Self::ZoomOut,
		Self::ZoomReset,
	];

	pub(crate) fn label(self) -> &'static str {
		match self {
			Self::NewTab => "New tab",
			Self::CommandPalette => "Command palette",
			Self::CycleTheme => "Cycle theme",
			Self::Search => "Search",
			Self::CloseTab => "Close tab",
			Self::PreviousPrompt => "Previous prompt",
			Self::NextPrompt => "Next prompt",
			Self::ZoomIn => "Zoom in",
			Self::ZoomOut => "Zoom out",
			Self::ZoomReset => "Reset zoom",
		}
	}

	pub(crate) fn config_key(self) -> &'static str {
		match self {
			Self::NewTab => "shortcut_new_tab",
			Self::CommandPalette => "shortcut_command_palette",
			Self::CycleTheme => "shortcut_cycle_theme",
			Self::Search => "shortcut_search",
			Self::CloseTab => "shortcut_close_tab",
			Self::PreviousPrompt => "shortcut_previous_prompt",
			Self::NextPrompt => "shortcut_next_prompt",
			Self::ZoomIn => "shortcut_zoom_in",
			Self::ZoomOut => "shortcut_zoom_out",
			Self::ZoomReset => "shortcut_zoom_reset",
		}
	}
}

/// Shortcut table, read from the config once instead of on every key press.
#[derive(Clone, Debug)]
pub(crate) struct Shortcuts {
	pub new_tab: Shortcut,
	pub command_palette: Shortcut,
	pub cycle_theme: Shortcut,
	pub search: Shortcut,
	pub close_tab: Shortcut,
	pub previous_prompt: Shortcut,
	pub next_prompt: Shortcut,
	pub zoom_in: Shortcut,
	pub zoom_out: Shortcut,
	pub zoom_reset: Shortcut,
}

impl Shortcuts {
	/// Builds the table from config text, falling back to platform defaults.
	pub(crate) fn from_config(text: &str) -> Self {
		let mac = cfg!(target_os = "macos");
		let primary = if mac { "super" } else { "ctrl" };
		let get = |key: &str, default: &str| {
			config::value(text, key)
				.and_then(|binding| Shortcut::parse(&binding))
				.or_else(|| Shortcut::parse(default))
				.expect("default shortcut is valid")
		};
		Self {
			new_tab: get(
				"shortcut_new_tab",
				if mac { "super+t" } else { "ctrl+shift+n" },
			),
			command_palette: get("shortcut_command_palette", "ctrl+shift+p"),
			cycle_theme: get("shortcut_cycle_theme", "ctrl+shift+t"),
			search: get(
				"shortcut_search",
				if mac { "super+f" } else { "ctrl+shift+f" },
			),
			close_tab: get(
				"shortcut_close_tab",
				if mac { "super+w" } else { "ctrl+shift+w" },
			),
			previous_prompt: get(
				"shortcut_previous_prompt",
				if mac { "super+up" } else { "ctrl+shift+up" },
			),
			next_prompt: get(
				"shortcut_next_prompt",
				if mac { "super+down" } else { "ctrl+shift+down" },
			),
			zoom_in: get("shortcut_zoom_in", &format!("{primary}+=")),
			zoom_out: get("shortcut_zoom_out", &format!("{primary}+-")),
			zoom_reset: get("shortcut_zoom_reset", &format!("{primary}+0")),
		}
	}

	pub(crate) fn load() -> Self {
		Self::from_config(&std::fs::read_to_string(config::config_path()).unwrap_or_default())
	}

	pub(crate) fn get(&self, action: ShortcutAction) -> &Shortcut {
		match action {
			ShortcutAction::NewTab => &self.new_tab,
			ShortcutAction::CommandPalette => &self.command_palette,
			ShortcutAction::CycleTheme => &self.cycle_theme,
			ShortcutAction::Search => &self.search,
			ShortcutAction::CloseTab => &self.close_tab,
			ShortcutAction::PreviousPrompt => &self.previous_prompt,
			ShortcutAction::NextPrompt => &self.next_prompt,
			ShortcutAction::ZoomIn => &self.zoom_in,
			ShortcutAction::ZoomOut => &self.zoom_out,
			ShortcutAction::ZoomReset => &self.zoom_reset,
		}
	}

	pub(crate) fn set(&mut self, action: ShortcutAction, shortcut: Shortcut) {
		match action {
			ShortcutAction::NewTab => self.new_tab = shortcut,
			ShortcutAction::CommandPalette => self.command_palette = shortcut,
			ShortcutAction::CycleTheme => self.cycle_theme = shortcut,
			ShortcutAction::Search => self.search = shortcut,
			ShortcutAction::CloseTab => self.close_tab = shortcut,
			ShortcutAction::PreviousPrompt => self.previous_prompt = shortcut,
			ShortcutAction::NextPrompt => self.next_prompt = shortcut,
			ShortcutAction::ZoomIn => self.zoom_in = shortcut,
			ShortcutAction::ZoomOut => self.zoom_out = shortcut,
			ShortcutAction::ZoomReset => self.zoom_reset = shortcut,
		}
	}

	pub(crate) fn conflict(
		&self,
		action: ShortcutAction,
		shortcut: &Shortcut,
	) -> Option<ShortcutAction> {
		ShortcutAction::ALL
			.into_iter()
			.find(|other| *other != action && self.get(*other) == shortcut)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn chr(value: &str) -> Key {
		Key::Character(value.into())
	}

	#[test]
	fn modifiers_must_match_exactly() {
		let shortcut = Shortcut::parse("ctrl+shift+p").unwrap();
		let ctrl_shift = Modifiers {
			ctrl: true,
			shift: true,
			..Default::default()
		};
		assert!(shortcut.matches(&chr("P"), ctrl_shift));
		assert!(!shortcut.matches(
			&chr("p"),
			Modifiers {
				ctrl: true,
				..Default::default()
			}
		));
		assert!(!shortcut.matches(&chr("o"), ctrl_shift));
	}

	#[test]
	fn config_overrides_defaults_and_invalid_bindings_fall_back() {
		let shortcuts =
			Shortcuts::from_config("shortcut_search = \"alt+s\"\nshortcut_new_tab = \"\"\n");
		let alt = Modifiers {
			alt: true,
			..Default::default()
		};
		assert!(shortcuts.search.matches(&chr("s"), alt));
		assert_eq!(
			shortcuts.new_tab,
			Shortcut::parse(if cfg!(target_os = "macos") {
				"super+t"
			} else {
				"ctrl+shift+n"
			})
			.unwrap()
		);
	}

	#[test]
	fn named_keys_can_be_bound() {
		use winit::keyboard::NamedKey;
		let up = Shortcut::parse("ctrl+shift+up").unwrap();
		let ctrl_shift = Modifiers {
			ctrl: true,
			shift: true,
			..Default::default()
		};
		assert!(up.matches(&Key::Named(NamedKey::ArrowUp), ctrl_shift));
		assert!(!up.matches(&Key::Named(NamedKey::ArrowDown), ctrl_shift));
		assert!(!up.matches(&chr("u"), ctrl_shift));
		let page = Shortcut::parse("shift+pagedown").unwrap();
		let shift = Modifiers {
			shift: true,
			..Default::default()
		};
		assert!(page.matches(&Key::Named(NamedKey::PageDown), shift));
	}

	#[test]
	fn captured_bindings_round_trip_and_require_a_modifier() {
		let ctrl_shift = Modifiers {
			ctrl: true,
			shift: true,
			..Default::default()
		};
		let captured = Shortcut::from_event(&chr("K"), ctrl_shift).unwrap();
		assert_eq!(captured.binding(), "ctrl+shift+k");
		assert_eq!(captured.display(false), "Ctrl+Shift+K");
		let plus = Shortcut::from_event(&chr("+"), ctrl_shift).unwrap();
		assert_eq!(plus.binding(), "ctrl+shift+plus");
		assert!(
			Shortcut::parse(&plus.binding())
				.unwrap()
				.matches(&chr("+"), ctrl_shift)
		);
		assert!(Shortcut::from_event(&chr("k"), Modifiers::default()).is_none());
	}

	#[test]
	fn shortcut_actions_can_be_updated_and_conflicts_are_reported() {
		let mut shortcuts = Shortcuts::from_config("");
		let binding = Shortcut::parse("ctrl+alt+n").unwrap();
		shortcuts.set(ShortcutAction::NewTab, binding.clone());
		assert_eq!(
			shortcuts.get(ShortcutAction::NewTab).binding(),
			"ctrl+alt+n"
		);
		assert_eq!(shortcuts.conflict(ShortcutAction::NewTab, &binding), None);
		assert_eq!(
			shortcuts.conflict(ShortcutAction::Search, &binding),
			Some(ShortcutAction::NewTab)
		);
	}
}
