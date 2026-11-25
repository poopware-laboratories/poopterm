//! The line discipline a kernel would run between the terminal and a
//! program, for the in-process device: echo, editing a line before it is
//! sent (erase, kill, interrupt, end of file), and newlines out as CR LF.
//! Keys that send escape sequences (arrows, function keys, paste markers)
//! are swallowed whole, since a line being edited has no cursor to move.

/// Most characters a line may hold, as a kernel's line buffer is bounded.
pub(crate) const MAX_LINE: usize = 4096;

/// What a piece of input did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
	/// Show these bytes: the echo of what was typed or erased.
	Echo(Vec<u8>),
	/// Enter finished this line.
	Line(String),
	/// `^C`: the line was dropped.
	Interrupt,
	/// `^D` on an empty line.
	Eof,
}

/// Where an escape sequence being swallowed is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Escape {
	#[default]
	None,
	/// After ESC.
	Start,
	/// Inside `ESC [` or `ESC O`, until a final byte.
	Sequence,
}

/// One line being edited.
#[derive(Debug, Default)]
pub(crate) struct Discipline {
	line: String,
	/// UTF-8 bytes of a character not complete yet.
	pending: Vec<u8>,
	escape: Escape,
}

impl Discipline {
	/// Feeds what the user typed, in order.
	pub(crate) fn input(&mut self, bytes: &[u8]) -> Vec<Event> {
		let mut events = Vec::new();
		for &byte in bytes {
			match self.escape {
				Escape::Start => {
					self.escape = if matches!(byte, b'[' | b'O') {
						Escape::Sequence
					} else {
						Escape::None
					};
					continue;
				}
				Escape::Sequence => {
					if (0x40..=0x7e).contains(&byte) {
						self.escape = Escape::None;
					}
					continue;
				}
				Escape::None => {}
			}
			match byte {
				0x1b => self.escape = Escape::Start,
				b'\r' | b'\n' => {
					events.push(Event::Echo(b"\r\n".to_vec()));
					events.push(Event::Line(std::mem::take(&mut self.line)));
				}
				0x7f | 0x08 => {
					if let Some(ch) = self.line.pop() {
						let width = crate::width::width(ch).max(1);
						let erase = "\x08".repeat(width) + &" ".repeat(width) + &"\x08".repeat(width);
						events.push(Event::Echo(erase.into_bytes()));
					}
				}
				0x03 => {
					self.line.clear();
					events.push(Event::Echo(b"^C\r\n".to_vec()));
					events.push(Event::Interrupt);
				}
				0x15 => {
					let width: usize = self
						.line
						.chars()
						.map(|ch| crate::width::width(ch).max(1))
						.sum();
					self.line.clear();
					if width > 0 {
						let erase = "\x08".repeat(width) + &" ".repeat(width) + &"\x08".repeat(width);
						events.push(Event::Echo(erase.into_bytes()));
					}
				}
				0x04 if self.line.is_empty() => events.push(Event::Eof),
				// Other controls (tab, ^D within a line, ^Z…) do nothing here.
				0x00..=0x1f => {}
				_ => self.typed(byte, &mut events),
			}
		}
		events
	}

	/// A byte of text: echoed once its character is whole.
	fn typed(&mut self, byte: u8, events: &mut Vec<Event>) {
		self.pending.push(byte);
		let text = match std::str::from_utf8(&self.pending) {
			Ok(text) => text.to_owned(),
			// Too short yet: wait for the rest of the character.
			Err(error) if error.error_len().is_none() && self.pending.len() < 4 => return,
			Err(_) => {
				self.pending.clear();
				return;
			}
		};
		self.pending.clear();
		if self.line.chars().count() < MAX_LINE {
			self.line.push_str(&text);
			events.push(Event::Echo(text.into_bytes()));
		}
	}
}

/// Program output as a terminal shows it: each `\n` as `\r\n`, as a
/// kernel does with `ONLCR`.
pub(crate) fn output(bytes: &[u8]) -> Vec<u8> {
	let mut out = Vec::with_capacity(bytes.len() + bytes.len() / 16);
	for &byte in bytes {
		if byte == b'\n' {
			out.push(b'\r');
		}
		out.push(byte);
	}
	out
}
