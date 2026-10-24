//! Key modifier options as the key encoders apply them: xterm's tables for
//! modifyCursorKeys, modifyFunctionKeys, modifyKeypadKeys and
//! modifyOtherKeys, with formatOtherKeys and the modifier mask.
use super::{
	keypad::encode_keypad,
	keys::{KeyModes, Modifiers, encode_key},
	modify::{CURSOR_KEYS, FUNCTION_KEYS, KEYPAD_KEYS, KeyOptions},
};
use winit::keyboard::{Key, NamedKey};

const OTHER_KEYS: usize = 4;
const S: u8 = 1;
const A: u8 = 2;
const C: u8 = 4;
const M: u8 = 8;

fn mods(bits: u8) -> Modifiers {
	Modifiers {
		shift: bits & S != 0,
		alt: bits & A != 0,
		ctrl: bits & C != 0,
		super_key: bits & M != 0,
	}
}

fn modes(resource: usize, level: i8) -> KeyModes {
	let mut modes = KeyModes::default();
	modes.options.modify[resource] = level;
	modes
}

fn key(key: Key, bits: u8, modes: KeyModes) -> String {
	String::from_utf8(encode_key(&key, mods(bits), modes).unwrap()).unwrap()
}

fn named(named: NamedKey) -> Key {
	Key::Named(named)
}

fn text(s: &str) -> Key {
	Key::Character(s.into())
}

#[test]
fn cursor_keys_follow_modify_cursor_keys() {
	use NamedKey::*;
	let rows: [(NamedKey, i8, bool, u8, &str); 20] = [
		(ArrowUp, -1, false, C, "\x1b[A"),
		(ArrowUp, -1, true, C, "\x1bOA"),
		(ArrowUp, 0, false, C, "\x1bO5A"),
		(ArrowUp, 0, true, C, "\x1bO5A"),
		(ArrowUp, 1, false, C, "\x1b[5A"),
		(ArrowUp, 2, false, C, "\x1b[1;5A"),
		(ArrowUp, 2, true, C, "\x1b[1;5A"),
		(ArrowUp, 3, false, C, "\x1b[>1;5A"),
		(ArrowDown, 1, false, S, "\x1b[2B"),
		(ArrowDown, 3, true, S, "\x1b[>1;2B"),
		(ArrowRight, 0, false, A, "\x1bO3C"),
		(ArrowRight, -1, false, A, "\x1b[C"),
		(ArrowLeft, 2, false, C | S, "\x1b[1;6D"),
		(ArrowLeft, 1, true, M, "\x1b[9D"),
		(Home, 2, false, S, "\x1b[1;2H"),
		(Home, 0, false, S, "\x1bO2H"),
		(Home, -1, true, S, "\x1bOH"),
		(End, 1, false, C | A, "\x1b[7F"),
		(End, 3, false, C | A | S | M, "\x1b[>1;16F"),
		(End, -1, false, C, "\x1b[F"),
	];
	for (name, level, application_cursor, bits, expected) in rows {
		let modes = KeyModes {
			application_cursor,
			..modes(CURSOR_KEYS, level)
		};
		let got = key(named(name), bits, modes);
		assert_eq!(
			got, expected,
			"{name:?} at {level}, application {application_cursor}"
		);
	}
}

#[test]
fn unmodified_cursor_keys_ignore_the_level() {
	for level in -1..=3 {
		assert_eq!(
			key(named(NamedKey::ArrowUp), 0, modes(CURSOR_KEYS, level)),
			"\x1b[A"
		);
		let application = KeyModes {
			application_cursor: true,
			..modes(CURSOR_KEYS, level)
		};
		assert_eq!(key(named(NamedKey::Home), 0, application), "\x1bOH");
	}
}

