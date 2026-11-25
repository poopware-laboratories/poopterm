//! Text from the on-screen keyboard and input methods as the keys and bytes
//! a terminal gets: its Return arrives as the character `\n`, but a
//! terminal's Enter is a carriage return, which a program in raw mode
//! (nano, vim) tells apart from a line feed, and which the line discipline
//! turns back into `\n` for programs reading lines.
use winit::keyboard::{Key, NamedKey};

/// `key` as a keyboard would report it: on iOS, winit reports every
/// on-screen keystroke as a character, so Return comes as `"\n"`; it is
/// the Enter key.
pub(crate) fn soft_key(key: Key) -> Key {
	match &key {
		Key::Character(text) if matches!(text.as_str(), "\n" | "\r" | "\r\n") => {
			Key::Named(NamedKey::Enter)
		}
		_ => key,
	}
}

/// The bytes for committed `text`: each line break, `\n` or `\r\n`, as `\r`.
pub(crate) fn commit_bytes(text: &str) -> Vec<u8> {
	let mut bytes = Vec::with_capacity(text.len());
	let mut chars = text.bytes().peekable();
	while let Some(byte) = chars.next() {
		match byte {
			b'\r' if chars.peek() == Some(&b'\n') => {}
			b'\n' => bytes.push(b'\r'),
			other => bytes.push(other),
		}
	}
	bytes
}

#[cfg(test)]
#[path = "commit_tests.rs"]
mod tests;
