//! The terminal settings a new PTY starts with, set explicitly instead of
//! left to each kernel's defaults: what a login terminal has (line editing,
//! signals, echo, `\n` out as `\r\n`, the usual control characters), plus
//! `IUTF8` where the system has it, so Backspace erases a whole UTF-8
//! character.

use std::{
	io,
	os::fd::{AsFd, AsRawFd},
};

/// `IUTF8`, where the libc crate declares it (not for FreeBSD).
#[cfg(any(target_os = "linux", target_os = "macos"))]
const IUTF8: libc::tcflag_t = libc::IUTF8;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const IUTF8: libc::tcflag_t = 0;

/// The control characters, as login terminals set them: `^C` interrupts,
/// `^\` quits, DEL erases, `^U` kills the line, `^W` the word, `^D` ends
/// input, `^Z` suspends, `^Q`/`^S` start and stop output, `^R` reprints,
/// `^V` quotes the next character; reads wait for one byte, without a timer.
const CONTROLS: [(usize, libc::cc_t); 13] = [
	(libc::VINTR, 0x03),
	(libc::VQUIT, 0x1c),
	(libc::VERASE, 0x7f),
	(libc::VKILL, 0x15),
	(libc::VWERASE, 0x17),
	(libc::VEOF, 0x04),
	(libc::VSUSP, 0x1a),
	(libc::VSTART, 0x11),
	(libc::VSTOP, 0x13),
	(libc::VREPRINT, 0x12),
	(libc::VLNEXT, 0x16),
	(libc::VMIN, 1),
	(libc::VTIME, 0),
];

/// `settings` with the defaults set and what would undo them cleared:
/// input `\r` read as `\n` and `^S`/`^Q` flow control; output `\n` written
/// as `\r\n`; 8-bit characters; line editing with signals and echo, control
/// characters echoed as `^X`; 38400 baud, which programs read as fast.
pub(crate) fn sane(mut settings: libc::termios) -> libc::termios {
	settings.c_iflag |= libc::ICRNL | libc::IXON | IUTF8;
	settings.c_iflag &= !(libc::INLCR | libc::IGNCR | libc::ISTRIP);
	settings.c_oflag |= libc::OPOST | libc::ONLCR;
	settings.c_oflag &= !libc::OCRNL;
	settings.c_cflag |= libc::CS8 | libc::CREAD | libc::HUPCL;
	settings.c_cflag &= !libc::PARENB;
	let editing = libc::ISIG | libc::ICANON | libc::IEXTEN | libc::ECHO | libc::ECHOE;
	settings.c_lflag |= editing | libc::ECHOK | libc::ECHOCTL | libc::ECHOKE;
	settings.c_lflag &= !(libc::ECHONL | libc::NOFLSH);
	for (control, value) in CONTROLS {
		settings.c_cc[control] = value;
	}
	// SAFETY: sets a speed field of `settings`, which we own, to a valid speed.
	unsafe { libc::cfsetispeed(&mut settings, libc::B38400) };
	// SAFETY: as above.
	unsafe { libc::cfsetospeed(&mut settings, libc::B38400) };
	settings
}

/// The settings of the terminal `fd` is a side of.
pub(crate) fn get(fd: &impl AsFd) -> io::Result<libc::termios> {
	let mut settings = std::mem::MaybeUninit::uninit();
	// SAFETY: on success `tcgetattr` fills the whole `termios`, read only then.
	if unsafe { libc::tcgetattr(fd.as_fd().as_raw_fd(), settings.as_mut_ptr()) } != 0 {
		return Err(io::Error::last_os_error());
	}
	// SAFETY: filled above.
	Ok(unsafe { settings.assume_init() })
}

/// Sets the settings of the terminal `fd` is a side of, at once.
pub(crate) fn set(fd: &impl AsFd, settings: &libc::termios) -> io::Result<()> {
	// SAFETY: `tcsetattr` reads one `termios`, which outlives the call.
	match unsafe { libc::tcsetattr(fd.as_fd().as_raw_fd(), libc::TCSANOW, settings) } {
		0 => Ok(()),
		_ => Err(io::Error::last_os_error()),
	}
}

#[cfg(test)]
#[path = "termios_tests.rs"]
mod tests;
