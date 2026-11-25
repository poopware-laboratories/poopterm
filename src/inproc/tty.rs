//! The line discipline between the keyboard and a running program, as a
//! kernel's terminal driver runs it, for the in-process device where iOS
//! has no pseudo-terminal: the program's `termios` settings say whether
//! keys reach it a line at a time, edited and echoed (canonical), or as
//! they come (raw, for editors and pagers), and whether `^C` interrupts.

/// The `termios` settings the discipline follows, read from the
/// program's own (`Mode::of`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Mode {
	/// `ICANON`: lines are edited here and sent whole.
	pub canonical: bool,
	/// `ECHO`: what is typed is shown.
	pub echo: bool,
	/// `ECHOE`: erasing rubs the character out on screen.
	pub echo_erase: bool,
	/// `ECHOCTL`: control characters echo as `^X`.
	pub echo_controls: bool,
	/// `ISIG`: the interrupt and quit characters interrupt.
	pub signals: bool,
	/// `ICRNL`: Return reads as `\n`.
	pub return_is_newline: bool,
	/// `OPOST` with `ONLCR`: `\n` out as `\r\n`.
	pub newline_out: bool,
	pub interrupt: Option<u8>,
	pub quit: Option<u8>,
	pub erase: Option<u8>,
	pub kill: Option<u8>,
	pub word_erase: Option<u8>,
	pub end_of_file: Option<u8>,
	/// `VLNEXT`: sends the next character as it is.
	pub literal_next: Option<u8>,
}

impl Default for Mode {
	/// A login terminal's settings, as `stty sane` sets them.
	fn default() -> Self {
		Self {
			canonical: true,
			echo: true,
			echo_erase: true,
			echo_controls: true,
			signals: true,
			return_is_newline: true,
			newline_out: true,
			interrupt: Some(0x03),
			quit: Some(0x1c),
			erase: Some(0x7f),
			kill: Some(0x15),
			word_erase: Some(0x17),
			end_of_file: Some(0x04),
			literal_next: Some(0x16),
		}
	}
}

/// What keys did to the program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
	/// Show these bytes (output processing still applies).
	Echo(Vec<u8>),
	/// The program reads these bytes.
	Send(Vec<u8>),
	/// Interrupt the program (`^C`), or make it quit (`^\`).
	Interrupt,
	Quit,
	/// End of input: `^D` on an empty line.
	EndOfFile,
}

/// Most bytes a line being edited holds, as a kernel's buffer is bounded;
/// keys past it are dropped until the line is sent.
pub(crate) const MAX_LINE: usize = 4096;

/// The line being edited for the program, in canonical mode.
#[derive(Debug, Default)]
pub(crate) struct Line {
	line: Vec<u8>,
	/// The character after `VLNEXT` is taken as it is.
	literal: bool,
}

impl Line {
	/// Feeds what was typed, with the program's settings now.
	pub(crate) fn feed(&mut self, mode: &Mode, bytes: &[u8]) -> Vec<Event> {
		let mut events = Vec::new();
		let mut raw = Vec::new();
		for &typed in bytes {
			let byte = if typed == b'\r' && mode.return_is_newline && !self.literal {
				b'\n'
			} else {
				typed
			};
			if self.literal {
				self.literal = false;
				self.keep(mode, byte, &mut raw, &mut events);
				continue;
			}
			if mode.signals && Some(byte) == mode.interrupt {
				self.signal(mode, byte, Event::Interrupt, &mut raw, &mut events);
				continue;
			}
			if mode.signals && Some(byte) == mode.quit {
				self.signal(mode, byte, Event::Quit, &mut raw, &mut events);
				continue;
			}
			if !mode.canonical {
				raw.push(byte);
				if mode.echo {
					echo(mode, byte, &mut events);
				}
				continue;
			}
			if mode.literal_next == Some(byte) {
				self.literal = true;
				continue;
			}
			if Some(byte) == mode.erase || (mode.erase == Some(0x7f) && byte == 0x08) {
				let erased = self.erase_char();
				self.rub_out(mode, &erased, &mut events);
			} else if Some(byte) == mode.word_erase {
				let erased = self.erase_word();
				self.rub_out(mode, &erased, &mut events);
			} else if Some(byte) == mode.kill {
				let erased = std::mem::take(&mut self.line);
				self.rub_out(mode, &erased, &mut events);
			} else if Some(byte) == mode.end_of_file {
				if self.line.is_empty() {
					events.push(Event::EndOfFile);
				} else {
					events.push(Event::Send(std::mem::take(&mut self.line)));
				}
			} else {
				self.keep(mode, byte, &mut raw, &mut events);
			}
		}
		if !raw.is_empty() {
			events.push(Event::Send(raw));
		}
		merge_echoes(events)
	}

	/// What was typed on the line and not sent yet, taken off it: when the
	/// program leaves canonical mode, it reads that at once, as from a
	/// kernel's terminal.
	pub(crate) fn take_pending(&mut self) -> Vec<u8> {
		self.literal = false;
		std::mem::take(&mut self.line)
	}

