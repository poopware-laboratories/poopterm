use super::*;

fn raw() -> Mode {
	Mode {
		canonical: false,
		echo: false,
		signals: false,
		return_is_newline: false,
		newline_out: false,
		..Mode::default()
	}
}

fn feed(mode: &Mode, bytes: &[u8]) -> Vec<Event> {
	Line::default().feed(mode, bytes)
}

fn echo(text: &[u8]) -> Event {
	Event::Echo(text.to_vec())
}

fn send(text: &[u8]) -> Event {
	Event::Send(text.to_vec())
}

#[test]
fn a_canonical_line_is_echoed_then_sent_whole_on_return() {
	let mode = Mode::default();
	let mut line = Line::default();
	assert_eq!(
		line.feed(&mode, b"hi"),
		[echo(b"hi")],
		"nothing sent before Return"
	);
	assert_eq!(line.feed(&mode, b"\r"), [echo(b"\n"), send(b"hi\n")]);
	assert_eq!(
		line.feed(&mode, b"a\nb\r"),
		[echo(b"a\n"), send(b"a\n"), echo(b"b\n"), send(b"b\n")]
	);
}

#[test]
fn return_stays_a_carriage_return_without_icrnl() {
	let mode = Mode {
		return_is_newline: false,
		..Mode::default()
	};
	assert_eq!(
		feed(&mode, b"x\r"),
		[echo(b"x^M")],
		"\\r is no line end then"
	);
}

#[test]
fn erasing_takes_back_whole_characters_and_their_columns() {
	let mode = Mode::default();
	let mut line = Line::default();
	line.feed(&mode, "aé漢".as_bytes());
	assert_eq!(
		line.feed(&mode, b"\x7f"),
		[echo(b"\x08 \x08\x08 \x08")],
		"a wide one takes two"
	);
	assert_eq!(
		line.feed(&mode, b"\x08"),
		[echo(b"\x08 \x08")],
		"backspace erases too"
	);
	assert_eq!(line.feed(&mode, b"\r"), [echo(b"\n"), send(b"a\n")]);
	assert_eq!(
		line.feed(&mode, b"\x7f"),
		[],
		"nothing to erase on an empty line"
	);
}

#[test]
fn erasing_without_echoe_echoes_what_went() {
	let mode = Mode {
		echo_erase: false,
		..Mode::default()
	};
	let mut line = Line::default();
	line.feed(&mode, b"ab");
	assert_eq!(line.feed(&mode, b"\x7f"), [echo(b"b")]);
}

#[test]
fn word_erase_and_kill_drop_a_word_and_the_line() {
	let mode = Mode::default();
	let mut line = Line::default();
	line.feed(&mode, b"git  commit  ");
	assert_eq!(
		line.feed(&mode, b"\x17"),
		[echo(&b"\x08 \x08".repeat(8))],
		"the word and blanks after it"
	);
	assert_eq!(
		line.feed(&mode, b"\x15"),
		[echo(&b"\x08 \x08".repeat(5))],
		"the rest of the line"
	);
	assert_eq!(line.feed(&mode, b"ok\r"), [echo(b"ok\n"), send(b"ok\n")]);
}

#[test]
fn end_of_file_sends_a_partial_line_or_ends_input_on_an_empty_one() {
	let mode = Mode::default();
	let mut line = Line::default();
	assert_eq!(
		line.feed(&mode, b"abc\x04"),
		[echo(b"abc"), send(b"abc")],
		"no newline added"
	);
	assert_eq!(line.feed(&mode, b"\x04"), [Event::EndOfFile]);
}

#[test]
fn interrupt_and_quit_drop_the_line_and_signal() {
	let mode = Mode::default();
	let mut line = Line::default();
	assert_eq!(
		line.feed(&mode, b"half\x03"),
		[echo(b"half^C\n"), Event::Interrupt]
	);
	assert_eq!(
		line.feed(&mode, b"\r"),
		[echo(b"\n"), send(b"\n")],
		"the half line is gone"
	);
	assert_eq!(feed(&mode, b"\x1c"), [echo(b"^\\\n"), Event::Quit]);
}

#[test]
fn without_isig_the_interrupt_key_is_just_a_character() {
	let mode = Mode {
		signals: false,
		..raw()
	};
	assert_eq!(feed(&mode, b"\x03\x1c"), [send(b"\x03\x1c")]);
}

#[test]
fn raw_mode_sends_keys_as_they_come_without_echo() {
	let mode = raw();
	assert_eq!(feed(&mode, b"\x1b[A\x7fq\r"), [send(b"\x1b[A\x7fq\r")]);
}