#[test]
fn ss3_function_keys_follow_modify_function_keys() {
	use NamedKey::*;
	let rows: [(NamedKey, i8, u8, &str); 10] = [
		(F1, -1, S, "\x1bOP"),
		(F1, 0, S, "\x1bO2P"),
		(F1, 1, S, "\x1b[2P"),
		(F1, 2, S, "\x1b[1;2P"),
		(F1, 3, S, "\x1b[>1;2P"),
		(F2, 2, C, "\x1b[1;5Q"),
		(F3, 1, A, "\x1b[3R"),
		(F4, 0, C | A, "\x1bO7S"),
		(F4, -1, C | A, "\x1bOS"),
		(F2, 3, M, "\x1b[>1;9Q"),
	];
	for (name, level, bits, expected) in rows {
		assert_eq!(
			key(named(name), bits, modes(FUNCTION_KEYS, level)),
			expected,
			"{name:?} {level}"
		);
	}
}

#[test]
fn numbered_keys_follow_modify_function_keys() {
	use NamedKey::*;
	let rows: [(NamedKey, i8, u8, &str); 14] = [
		(F5, -1, C, "\x1b[15~"),
		(F5, 0, C, "\x1b[15;5~"),
		(F5, 1, C, "\x1b[15;5~"),
		(F5, 2, C, "\x1b[15;5~"),
		(F5, 3, C, "\x1b[>15;5~"),
		(F12, 2, S, "\x1b[24;2~"),
		(F12, 3, S, "\x1b[>24;2~"),
		(F12, -1, S, "\x1b[24~"),
		(Insert, 2, S, "\x1b[2;2~"),
		(Delete, 3, C, "\x1b[>3;5~"),
		(PageUp, 1, A, "\x1b[5;3~"),
		(PageDown, -1, A, "\x1b[6~"),
		(PageDown, 0, C | S, "\x1b[6;6~"),
		(Delete, 2, 0, "\x1b[3~"),
	];
	for (name, level, bits, expected) in rows {
		assert_eq!(
			key(named(name), bits, modes(FUNCTION_KEYS, level)),
			expected,
			"{name:?} {level}"
		);
	}
}

#[test]
fn cursor_and_function_levels_are_independent() {
	let mut modes = KeyModes::default();
	modes.options.modify[CURSOR_KEYS] = -1;
	modes.options.modify[FUNCTION_KEYS] = 3;
	assert_eq!(key(named(NamedKey::ArrowUp), C, modes), "\x1b[A");
	assert_eq!(key(named(NamedKey::F1), C, modes), "\x1b[>1;5P");
	assert_eq!(key(named(NamedKey::F5), C, modes), "\x1b[>15;5~");
}

#[test]
fn the_kitty_protocol_ignores_the_modify_levels() {
	let options = KeyOptions {
		modify: [-1, -1, 3, -1, 2, 0, 0, 0],
		format: [0, 0, 0, 0, 1, 0, 0, 0],
		other_mask: 0,
	};
	let kitty = KeyModes {
		kitty: true,
		..KeyModes::default()
	};
	let with_options = KeyModes { options, ..kitty };
	assert_eq!(key(named(NamedKey::ArrowUp), C, with_options), "\x1b[1;5A");
	assert_eq!(key(named(NamedKey::F1), S, with_options), "\x1b[1;2P");
	assert_eq!(key(named(NamedKey::F5), A, with_options), "\x1b[15;3~");
	for (k, bits) in [
		(text("a"), C),
		(named(NamedKey::Tab), A),
		(named(NamedKey::Escape), 0),
	] {
		let plain = encode_key(&k, mods(bits), kitty);
		assert_eq!(encode_key(&k, mods(bits), with_options), plain, "{k:?}");
	}
}

