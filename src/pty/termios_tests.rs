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

/// Everything the keyboard side gets back within a moment: the echo.
fn echo_of(keyboard: &mut File) -> Vec<u8> {
	let mut keyboard = keyboard.try_clone().unwrap();
	let (send, heard) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let mut buf = [0; 256];
		while let Ok(n @ 1..) = keyboard.read(&mut buf) {
			if send.send(buf[..n].to_vec()).is_err() {
				return;
			}
		}
	});
	let mut echo = Vec::new();
	while let Ok(bytes) = heard.recv_timeout(std::time::Duration::from_millis(200)) {
		echo.extend_from_slice(&bytes);
	}
	echo
}

#[test]
fn ctrl_r_reprints_the_line() {
	let (mut keyboard, _program) = pair();
	keyboard.write_all(b"ab\x12").unwrap();
	assert_eq!(echo_of(&mut keyboard), b"ab^R\r\nab");
}

#[test]
fn erasing_wipes_the_character_off_the_screen() {
	let (mut keyboard, _program) = pair();
	keyboard.write_all(b"ab\x7f").unwrap();
	assert_eq!(echo_of(&mut keyboard), b"ab\x08 \x08");
}

#[test]
fn ctrl_c_echoes_and_drops_the_line_typed_so_far() {
	let (mut keyboard, mut program) = pair();
	keyboard.write_all(b"abc\x03").unwrap();
	assert!(echo_of(&mut keyboard).ends_with(b"^C"));
	keyboard.write_all(b"def\n").unwrap();
	let mut line = [0; 16];
	let n = program.read(&mut line).unwrap();
	assert_eq!(&line[..n], b"def\n");
}

#[test]
fn ctrl_s_stops_output_until_ctrl_q() {
	let (mut keyboard, mut program) = pair();
	let mut screen = keyboard.try_clone().unwrap();
	let (send, shown) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let mut buf = [0; 64];
		while let Ok(n @ 1..) = screen.read(&mut buf) {
			if send.send(buf[..n].to_vec()).is_err() {
				return;
			}
		}
	});
	keyboard.write_all(b"\x13").unwrap();
	// While stopped, the program's write waits rather than reaching the screen.
	let writing = std::thread::spawn(move || program.write_all(b"held").unwrap());
	let moment = std::time::Duration::from_millis(300);
	assert!(shown.recv_timeout(moment).is_err(), "output while stopped");
	keyboard.write_all(b"\x11").unwrap();
	assert_eq!(
		shown
			.recv_timeout(std::time::Duration::from_secs(5))
			.unwrap(),
		b"held"
	);
	writing.join().unwrap();
}

#[test]
fn lines_are_handed_over_one_at_a_time() {
	let (mut keyboard, mut program) = pair();
	keyboard.write_all(b"one\ntwo\n").unwrap();
	let mut line = [0; 16];
	let n = program.read(&mut line).unwrap();
	assert_eq!(&line[..n], b"one\n");
	let n = program.read(&mut line).unwrap();
	assert_eq!(&line[..n], b"two\n");
}

#[test]
fn eight_bit_input_passes_unchanged() {
	assert_eq!(line(b"\xe9\xa0\xff\n"), b"\xe9\xa0\xff\n");
}

#[test]
fn without_line_editing_a_read_returns_each_key_at_once() {
	let (mut keyboard, mut program) = pair();
	let mut settings = get(&program).unwrap();
	settings.c_lflag &= !libc::ICANON;
	set(&program, &settings).unwrap();
	keyboard.write_all(b"x").unwrap();
	let mut key = [0; 16];
	assert_eq!(program.read(&mut key).unwrap(), 1);
	assert_eq!(key[0], b'x');
}