#[test]
fn raw_mode_with_signals_still_interrupts_and_keeps_later_keys() {
	let mode = Mode {
		signals: true,
		..raw()
	};
	assert_eq!(
		feed(&mode, b"ab\x03cd"),
		[Event::Interrupt, send(b"cd")],
		"what came before ^C is dropped"
	);
}

#[test]
fn raw_mode_with_echo_echoes_controls_as_carets() {
	let mode = Mode {
		echo: true,
		..raw()
	};
	assert_eq!(feed(&mode, b"a\x01\t"), [echo(b"a^A\t"), send(b"a\x01\t")]);
	let plain = Mode {
		echo_controls: false,
		..mode
	};
	assert_eq!(feed(&plain, b"\x01"), [echo(b"\x01"), send(b"\x01")]);
}

#[test]
fn literal_next_takes_a_control_character_into_the_line() {
	let mode = Mode::default();
	assert_eq!(
		feed(&mode, b"\x16\x03\x16\x7f\r"),
		[echo(b"^C^?\n"), send(b"\x03\x7f\n")]
	);
	assert_eq!(
		feed(&mode, b"\x16\r\n"),
		[echo(b"^M\n"), send(b"\r\n")],
		"even Return, as \\r"
	);
}

#[test]
fn echo_off_still_edits_and_sends_but_shows_nothing() {
	let mode = Mode {
		echo: false,
		..Mode::default()
	};
	assert_eq!(
		feed(&mode, b"pw\x7fd\r"),
		[send(b"pd\n")],
		"a password prompt"
	);
}

#[test]
fn a_line_stops_growing_at_its_limit_but_still_ends() {
	let mode = Mode {
		echo: false,
		..Mode::default()
	};
	let mut line = Line::default();
	line.feed(&mode, &vec![b'x'; MAX_LINE + 50]);
	let sent = line.feed(&mode, b"\r");
	assert_eq!(
		sent,
		[send(&[vec![b'x'; MAX_LINE - 1], vec![b'\n']].concat())]
	);
}

#[test]
fn a_disabled_control_character_does_nothing_special() {
	let mode = Mode {
		erase: None,
		kill: None,
		..Mode::default()
	};
	assert_eq!(feed(&mode, b"a\x15\r"), [echo(b"a^U\n"), send(b"a\x15\n")]);
}

#[test]
fn output_adds_carriage_returns_only_when_asked() {
	assert_eq!(output(&Mode::default(), b"a\nb"), b"a\r\nb");
	assert_eq!(output(&raw(), b"a\nb"), b"a\nb");
}

#[cfg(target_vendor = "apple")]
#[test]
fn settings_read_from_termios_as_the_flags_say() {
	let mut settings = libc::termios {
		c_iflag: 0,
		c_oflag: 0,
		c_cflag: 0,
		c_lflag: 0,
		c_cc: [0; libc::NCCS],
		c_ispeed: 0,
		c_ospeed: 0,
	};
	let off = Mode::of(&settings);
	assert!(!off.canonical && !off.echo && !off.signals && !off.newline_out);
	assert_eq!(off.interrupt, None, "0 is no character");
	settings.c_lflag = libc::ICANON | libc::ECHO | libc::ISIG | libc::ECHOE | libc::ECHOCTL;
	settings.c_iflag = libc::ICRNL;
	settings.c_oflag = libc::ONLCR;
	settings.c_cc[libc::VINTR] = 0x03;
	settings.c_cc[libc::VERASE] = 0xff;
	let on = Mode::of(&settings);
	assert!(on.canonical && on.echo && on.signals && on.echo_erase && on.echo_controls);
	assert!(on.return_is_newline && !on.newline_out, "ONLCR needs OPOST");
	assert_eq!(
		(on.interrupt, on.erase),
		(Some(0x03), None),
		"0xff is _POSIX_VDISABLE"
	);
	settings.c_oflag |= libc::OPOST;
	assert!(Mode::of(&settings).newline_out);
}

#[test]
fn the_pending_line_can_be_taken_whole() {
	let mode = Mode::default();
	let mut line = Line::default();
	line.feed(&mode, b"ab\x16");
	assert_eq!(line.take_pending(), b"ab");
	assert_eq!(
		line.feed(&mode, b"\x03"),
		[echo(b"^C\n"), Event::Interrupt],
		"no literal left over"
	);
	assert_eq!(line.take_pending(), b"");
}
