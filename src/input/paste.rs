//! Preparing clipboard text for the PTY.

const PASTE_START: &str = "\x1b[200~";
const PASTE_END: &str = "\x1b[201~";

/// Converts clipboard text into PTY input. Line endings become CR, as a
/// typed Enter would be. With bracketed paste (`?2004`) the text is wrapped
/// in markers, and any markers inside it are removed so pasted content
/// cannot end the paste early and inject keystrokes.
pub(crate) fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
	let mut text = text.replace("\r\n", "\r").replace('\n', "\r");
	if bracketed {
		text = format!("{PASTE_START}{}{PASTE_END}", without_markers(&text));
	}
	text.into_bytes()
}

/// `text` without paste markers, also those that removing others forms
/// (`ESC [ 20` `ESC [ 201 ~` `1 ~`), in one pass: each marker is cut as
/// soon as it ends what was kept, so the work grows with the text.
fn without_markers(text: &str) -> String {
	let mut kept = String::with_capacity(text.len());
	for ch in text.chars() {
		kept.push(ch);
		if kept.ends_with(PASTE_START) || kept.ends_with(PASTE_END) {
			kept.truncate(kept.len() - PASTE_END.len());
		}
	}
	kept
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn bracketed_paste_cannot_be_terminated_by_the_pasted_text() {
		let bytes = paste_bytes("ls\x1b[201~rm -rf ~\n", true);
		assert_eq!(bytes, b"\x1b[200~lsrm -rf ~\r\x1b[201~");
	}

	#[test]
	fn newlines_are_sent_as_carriage_returns() {
		assert_eq!(paste_bytes("a\r\nb\nc", false), b"a\rb\rc");
	}
}
