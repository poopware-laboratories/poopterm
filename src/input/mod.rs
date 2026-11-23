//! Keyboard, mouse and touch input for the PTY, plus configurable shortcuts.
pub(crate) mod click;
#[cfg(test)]
mod clipboard_shortcuts_tests;
pub(crate) mod keyboard_modes;
#[cfg(test)]
mod keyboard_modes_tests;
pub(crate) mod keypad;
pub(crate) mod keys;
pub(crate) mod modify;
#[cfg(test)]
mod modify_tests;
pub(crate) mod mouse;
pub(crate) mod pane_shortcuts;
#[cfg(test)]
mod pane_shortcuts_tests;
pub(crate) mod paste;
#[cfg(test)]
mod paste_tests;
pub(crate) mod scroll;
#[cfg(test)]
mod shortcut_actions_tests;
pub(crate) mod shortcuts;
pub(crate) mod touch;
#[cfg_attr(
	not(any(target_os = "ios", test)),
	expect(dead_code, reason = "only iOS's keyboard asks")
)]
pub(crate) mod typing;
