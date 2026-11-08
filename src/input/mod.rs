//! Keyboard and mouse encoding for the PTY, plus configurable shortcuts.
pub(crate) mod click;
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
pub(crate) mod scroll;
pub(crate) mod shortcuts;
