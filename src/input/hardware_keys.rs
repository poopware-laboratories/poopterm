//! Keys of a hardware keyboard on iOS, as UIKit reports a press: its
//! modifier flags, its characters without modifiers and its HID usage.
//! winit only passes typed text on, so presses it would lose (a modifier
//! held, or a key that types no text) become keys with their modifiers.
use super::keys::Modifiers;
use winit::keyboard::{Key, NamedKey};

/// `UIKeyModifierFlags`.
pub(crate) const SHIFT: isize = 1 << 17;
pub(crate) const CONTROL: isize = 1 << 18;
pub(crate) const ALTERNATE: isize = 1 << 19;
pub(crate) const COMMAND: isize = 1 << 20;

/// The named key a HID usage (`UIKeyboardHIDUsage`) is, if it types no text.
fn named(usage: isize) -> Option<NamedKey> {
	Some(match usage {
		0x28 => NamedKey::Enter,
		0x29 => NamedKey::Escape,
		0x2A => NamedKey::Backspace,
		0x2B => NamedKey::Tab,
		0x49 => NamedKey::Insert,
		0x4A => NamedKey::Home,
		0x4B => NamedKey::PageUp,
		0x4C => NamedKey::Delete,
		0x4D => NamedKey::End,
		0x4E => NamedKey::PageDown,
		0x4F => NamedKey::ArrowRight,
		0x50 => NamedKey::ArrowLeft,
		0x51 => NamedKey::ArrowDown,
		0x52 => NamedKey::ArrowUp,
		0x3A..=0x45 => FUNCTION[(usage - 0x3A) as usize],
		_ => return None,
	})
}

const FUNCTION: [NamedKey; 12] = [
	NamedKey::F1,
	NamedKey::F2,
	NamedKey::F3,
	NamedKey::F4,
	NamedKey::F5,
	NamedKey::F6,
	NamedKey::F7,
	NamedKey::F8,
	NamedKey::F9,
	NamedKey::F10,
	NamedKey::F11,
	NamedKey::F12,
];

/// The key and modifiers a press is, when the app takes it from UIKit:
/// with Control, Option or Command held, or a key that types no text
/// (Enter and Backspace only with a modifier, as UIKit types them already).
/// `None` leaves the press to UIKit, which types its text.
pub(crate) fn press(flags: isize, characters: &str, usage: isize) -> Option<(Key, Modifiers)> {
	let mods = Modifiers {
		ctrl: flags & CONTROL != 0,
		shift: flags & SHIFT != 0,
		alt: flags & ALTERNATE != 0,
		super_key: flags & COMMAND != 0,
	};
	let modified = mods.ctrl || mods.alt || mods.super_key;
	if let Some(named) = named(usage) {
		let typed = matches!(named, NamedKey::Enter | NamedKey::Backspace);
		return (modified || !typed).then_some((Key::Named(named), mods));
	}
	let mut chars = characters.chars();
	let (Some(ch), None) = (chars.next(), chars.next()) else {
		return None;
	};
	(modified && !ch.is_control()).then(|| (Key::Character(ch.to_string().into()), mods))
}

#[cfg(test)]
#[path = "hardware_keys_tests.rs"]
mod tests;
