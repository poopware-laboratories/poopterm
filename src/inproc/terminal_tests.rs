use std::{io::Read, time::Duration};

use super::*;
use crate::inproc::tests::Screen;

fn terminal() -> (Arc<Terminal>, Screen) {
	crate::sys::signal::quiet_job_signals();
	let screen = Screen::default();
	let terminal = Terminal::new(Box::new(screen.clone()), Arc::new(Size::new(80, 24)));
	(terminal, screen)
}

/// Everything the program's end of the keyboard reads, once it ends.
fn read_all(mut keys: PipeReader) -> Vec<u8> {
	let mut read = Vec::new();
	keys.read_to_end(&mut read).unwrap();
	read
}

#[test]
fn keys_go_to_the_shell_while_no_program_runs() {
	let (terminal, screen) = terminal();
	assert_eq!(terminal.keyboard(b"ls\r"), Some(b"ls\r".to_vec()));
	assert_eq!(screen.shown(), "", "the shell echoes its own line");
}

#[test]
fn a_program_reading_the_keyboard_gets_whole_echoed_lines() {
	let (terminal, screen) = terminal();
	let keys = terminal.attach().unwrap();
	assert_eq!(
		terminal.keyboard(b"hi\x7fo\r"),
		None,
		"the program's, not the shell's"
	);
	assert_eq!(terminal.keyboard(b"\x04"), None);
	assert_eq!(read_all(keys), b"ho\n", "edited, then ended by ^D");
	assert_eq!(
		screen.shown(),
		"hi\x08 \x08o\r\n",
		"echo, with \\n shown as \\r\\n"
	);
	terminal.detach();
	assert_eq!(
		terminal.keyboard(b"x"),
		Some(b"x".to_vec()),
		"the shell's again"
	);
}

#[test]
fn a_program_in_raw_mode_gets_keys_at_once_and_unprocessed_output() {
	let (terminal, screen) = terminal();
	let keys = terminal.attach().unwrap();
	let mut raw = terminal.settings();
	raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
	raw.c_iflag &= !libc::ICRNL;
	raw.c_oflag &= !libc::OPOST;
	terminal.set_settings(&raw, false);
	terminal.keyboard(b"\x1b[A\x03\r");
	terminal.write(b"a\nb").unwrap();
	terminal.detach();
	assert_eq!(
		read_all(keys),
		b"\x1b[A\x03\r",
		"arrows, ^C and Return as they are"
	);
	assert_eq!(screen.shown(), "a\nb", "no echo, no \\r added");
	assert!(!terminal.interrupted(), "^C was a key, not an interrupt");
}

#[test]
fn interrupting_marks_the_program_and_drops_what_was_typed() {
	let (terminal, screen) = terminal();
	let keys = terminal.attach().unwrap();
	terminal.keyboard(b"half\x03");
	assert!(terminal.interrupted());
	terminal.detach();
	assert_eq!(read_all(keys), b"", "the half line never reached it");
	assert_eq!(screen.shown(), "half^C\r\n");
	assert!(terminal.take_interrupted());
	assert!(!terminal.interrupted(), "asking clears it");
}

#[test]
fn a_new_program_starts_uninterrupted_with_a_fresh_line() {
	let (terminal, _) = terminal();
	let first = terminal.attach().unwrap();
	terminal.keyboard(b"left over\x03");
	terminal.keyboard(b"typed");
	terminal.detach();
	drop(first);
	let keys = terminal.attach().unwrap();
	assert!(!terminal.interrupted());
	terminal.keyboard(b"new\r");
	terminal.detach();
	assert_eq!(read_all(keys), b"new\n", "nothing of the line before");
}

#[test]
fn a_program_not_reading_the_keyboard_still_takes_keys_and_interrupts() {
	let (terminal, _) = terminal();
	let _keys = terminal.attach().unwrap();
	assert_eq!(
		terminal.keyboard(b"unread\r"),
		None,
		"the program's, read or not"
	);
	terminal.keyboard(b"\x03");
	assert!(terminal.interrupted());
	terminal.detach();
}

#[test]
fn keys_a_program_never_reads_are_dropped_once_its_pipe_is_full() {
	let (terminal, _) = terminal();
	let _keys = terminal.attach().unwrap();
	let mut raw = terminal.settings();
	raw.c_lflag &= !(libc::ICANON | libc::ECHO);
	terminal.set_settings(&raw, false);
	for _ in 0..64 {
		terminal.keyboard(&[b'k'; 4096]);
	}
	terminal.keyboard(b"\x03");
	assert!(terminal.interrupted(), "^C still got through");
	terminal.detach();
}