#[test]
fn modify_other_keys_replaces_the_legacy_encoding() {
	let level2 = modes(OTHER_KEYS, 2);
	assert_eq!(key(text("a"), C, KeyModes::default()), "\x01");
	assert_eq!(key(text("a"), C, level2), "\x1b[27;5;97~");
	assert_eq!(key(text("x"), A, KeyModes::default()), "\x1bx");
	assert_eq!(key(text("x"), A, level2), "\x1b[27;3;120~");
	assert_eq!(key(named(NamedKey::Backspace), A, level2), "\x1b[27;3;127~");
	assert_eq!(key(named(NamedKey::Escape), C, level2), "\x1b[27;5;27~");
}

#[test]
fn plain_keys_keep_their_legacy_bytes_with_modify_other_keys() {
	let level2 = modes(OTHER_KEYS, 2);
	assert_eq!(key(text("a"), 0, level2), "a");
	assert_eq!(key(text("A"), S, level2), "A");
	assert_eq!(key(named(NamedKey::Enter), 0, level2), "\r");
	assert_eq!(key(named(NamedKey::Tab), 0, level2), "\t");
	assert_eq!(key(named(NamedKey::Escape), 0, level2), "\x1b");
	assert_eq!(key(named(NamedKey::Backspace), 0, level2), "\x7f");
}

#[test]
fn newline_mode_only_changes_the_plain_enter() {
	let modes = KeyModes {
		newline: true,
		..modes(OTHER_KEYS, 2)
	};
	assert_eq!(key(named(NamedKey::Enter), 0, modes), "\r\n");
	assert_eq!(key(named(NamedKey::Enter), C, modes), "\x1b[27;5;13~");
	assert_eq!(key(named(NamedKey::Enter), S, modes), "\x1b[27;2;13~");
}

#[test]
fn level_two_reports_every_modifier_combination() {
	let level2 = modes(OTHER_KEYS, 2);
	let rows: [(u8, Option<&str>); 15] = [
		(S, None),
		(A, Some("\x1b[27;3;97~")),
		(S | A, Some("\x1b[27;4;97~")),
		(C, Some("\x1b[27;5;97~")),
		(S | C, Some("\x1b[27;6;97~")),
		(A | C, Some("\x1b[27;7;97~")),
		(S | A | C, Some("\x1b[27;8;97~")),
		(M, Some("\x1b[27;9;97~")),
		(S | M, Some("\x1b[27;10;97~")),
		(A | M, Some("\x1b[27;11;97~")),
		(S | A | M, Some("\x1b[27;12;97~")),
		(C | M, Some("\x1b[27;13;97~")),
		(S | C | M, Some("\x1b[27;14;97~")),
		(A | C | M, Some("\x1b[27;15;97~")),
		(S | A | C | M, Some("\x1b[27;16;97~")),
	];
	for (bits, expected) in rows {
		let got = encode_key(&text("a"), mods(bits), level2).map(|b| String::from_utf8(b).unwrap());
		if let Some(expected) = expected {
			assert_eq!(got.as_deref(), Some(expected), "modifiers {bits}");
		} else {
			assert_ne!(
				got.as_deref().unwrap_or(""),
				"\x1b[27;2;97~",
				"Shift alone is the character"
			);
		}
	}
}

#[test]
fn level_one_leaves_ctrl_with_a_control_character_alone() {
	let level1 = modes(OTHER_KEYS, 1);
	let level2 = modes(OTHER_KEYS, 2);
	for c in [
		"a", "z", "@", "[", "\\", "]", "^", "_", "?", "2", "5", "8", "/",
	] {
		let code = u32::from(c.chars().next().unwrap());
		assert_ne!(
			key(text(c), C, level1),
			format!("\x1b[27;5;{code}~"),
			"Ctrl+{c} at 1"
		);
		assert_eq!(
			key(text(c), C, level2),
			format!("\x1b[27;5;{code}~"),
			"Ctrl+{c} at 2"
		);
	}
	assert_ne!(key(named(NamedKey::Space), C, level1), "\x1b[27;5;32~");
}

