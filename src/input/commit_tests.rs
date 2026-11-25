use super::*;

#[test]
fn a_return_from_the_keyboard_is_a_carriage_return() {
	assert_eq!(commit_bytes("\n"), b"\r");
}

#[test]
fn plain_text_is_unchanged_including_non_ascii() {
	assert_eq!(commit_bytes("hello"), b"hello");
	assert_eq!(commit_bytes("é漢😀"), "é漢😀".as_bytes());
	assert_eq!(commit_bytes(""), b"");
}

#[test]
fn text_with_line_breaks_keeps_each_one_as_a_single_carriage_return() {
	assert_eq!(commit_bytes("a\nb\n"), b"a\rb\r");
	assert_eq!(commit_bytes("a\r\nb"), b"a\rb", "a CR LF pair is one break");
	assert_eq!(commit_bytes("\n\n"), b"\r\r", "empty lines stay");
	assert_eq!(commit_bytes("a\rb"), b"a\rb", "a lone CR is already right");
}

#[test]
fn a_carriage_return_before_a_line_feed_only_collapses_with_that_one() {
	assert_eq!(commit_bytes("\r\r\n"), b"\r\r");
	assert_eq!(commit_bytes("\r\n\n"), b"\r\r");
}

fn character(text: &str) -> Key {
	Key::Character(text.into())
}

#[test]
fn the_return_character_the_on_screen_keyboard_reports_is_the_enter_key() {
	for text in ["\n", "\r", "\r\n"] {
		assert_eq!(
			soft_key(character(text)),
			Key::Named(NamedKey::Enter),
			"{text:?}"
		);
	}
}

#[test]
fn other_keys_are_left_alone() {
	for key in [
		character("a"),
		character(" "),
		character("\t"),
		character("\n\n"),
		character("é"),
		Key::Named(NamedKey::Enter),
		Key::Named(NamedKey::Backspace),
	] {
		assert_eq!(soft_key(key.clone()), key);
	}
}

#[test]
fn the_enter_key_encodes_as_a_carriage_return_like_a_hardware_return() {
	use crate::input::keys::encode_key;
	let none = crate::input::keys::Modifiers::default();
	let modes = crate::input::keys::KeyModes::default();
	let typed = encode_key(&soft_key(character("\n")), none, modes);
	assert_eq!(typed, Some(b"\r".to_vec()));
	let plain = encode_key(&character("\n"), none, modes);
	assert_ne!(plain, typed, "left as a character it was not that");
}
