//! Hardware keyboard presses on iOS as keys.
use super::*;

const P: isize = 0x13;

fn mods(ctrl: bool, shift: bool, alt: bool, super_key: bool) -> Modifiers {
	Modifiers {
		ctrl,
		shift,
		alt,
		super_key,
	}
}

#[test]
fn ctrl_shift_p_is_the_palette_shortcut() {
	let (key, held) = press(CONTROL | SHIFT, "p", P).unwrap();
	assert_eq!(held, mods(true, true, false, false));
	let palette = crate::input::shortcuts::Shortcut::parse("ctrl+shift+p").unwrap();
	assert!(palette.matches(&key, held));
}

#[test]
fn plain_and_shifted_letters_are_left_to_uikit_which_types_them() {
	assert_eq!(press(0, "p", P), None);
	assert_eq!(press(SHIFT, "p", P), None, "UIKit types P");
}

#[test]
fn ctrl_option_and_command_take_a_letter() {
	assert_eq!(
		press(CONTROL, "c", 0x06),
		Some((Key::Character("c".into()), mods(true, false, false, false)))
	);
	assert_eq!(
		press(ALTERNATE, "b", 0x05).unwrap().1,
		mods(false, false, true, false)
	);
	assert_eq!(
		press(COMMAND, "t", 0x17).unwrap().1,
		mods(false, false, false, true)
	);
}

#[test]
fn keys_that_type_nothing_are_taken_with_or_without_modifiers() {
	for (usage, named) in [
		(0x29, NamedKey::Escape),
		(0x2B, NamedKey::Tab),
		(0x52, NamedKey::ArrowUp),
		(0x51, NamedKey::ArrowDown),
		(0x50, NamedKey::ArrowLeft),
		(0x4F, NamedKey::ArrowRight),
		(0x4A, NamedKey::Home),
		(0x4D, NamedKey::End),
		(0x4B, NamedKey::PageUp),
		(0x4E, NamedKey::PageDown),
		(0x4C, NamedKey::Delete),
		(0x3A, NamedKey::F1),
		(0x45, NamedKey::F12),
	] {
		assert_eq!(
			press(0, "", usage),
			Some((Key::Named(named), Modifiers::default())),
			"{usage:#x}"
		);
	}
	assert_eq!(
		press(SHIFT, "", 0x2B).unwrap().1,
		mods(false, true, false, false),
		"shift+tab"
	);
}

#[test]
fn enter_and_backspace_are_left_to_uikit_unless_modified() {
	assert_eq!(press(0, "\r", 0x28), None);
	assert_eq!(press(0, "", 0x2A), None);
	assert_eq!(
		press(ALTERNATE, "", 0x2A),
		Some((
			Key::Named(NamedKey::Backspace),
			mods(false, false, true, false)
		))
	);
}

#[test]
fn a_press_of_no_one_character_is_left_alone() {
	assert_eq!(press(CONTROL, "", 0x04), None, "a modifier alone");
	assert_eq!(press(CONTROL, "ab", 0x04), None);
	assert_eq!(press(CONTROL, "\u{1b}", 0x04), None, "a control character");
}

#[test]
fn flags_of_other_kinds_are_ignored() {
	// Caps lock (1 << 16) and the numeric pad (1 << 21) are no modifiers here.
	let other = (1 << 16) | (1 << 21);
	assert_eq!(press(other, "p", P), None);
	assert_eq!(
		press(other | CONTROL, "p", P).unwrap().1,
		mods(true, false, false, false)
	);
}