#[test]
fn level_one_encodes_ctrl_with_other_characters() {
	let level1 = modes(OTHER_KEYS, 1);
	let rows = [
		("1", "\x1b[27;5;49~"),
		("9", "\x1b[27;5;57~"),
		("0", "\x1b[27;5;48~"),
		(";", "\x1b[27;5;59~"),
		(",", "\x1b[27;5;44~"),
		(".", "\x1b[27;5;46~"),
		("'", "\x1b[27;5;39~"),
		("=", "\x1b[27;5;61~"),
		("-", "\x1b[27;5;45~"),
		("`", "\x1b[27;5;96~"),
	];
	for (c, expected) in rows {
		assert_eq!(key(text(c), C, level1), expected, "Ctrl+{c}");
	}
}

#[test]
fn level_one_encodes_alt_super_and_modified_named_keys() {
	let level1 = modes(OTHER_KEYS, 1);
	assert_eq!(key(text("a"), C | A, level1), "\x1b[27;7;97~");
	assert_eq!(key(text("a"), M, level1), "\x1b[27;9;97~");
	assert_eq!(key(text("a"), A, level1), "\x1b[27;3;97~");
	assert_eq!(key(named(NamedKey::Tab), C, level1), "\x1b[27;5;9~");
	assert_eq!(key(named(NamedKey::Tab), C | S, level1), "\x1b[27;6;9~");
	assert_eq!(key(named(NamedKey::Enter), S, level1), "\x1b[27;2;13~");
	assert_eq!(key(named(NamedKey::Backspace), C, level1), "\x1b[27;5;127~");
}

#[test]
fn level_one_keeps_shift_and_ctrl_on_letters() {
	let level1 = modes(OTHER_KEYS, 1);
	assert_ne!(key(text("A"), C | S, level1), "\x1b[27;6;65~");
	assert_eq!(key(text("A"), S, level1), "A");
	assert_eq!(key(named(NamedKey::Tab), S, level1), "\x1b[Z", "back tab");
}

#[test]
fn level_three_encodes_shifted_and_plain_keys() {
	let level3 = modes(OTHER_KEYS, 3);
	let rows: [(Key, u8, &str); 6] = [
		(text("a"), 0, "\x1b[27;1;97~"),
		(text("A"), S, "\x1b[27;2;65~"),
		(text("!"), S, "\x1b[27;2;33~"),
		(named(NamedKey::Enter), 0, "\x1b[27;1;13~"),
		(named(NamedKey::Tab), 0, "\x1b[27;1;9~"),
		(named(NamedKey::Escape), 0, "\x1b[27;1;27~"),
	];
	for (k, bits, expected) in rows {
		assert_eq!(key(k.clone(), bits, level3), expected, "{k:?}");
	}
}

#[test]
fn format_one_applies_to_every_level() {
	let rows: [(i8, Key, u8, &str); 7] = [
		(1, text("1"), C, "\x1b[49;5u"),
		(2, text("a"), C, "\x1b[97;5u"),
		(2, named(NamedKey::Enter), S, "\x1b[13;2u"),
		(2, named(NamedKey::Escape), C, "\x1b[27;5u"),
		(2, named(NamedKey::Backspace), A, "\x1b[127;3u"),
		(3, text("a"), 0, "\x1b[97;1u"),
		(3, named(NamedKey::Space), S | C | A | M, "\x1b[32;16u"),
	];
	for (level, k, bits, expected) in rows {
		let mut modes = modes(OTHER_KEYS, level);
		modes.options.format[OTHER_KEYS] = 1;
		assert_eq!(key(k.clone(), bits, modes), expected, "{k:?} at {level}");
	}
}

#[test]
fn other_formats_fall_back_to_the_tilde_form() {
	let mut modes = modes(OTHER_KEYS, 2);
	modes.options.format[OTHER_KEYS] = 2;
	assert_eq!(key(text("a"), C, modes), "\x1b[27;5;97~");
	modes.options.format = [1, 1, 1, 1, 0, 1, 1, 1];
	assert_eq!(
		key(text("a"), C, modes),
		"\x1b[27;5;97~",
		"only resource 4 counts"
	);
}

