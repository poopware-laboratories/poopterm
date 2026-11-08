//! The settings a new PTY starts with, shown by what line editing does with
//! typed keys on a real PTY.
use super::*;
use crate::pty::device::{self, WindowSize};
use std::{
	fs::File,
	io::{Read, Write},
};

/// A new pair: the keyboard (master) and what the program reads (slave).
fn pair() -> (File, File) {
	let pair = device::open(WindowSize {
		cols: 80,
		rows: 24,
		..WindowSize::default()
	})
	.unwrap();
	(File::from(pair.master), File::from(pair.slave))
}

/// What a program reading the slave gets after `typed` on the keyboard:
/// one finished line, as line editing hands it over.
fn line(typed: &[u8]) -> Vec<u8> {
	let (mut keyboard, mut program) = pair();
	keyboard.write_all(typed).unwrap();
	let mut line = vec![0; 256];
	let n = program.read(&mut line).unwrap();
	line.truncate(n);
	line
}

#[test]
fn backspace_is_del_and_erases_one_character() {
	assert_eq!(line(b"ab\x7f\n"), b"a\n");
	// ^H is a character like any other, not the erase key.
	assert_eq!(line(b"ab\x08\n"), b"ab\x08\n");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn backspace_erases_a_whole_utf8_character() {
	assert_eq!(line("aé\x7f\n".as_bytes()), b"a\n");
	assert_eq!(line("x😀\x7f\x7f\n".as_bytes()), b"\n");
}

#[test]
fn ctrl_u_kills_the_line_and_ctrl_w_the_word() {
	assert_eq!(line(b"abc\x15d\n"), b"d\n");
	assert_eq!(line(b"one two\x17three\n"), b"one three\n");
}

#[test]
fn return_arrives_as_a_newline() {
	assert_eq!(line(b"cd /tmp\r"), b"cd /tmp\n");
}

#[test]
fn ctrl_v_quotes_the_next_key() {
	assert_eq!(line(b"a\x16\x7f\n"), b"a\x7f\n");
}

#[test]
fn ctrl_d_at_the_start_of_a_line_ends_input() {
	let (mut keyboard, mut program) = pair();
	keyboard.write_all(b"\x04").unwrap();
	assert_eq!(program.read(&mut [0; 16]).unwrap(), 0);
	// Mid-line, it hands over what was typed, without a newline.
	keyboard.write_all(b"ab\x04").unwrap();
	let mut buf = [0; 16];
	let n = program.read(&mut buf).unwrap();
	assert_eq!(&buf[..n], b"ab");
}

#[test]
fn typed_keys_echo_with_control_characters_shown() {
	let (mut keyboard, program) = pair();
	keyboard.write_all(b"hi\x07\n").unwrap();
	let mut echo = vec![0; 64];
	let n = keyboard.read(&mut echo).unwrap();
	assert_eq!(&echo[..n], b"hi^G\r\n");
	drop(program);
}

#[test]
fn defaults_undo_raw_mode() {
	let (mut keyboard, mut program) = pair();
	let mut raw = get(&program).unwrap();
	// SAFETY: makes `raw`, which we own, raw.
	unsafe { libc::cfmakeraw(&mut raw) };
	set(&program, &raw).unwrap();
	set(&program, &sane(get(&program).unwrap())).unwrap();
	keyboard.write_all(b"ab\x7f\r").unwrap();
	let mut line = [0; 16];
	let n = program.read(&mut line).unwrap();
	assert_eq!(&line[..n], b"a\n");
}

#[test]
fn defaults_turn_off_what_would_mangle_lines() {
	let (keyboard, program) = pair();
	let mut odd = get(&program).unwrap();
	odd.c_iflag |= libc::INLCR | libc::IGNCR;
	odd.c_oflag |= libc::OCRNL;
	odd.c_lflag |= libc::ECHONL | libc::TOSTOP;
	let fixed = sane(odd);
	for (flags, bit) in [
		(fixed.c_iflag, libc::INLCR),
		(fixed.c_iflag, libc::IGNCR),
		(fixed.c_oflag, libc::OCRNL),
		(fixed.c_lflag, libc::ECHONL),
	] {
		assert_eq!(flags & bit, 0, "{bit:#x}");
	}
	// Unrelated settings stay as they were.
	assert_ne!(fixed.c_lflag & libc::TOSTOP, 0);
	drop((keyboard, program));
}

#[test]
fn speeds_are_set_where_programs_look_for_them() {
	let (_keyboard, program) = pair();
	let settings = get(&program).unwrap();
	// SAFETY: reads a field of a termios we own.
	assert_eq!(unsafe { libc::cfgetospeed(&settings) }, libc::B38400);
	// SAFETY: as above.
	assert_eq!(unsafe { libc::cfgetispeed(&settings) }, libc::B38400);
}

#[test]
fn settings_round_trip_through_the_kernel() {
	let (_keyboard, program) = pair();
	let mut settings = get(&program).unwrap();
	settings.c_cc[libc::VINTR] = 0x07;
	set(&program, &settings).unwrap();
	assert_eq!(get(&program).unwrap().c_cc[libc::VINTR], 0x07);
	assert_eq!(get(&_keyboard).unwrap().c_cc[libc::VINTR], 0x07);
}

#[test]
fn something_that_is_not_a_terminal_has_no_settings() {
	let file = File::open("Cargo.toml").unwrap();
	assert_eq!(get(&file).unwrap_err().raw_os_error(), Some(libc::ENOTTY));
}
