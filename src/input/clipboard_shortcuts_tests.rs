//! Tests for the clipboard and tab cycling shortcuts: their defaults on
//! each platform, overrides from the config, exact matching, and how they
//! sit beside the other actions.
use super::{
	keys::Modifiers,
	shortcuts::{Shortcut, ShortcutAction, Shortcuts},
};
use winit::keyboard::{Key, NamedKey};

const MAC: bool = cfg!(target_os = "macos");
const CLIPBOARD: [ShortcutAction; 4] = ShortcutAction::CLIPBOARD;

fn chr(value: &str) -> Key {
	Key::Character(value.into())
}

fn tab() -> Key {
	Key::Named(NamedKey::Tab)
}

fn mods(chord: &str) -> Modifiers {
	Modifiers {
		ctrl: chord.contains("ctrl"),
		shift: chord.contains("shift"),
		alt: chord.contains("alt"),
		super_key: chord.contains("super"),
	}
}

/// The clipboard action `key` with `chord` runs in `shortcuts`.
fn action(shortcuts: &Shortcuts, key: &Key, chord: &str) -> Option<ShortcutAction> {
	shortcuts.matching(&CLIPBOARD, key, mods(chord))
}

#[test]
fn copy_and_paste_default_to_the_platform_chords() {
	let shortcuts = Shortcuts::from_config("");
	let primary = if MAC { "super" } else { "ctrl+shift" };
	assert_eq!(
		action(&shortcuts, &chr("c"), primary),
		Some(ShortcutAction::Copy)
	);
	assert_eq!(
		action(&shortcuts, &chr("V"), primary),
		Some(ShortcutAction::Paste)
	);
	let (copy, paste) = (shortcuts.copy.binding(), shortcuts.paste.binding());
	if MAC {
		assert_eq!((copy.as_str(), paste.as_str()), ("super+c", "super+v"));
	} else {
		assert_eq!(
			(copy.as_str(), paste.as_str()),
			("ctrl+shift+c", "ctrl+shift+v")
		);
	}
}

#[test]
fn ctrl_tab_cycles_forward_and_with_shift_back_by_default() {
	let shortcuts = Shortcuts::from_config("");
	assert_eq!(
		action(&shortcuts, &tab(), "ctrl"),
		Some(ShortcutAction::NextTab)
	);
	assert_eq!(
		action(&shortcuts, &tab(), "ctrl+shift"),
		Some(ShortcutAction::PreviousTab)
	);
	assert_eq!(shortcuts.next_tab.display(false), "Ctrl+Tab");
	assert_eq!(shortcuts.previous_tab.display(true), "⌃⇧Tab");
}

#[test]
fn chords_with_other_modifiers_are_left_to_the_program() {
	let shortcuts = Shortcuts::from_config("");
	// Plain Tab and Ctrl+C are the shell's: completion and interrupt.
	for (key, chord) in [
		(tab(), ""),
		(tab(), "shift"),
		(tab(), "ctrl+alt"),
		(chr("c"), "ctrl"),
		(chr("v"), "ctrl"),
		(chr("c"), "ctrl+shift+alt"),
		(chr("x"), if MAC { "super" } else { "ctrl+shift" }),
	] {
		assert_eq!(
			action(&shortcuts, &key, chord),
			None,
			"{key:?} with {chord:?}"
		);
	}
}

#[test]
fn the_config_rebinds_each_one() {
	let shortcuts = Shortcuts::from_config(
		"shortcut_copy = \"alt+c\"\nshortcut_paste = \"alt+v\"\n\
		 shortcut_previous_tab = \"alt+left\"\nshortcut_next_tab = \"alt+right\"\n",
	);
	let right = Key::Named(NamedKey::ArrowRight);
	let left = Key::Named(NamedKey::ArrowLeft);
	assert_eq!(
		action(&shortcuts, &chr("c"), "alt"),
		Some(ShortcutAction::Copy)
	);
	assert_eq!(
		action(&shortcuts, &chr("v"), "alt"),
		Some(ShortcutAction::Paste)
	);
	assert_eq!(
		action(&shortcuts, &left, "alt"),
		Some(ShortcutAction::PreviousTab)
	);
	assert_eq!(
		action(&shortcuts, &right, "alt"),
		Some(ShortcutAction::NextTab)
	);
	// The old chords are free for the program again.
	assert_eq!(action(&shortcuts, &tab(), "ctrl"), None);
	let primary = if MAC { "super" } else { "ctrl+shift" };
	assert_eq!(action(&shortcuts, &chr("c"), primary), None);
}

#[test]
fn unusable_bindings_fall_back_to_the_defaults() {
	let shortcuts = Shortcuts::from_config("shortcut_copy = \"\"\nshortcut_next_tab = \"ctrl+\"\n");
	let defaults = Shortcuts::from_config("");
	assert_eq!(shortcuts.copy, defaults.copy);
	assert_eq!(shortcuts.next_tab, defaults.next_tab);
}

#[test]
fn the_clipboard_actions_come_last_with_their_own_labels_and_keys() {
	let all = ShortcutAction::ALL;
	assert_eq!(all[18..], CLIPBOARD);
	let named: Vec<_> = CLIPBOARD
		.iter()
		.map(|action| (action.label(), action.config_key()))
		.collect();
	assert_eq!(
		named,
		[
			("Copy", "shortcut_copy"),
			("Paste", "shortcut_paste"),
			("Previous tab", "shortcut_previous_tab"),
			("Next tab", "shortcut_next_tab"),
		]
	);
}

#[test]
fn setting_one_is_read_back_and_conflicts_with_the_others() {
	let mut shortcuts = Shortcuts::from_config("");
	let chord = Shortcut::parse("ctrl+alt+c").unwrap();
	shortcuts.set(ShortcutAction::Copy, chord.clone());
	assert_eq!(shortcuts.get(ShortcutAction::Copy), &chord);
	assert_eq!(shortcuts.conflict(ShortcutAction::Copy, &chord), None);
	assert_eq!(
		shortcuts.conflict(ShortcutAction::Search, &chord),
		Some(ShortcutAction::Copy)
	);
	// Taking the new tab chord for Next tab is reported against New tab.
	let new_tab = shortcuts.new_tab.clone();
	assert_eq!(
		shortcuts.conflict(ShortcutAction::NextTab, &new_tab),
		Some(ShortcutAction::NewTab)
	);
}

#[test]
fn matching_takes_the_first_action_listed_when_two_share_a_chord() {
	let mut shortcuts = Shortcuts::from_config("");
	let shared = Shortcut::parse("alt+p").unwrap();
	shortcuts.set(ShortcutAction::Paste, shared.clone());
	shortcuts.set(ShortcutAction::NextTab, shared);
	let p = chr("p");
	assert_eq!(action(&shortcuts, &p, "alt"), Some(ShortcutAction::Paste));
	let reversed = [ShortcutAction::NextTab, ShortcutAction::Paste];
	assert_eq!(
		shortcuts.matching(&reversed, &p, mods("alt")),
		Some(ShortcutAction::NextTab)
	);
	assert_eq!(shortcuts.matching(&[], &p, mods("alt")), None);
}