#[test]
fn the_mask_drops_each_modifier_it_names() {
	let rows: [(u8, u8, Option<&str>); 6] = [
		(A, A, None),
		(A, C | A, Some("\x1b[27;5;120~")),
		(C, C | A, Some("\x1b[27;3;120~")),
		(M, M | C, Some("\x1b[27;5;120~")),
		(C | A, C | A | S, None),
		(S | A | C | M, S | A | C | M, None),
	];
	for (mask, bits, expected) in rows {
		let mut modes = modes(OTHER_KEYS, 2);
		modes.options.other_mask = mask;
		let got = encode_key(&text("x"), mods(bits), modes).map(|b| String::from_utf8(b).unwrap());
		match expected {
			Some(expected) => assert_eq!(got.as_deref(), Some(expected), "mask {mask}, mods {bits}"),
			None => assert!(
				!got.unwrap().starts_with("\x1b[27;"),
				"mask {mask}, mods {bits}"
			),
		}
	}
}

#[test]
fn level_three_reports_a_fully_masked_key_as_unmodified() {
	let mut modes = modes(OTHER_KEYS, 3);
	modes.options.other_mask = C;
	assert_eq!(key(text("a"), C, modes), "\x1b[27;1;97~");
}

#[test]
fn characters_beyond_ascii_report_their_code_point() {
	let level2 = modes(OTHER_KEYS, 2);
	assert_eq!(key(text("é"), C, level2), "\x1b[27;5;233~");
	assert_eq!(key(text("€"), A, level2), "\x1b[27;3;8364~");
	assert_eq!(key(text("😀"), C, level2), "\x1b[27;5;128512~");
}

#[test]
fn keypad_keys_follow_modify_keypad_keys() {
	let rows: [(i8, Key, u8, &str); 8] = [
		(-1, named(NamedKey::Enter), C, "\x1bOM"),
		(0, named(NamedKey::Enter), C, "\x1bO5M"),
		(1, named(NamedKey::Enter), C, "\x1b[5M"),
		(2, named(NamedKey::Enter), C, "\x1b[1;5M"),
		(3, named(NamedKey::Enter), C, "\x1b[>1;5M"),
		(2, text("+"), S, "\x1b[1;2k"),
		(-1, text("*"), A, "\x1bOj"),
		(3, text("/"), 0, "\x1bOo"),
	];
	for (level, k, bits, expected) in rows {
		let mut modes = KeyModes {
			application_keypad: true,
			..modes(KEYPAD_KEYS, level)
		};
		// Operators send SS3 only while ?1035 ignores NumLock.
		modes.keyboard.numlock = false;
		let got = String::from_utf8(encode_keypad(&k, mods(bits), modes).unwrap()).unwrap();
		assert_eq!(got, expected, "{k:?} at {level}");
	}
}

#[test]
fn keypad_begin_follows_modify_cursor_keys() {
	let begin = named(NamedKey::Clear);
	let rows: [(i8, &str); 5] = [
		(-1, "\x1b[E"),
		(0, "\x1bO2E"),
		(1, "\x1b[2E"),
		(2, "\x1b[1;2E"),
		(3, "\x1b[>1;2E"),
	];
	for (level, expected) in rows {
		let got = encode_keypad(&begin, mods(S), modes(CURSOR_KEYS, level)).unwrap();
		assert_eq!(String::from_utf8(got).unwrap(), expected, "level {level}");
	}
	let keypad_level = modes(KEYPAD_KEYS, -1);
	let got = encode_keypad(&begin, mods(S), keypad_level).unwrap();
	assert_eq!(
		got, b"\x1b[1;2E",
		"modifyKeypadKeys does not apply to Begin"
	);
}