#[test]
fn keys_for_a_program_that_has_ended_are_dropped() {
	let (terminal, _) = terminal();
	let keys = terminal.attach().unwrap();
	drop(keys);
	assert_eq!(terminal.keyboard(b"late\r"), None, "no panic, no block");
	terminal.detach();
}

#[test]
fn flushing_settings_drop_the_line_being_typed() {
	let (terminal, _) = terminal();
	let keys = terminal.attach().unwrap();
	terminal.keyboard(b"stale");
	let settings = terminal.settings();
	terminal.set_settings(&settings, true);
	terminal.keyboard(b"fresh\r");
	terminal.detach();
	assert_eq!(read_all(keys), b"fresh\n");
}

#[test]
fn hanging_up_ends_the_programs_input() {
	let (terminal, _) = terminal();
	let keys = terminal.attach().unwrap();
	terminal.keyboard(b"partial");
	terminal.hang_up();
	assert_eq!(read_all(keys), b"", "an unfinished line is not sent");
	terminal.detach();
	terminal.hang_up();
}

#[test]
fn the_size_is_the_sessions_live_one() {
	let (terminal, _) = terminal();
	assert_eq!(terminal.size(), (80, 24));
	terminal.size.set(120, 40);
	assert_eq!(
		terminal.size(),
		(120, 40),
		"resizing signals no one here, safely"
	);
	assert_eq!(format!("{terminal:?}"), "Terminal");
}

#[test]
fn output_follows_the_settings_and_the_shell_writes_raw() {
	let (terminal, screen) = terminal();
	terminal.write(b"1\n").unwrap();
	terminal.write_raw(b"2\n").unwrap();
	assert_eq!(screen.shown(), "1\r\n2\n");
	assert_eq!(terminal.mode(), Mode::default());
}

#[test]
fn keys_typed_before_a_program_goes_raw_reach_it_then() {
	let (terminal, _) = terminal();
	let keys = terminal.attach().unwrap();
	terminal.keyboard(b" q");
	let mut raw = terminal.settings();
	raw.c_lflag &= !(libc::ICANON | libc::ECHO);
	terminal.set_settings(&raw, false);
	terminal.keyboard(b"x");
	terminal.detach();
	assert_eq!(read_all(keys), b" qx", "the half line first, then new keys");
}

#[test]
fn staying_canonical_keeps_the_line_being_typed() {
	let (terminal, _) = terminal();
	let keys = terminal.attach().unwrap();
	terminal.keyboard(b"half");
	let mut quiet = terminal.settings();
	quiet.c_lflag &= !libc::ECHO;
	terminal.set_settings(&quiet, false);
	terminal.keyboard(b" line\r");
	terminal.detach();
	assert_eq!(read_all(keys), b"half line\n");
}

#[test]
fn only_a_signalling_character_under_isig_splits_what_was_typed() {
	let mode = Mode::default();
	assert_eq!(
		split_at_signal(&mode, b"ab\x03cd"),
		Some((&b"ab\x03"[..], &b"cd"[..]))
	);
	assert_eq!(
		split_at_signal(&mode, b"\x1cx"),
		Some((&b"\x1c"[..], &b"x"[..])),
		"quit too"
	);
	assert_eq!(split_at_signal(&mode, b"plain"), None);
	let raw = Mode {
		signals: false,
		..mode
	};
	assert_eq!(
		split_at_signal(&raw, b"ab\x03cd"),
		None,
		"^C is a key in raw mode"
	);
}

#[test]
fn keys_typed_after_interrupting_a_program_that_ends_go_to_the_shell() {
	let (terminal, _) = terminal();
	let keys = terminal.attach().unwrap();
	assert_eq!(terminal.keyboard(b"\x03echo after\r"), None);
	assert_eq!(
		terminal.keyboard(b"ls\r"),
		None,
		"still held, the program is ending"
	);
	terminal.detach();
	assert_eq!(read_all(keys), b"", "none reached the program");
	assert_eq!(terminal.take_leftover(), b"echo after\rls\r");
	assert_eq!(terminal.take_leftover(), b"", "taken once");
}

#[test]
fn a_program_that_survives_interrupt_gets_the_held_keys_later() {
	let (terminal, _) = terminal();
	let keys = terminal.attach().unwrap();
	terminal.keyboard(b"\x03first ");
	std::thread::sleep(HOLD_AFTER_SIGNAL + Duration::from_millis(100));
	terminal.keyboard(b"second\r");
	terminal.detach();
	assert_eq!(
		read_all(keys),
		b"first second\n",
		"a REPL that caught ^C reads on"
	);
	assert_eq!(terminal.take_leftover(), b"");
}