	/// A character that goes into the line (or straight out when raw); a
	/// newline sends the line.
	fn keep(&mut self, mode: &Mode, byte: u8, raw: &mut Vec<u8>, events: &mut Vec<Event>) {
		if !mode.canonical {
			raw.push(byte);
		} else if byte == b'\n' {
			self.line.push(byte);
			if mode.echo {
				events.push(Event::Echo(vec![b'\n']));
			}
			events.push(Event::Send(std::mem::take(&mut self.line)));
			return;
		} else if self.line.len() < MAX_LINE - 1 {
			self.line.push(byte);
		} else {
			return;
		}
		if mode.echo {
			echo(mode, byte, events);
		}
	}

	/// `^C` or `^\`: what was typed is dropped, the key is shown, and the
	/// program is signalled.
	fn signal(
		&mut self,
		mode: &Mode,
		byte: u8,
		event: Event,
		raw: &mut Vec<u8>,
		events: &mut Vec<Event>,
	) {
		self.line.clear();
		raw.clear();
		if mode.echo {
			echo(mode, byte, events);
			if mode.canonical {
				events.push(Event::Echo(vec![b'\n']));
			}
		}
		events.push(event);
	}

	/// Takes the last character (all of its UTF-8 bytes) off the line.
	fn erase_char(&mut self) -> Vec<u8> {
		let start = self
			.line
			.iter()
			.rposition(|&byte| byte & 0xc0 != 0x80)
			.unwrap_or(0);
		self.line.split_off(start)
	}

	/// Takes the last word and the blanks after it off the line.
	fn erase_word(&mut self) -> Vec<u8> {
		let blank = |byte: &u8| byte.is_ascii_whitespace();
		let end = self
			.line
			.iter()
			.rposition(|byte| !blank(byte))
			.map_or(0, |at| at + 1);
		let start = self.line[..end]
			.iter()
			.rposition(blank)
			.map_or(0, |at| at + 1);
		self.line.split_off(start)
	}

	/// Shows `erased` going away: a backspace, space, backspace for each
	/// column it took.
	fn rub_out(&self, mode: &Mode, erased: &[u8], events: &mut Vec<Event>) {
		if !mode.echo || erased.is_empty() {
			return;
		}
		if !mode.echo_erase {
			events.push(Event::Echo(erased.to_vec()));
			return;
		}
		let columns: usize = String::from_utf8_lossy(erased)
			.chars()
			.map(|ch| match ch {
				'\0'..='\x1f' | '\x7f' if mode.echo_controls => 2,
				_ => crate::width::width(ch).max(1),
			})
			.sum();
		let mut rub = Vec::with_capacity(columns * 3);
		for _ in 0..columns {
			rub.extend(b"\x08 \x08");
		}
		events.push(Event::Echo(rub));
	}
}

/// Echoes one typed byte: control characters as `^X` when `ECHOCTL` is on
/// (but tab and newline as themselves).
fn echo(mode: &Mode, byte: u8, events: &mut Vec<Event>) {
	let shown = match byte {
		b'\t' | b'\n' => vec![byte],
		0x00..=0x1f if mode.echo_controls => vec![b'^', byte + 0x40],
		0x7f if mode.echo_controls => b"^?".to_vec(),
		_ => vec![byte],
	};
	events.push(Event::Echo(shown));
}

/// Joins echoes that follow each other, so they are written at once.
fn merge_echoes(events: Vec<Event>) -> Vec<Event> {
	let mut merged: Vec<Event> = Vec::with_capacity(events.len());
	for event in events {
		match (merged.last_mut(), event) {
			(Some(Event::Echo(before)), Event::Echo(more)) => before.extend(more),
			(_, event) => merged.push(event),
		}
	}
	merged
}

/// What a program wrote, as the screen gets it: `\n` as `\r\n` when the
/// settings say so.
pub(crate) fn output(mode: &Mode, bytes: &[u8]) -> Vec<u8> {
	if mode.newline_out {
		super::line::output(bytes)
	} else {
		bytes.to_vec()
	}
}

#[cfg(unix)]
impl Mode {
	/// The settings a `termios` holds.
	pub(crate) fn of(settings: &libc::termios) -> Self {
		let control = |index: usize| {
			let value = settings.c_cc[index];
			// `_POSIX_VDISABLE` (0xff on the BSDs, 0 on Linux) turns one off.
			(value != 0xff && value != 0).then_some(value)
		};
		let local = |flag| settings.c_lflag & flag != 0;
		Self {
			canonical: local(libc::ICANON),
			echo: local(libc::ECHO),
			echo_erase: local(libc::ECHOE),
			echo_controls: local(libc::ECHOCTL),
			signals: local(libc::ISIG),
			return_is_newline: settings.c_iflag & libc::ICRNL != 0,
			newline_out: settings.c_oflag & libc::OPOST != 0 && settings.c_oflag & libc::ONLCR != 0,
			interrupt: control(libc::VINTR),
			quit: control(libc::VQUIT),
			erase: control(libc::VERASE),
			kill: control(libc::VKILL),
			word_erase: control(libc::VWERASE),
			end_of_file: control(libc::VEOF),
			literal_next: control(libc::VLNEXT),
		}
	}
}

#[cfg(test)]
#[path = "tty_tests.rs"]
mod tests;
