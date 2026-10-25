//! Font zoom: shortcuts that grow, shrink and reset the terminal font for
//! this session, without changing the size saved in Settings.

use winit::keyboard::Key;

use crate::input::{keys::Modifiers, shortcuts::Shortcuts};

/// The smallest and largest font sizes zoom reaches, in points. Settings
/// offers 10 to 32; zoom goes further for people who need very large text.
pub(crate) const MIN_POINTS: usize = 6;
pub(crate) const MAX_POINTS: usize = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zoom {
	In,
	Out,
	Reset,
}

/// The zoom `key` pressed with `mods` asks for, if any.
pub(crate) fn zoom_action(shortcuts: &Shortcuts, key: &Key, mods: Modifiers) -> Option<Zoom> {
	[
		(&shortcuts.zoom_in, Zoom::In),
		(&shortcuts.zoom_out, Zoom::Out),
		(&shortcuts.zoom_reset, Zoom::Reset),
	]
	.into_iter()
	.find_map(|(shortcut, zoom)| shortcut.matches(key, mods).then_some(zoom))
}

/// The font size after `zoom` from `current`; `configured` is Settings' size.
pub(crate) fn zoomed(current: usize, configured: usize, zoom: Zoom) -> usize {
	match zoom {
		Zoom::In => (current + 1).min(MAX_POINTS),
		Zoom::Out => current.saturating_sub(1).max(MIN_POINTS),
		Zoom::Reset => configured,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::input::shortcuts::{Shortcut, ShortcutAction};

	fn chr(value: &str) -> Key {
		Key::Character(value.into())
	}

	fn primary() -> Modifiers {
		Modifiers {
			ctrl: !cfg!(target_os = "macos"),
			super_key: cfg!(target_os = "macos"),
			..Default::default()
		}
	}

	#[test]
	fn each_step_changes_the_size_by_one_point() {
		assert_eq!(zoomed(14, 14, Zoom::In), 15);
		assert_eq!(zoomed(15, 14, Zoom::In), 16);
		assert_eq!(zoomed(14, 14, Zoom::Out), 13);
	}

	#[test]
	fn reset_returns_to_the_size_settings_chose() {
		assert_eq!(zoomed(20, 14, Zoom::Reset), 14);
		assert_eq!(zoomed(8, 14, Zoom::Reset), 14);
		assert_eq!(zoomed(14, 14, Zoom::Reset), 14);
	}

	#[test]
	fn zoom_reaches_beyond_the_settings_range_but_stays_bounded() {
		// Settings offers 10 to 32 points; zoom goes further for people who
		// need very large text, and down to a size that still draws.
		assert_eq!(zoomed(32, 14, Zoom::In), 33);
		assert_eq!(zoomed(MAX_POINTS - 1, 14, Zoom::In), MAX_POINTS);
		assert_eq!(zoomed(MAX_POINTS, 14, Zoom::In), MAX_POINTS);
		assert_eq!(zoomed(10, 14, Zoom::Out), 9);
		assert_eq!(zoomed(MIN_POINTS + 1, 14, Zoom::Out), MIN_POINTS);
		assert_eq!(zoomed(MIN_POINTS, 14, Zoom::Out), MIN_POINTS);
	}

	#[test]
	fn repeated_steps_walk_up_and_back_down() {
		let mut size = 14;
		for _ in 0..100 {
			size = zoomed(size, 14, Zoom::In);
		}
		assert_eq!(size, MAX_POINTS);
		for _ in 0..100 {
			size = zoomed(size, 14, Zoom::Out);
		}
		assert_eq!(size, MIN_POINTS);
	}

	#[test]
	fn the_default_shortcuts_are_primary_equals_minus_and_zero() {
		let shortcuts = Shortcuts::from_config("");
		assert_eq!(
			zoom_action(&shortcuts, &chr("="), primary()),
			Some(Zoom::In)
		);
		assert_eq!(
			zoom_action(&shortcuts, &chr("-"), primary()),
			Some(Zoom::Out)
		);
		assert_eq!(
			zoom_action(&shortcuts, &chr("0"), primary()),
			Some(Zoom::Reset)
		);
	}

	#[test]
	fn other_keys_and_modifiers_do_not_zoom() {
		let shortcuts = Shortcuts::from_config("");
		assert_eq!(zoom_action(&shortcuts, &chr("1"), primary()), None);
		assert_eq!(
			zoom_action(&shortcuts, &chr("="), Modifiers::default()),
			None
		);
		let alt = Modifiers {
			alt: true,
			..primary()
		};
		assert_eq!(zoom_action(&shortcuts, &chr("-"), alt), None);
	}

	#[test]
	fn zoom_shortcuts_can_be_rebound_in_the_config() {
		let config = "shortcut_zoom_in = \"ctrl+shift+plus\"\nshortcut_zoom_reset = \"alt+0\"\n";
		let shortcuts = Shortcuts::from_config(config);
		let ctrl_shift = Modifiers {
			ctrl: true,
			shift: true,
			..Default::default()
		};
		let alt = Modifiers {
			alt: true,
			..Default::default()
		};
		assert_eq!(
			zoom_action(&shortcuts, &chr("+"), ctrl_shift),
			Some(Zoom::In)
		);
		assert_eq!(
			zoom_action(&shortcuts, &chr("="), primary()),
			None,
			"the old one is gone"
		);
		assert_eq!(zoom_action(&shortcuts, &chr("0"), alt), Some(Zoom::Reset));
		assert_eq!(
			zoom_action(&shortcuts, &chr("-"), primary()),
			Some(Zoom::Out)
		);
	}

	#[test]
	fn zoom_actions_are_listed_for_settings_with_their_own_keys() {
		for (action, label, key) in [
			(ShortcutAction::ZoomIn, "Zoom in", "shortcut_zoom_in"),
			(ShortcutAction::ZoomOut, "Zoom out", "shortcut_zoom_out"),
			(
				ShortcutAction::ZoomReset,
				"Reset zoom",
				"shortcut_zoom_reset",
			),
		] {
			assert!(ShortcutAction::ALL.contains(&action));
			assert_eq!((action.label(), action.config_key()), (label, key));
		}
	}

	#[test]
	fn no_two_actions_share_a_default_binding_label_or_key() {
		let shortcuts = Shortcuts::from_config("");
		let all = ShortcutAction::ALL;
		for (i, a) in all.iter().enumerate() {
			for b in &all[i + 1..] {
				assert_ne!(shortcuts.get(*a), shortcuts.get(*b), "{a:?} and {b:?}");
				assert_ne!(a.label(), b.label());
				assert_ne!(a.config_key(), b.config_key());
			}
		}
	}

	#[test]
	fn zoom_bindings_can_be_set_and_conflict_with_others() {
		let mut shortcuts = Shortcuts::from_config("");
		let binding = Shortcut::parse("ctrl+alt+z").unwrap();
		shortcuts.set(ShortcutAction::ZoomOut, binding.clone());
		assert_eq!(shortcuts.get(ShortcutAction::ZoomOut), &binding);
		assert_eq!(
			shortcuts.conflict(ShortcutAction::ZoomIn, &binding),
			Some(ShortcutAction::ZoomOut)
		);
		let search = shortcuts.get(ShortcutAction::Search).clone();
		assert_eq!(
			shortcuts.conflict(ShortcutAction::ZoomReset, &search),
			Some(ShortcutAction::Search)
		);
	}

	#[test]
	fn equals_and_minus_chords_are_captured_and_shown() {
		let ctrl = Modifiers {
			ctrl: true,
			..Default::default()
		};
		let minus = Shortcut::from_event(&chr("-"), ctrl).unwrap();
		assert_eq!(minus.binding(), "ctrl+-");
		assert_eq!(minus.display(false), "Ctrl+-");
		let equals = Shortcut::parse("super+=").unwrap();
		assert_eq!(equals.display(true), "⌘=");
	}

	#[test]
	fn zoom_bindings_survive_saving_and_reloading_the_config() {
		// Settings saves every binding through config::updated; "=", "-" and
		// "+" must come back as the same chords.
		let mut shortcuts = Shortcuts::from_config("");
		for (action, binding) in [
			(ShortcutAction::ZoomIn, "ctrl+shift+="),
			(ShortcutAction::ZoomOut, "alt+-"),
			(ShortcutAction::ZoomReset, "super+0"),
			(ShortcutAction::Search, "ctrl+plus"),
		] {
			shortcuts.set(action, Shortcut::parse(binding).unwrap());
		}
		let values: Vec<_> = ShortcutAction::ALL
			.into_iter()
			.map(|action| {
				let binding = shortcuts.get(action).binding();
				(
					action.config_key(),
					crate::config::quote_config_string(&binding),
				)
			})
			.collect();
		let reloaded = Shortcuts::from_config(&crate::config::updated("font_size = 14\n", &values));
		for action in ShortcutAction::ALL {
			assert_eq!(reloaded.get(action), shortcuts.get(action), "{action:?}");
		}
	}

	#[test]
	fn an_unusable_zoom_binding_falls_back_to_its_default() {
		let shortcuts =
			Shortcuts::from_config("shortcut_zoom_out = \"\"\nshortcut_zoom_in = \"ctrl+\"\n");
		assert_eq!(
			zoom_action(&shortcuts, &chr("-"), primary()),
			Some(Zoom::Out)
		);
		assert_eq!(
			zoom_action(&shortcuts, &chr("="), primary()),
			Some(Zoom::In)
		);
	}

	#[test]
	fn a_session_of_key_presses_zooms_and_resets() {
		let shortcuts = Shortcuts::from_config("");
		let mut size = 14;
		for key in ["=", "=", "=", "-", "x"] {
			if let Some(zoom) = zoom_action(&shortcuts, &chr(key), primary()) {
				size = zoomed(size, 14, zoom);
			}
		}
		assert_eq!(size, 16);
		let zoom = zoom_action(&shortcuts, &chr("0"), primary()).unwrap();
		assert_eq!(zoomed(size, 14, zoom), 14);
	}
}
