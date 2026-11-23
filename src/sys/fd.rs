//! Descriptor flags the standard library cannot set.
use std::{
	io,
	os::fd::{AsFd, AsRawFd},
};

/// Makes writes and reads on `fd` fail with `WouldBlock` instead of
/// waiting: the keyboard's pipe to a program that does not read it must
/// not hold up the keys after (`^C` above all).
pub(crate) fn set_nonblocking(fd: &impl AsFd) -> io::Result<()> {
	let raw = fd.as_fd().as_raw_fd();
	// SAFETY: F_GETFL takes no argument and only reads the descriptor's flags.
	let flags = unsafe { libc::fcntl(raw, libc::F_GETFL) };
	if flags < 0 {
		return Err(io::Error::last_os_error());
	}
	// SAFETY: F_SETFL takes the flags as an int and changes only this
	// descriptor's status flags.
	if unsafe { libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(())
}

#[cfg(test)]
#[path = "fd_tests.rs"]
mod tests;
