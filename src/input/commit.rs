//! Text an input method commits (the on-screen keyboard, dictation) as the
//! bytes a terminal sends: its Return arrives as `\n`, but a terminal's
//! Enter is a carriage return, which a program in raw mode (nano, vim)
//! tells apart from a line feed, and which the line discipline turns back
//! into `\n` for programs reading lines.

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
