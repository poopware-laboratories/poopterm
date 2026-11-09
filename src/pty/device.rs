//! PTY pairs of our own, from POSIX: `posix_openpt`, `grantpt`, `unlockpt`
//! and `ptsname` allocate the pair, and the `TIOCSWINSZ`/`TIOCGWINSZ`
//! ioctls set and read its window size in cells and pixels. Both sides are
//! closed on exec and neither becomes our controlling terminal.

use std::{
	fs::OpenOptions,
	io,
	os::{
		fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
		unix::{ffi::OsStrExt, fs::OpenOptionsExt},
	},
	path::PathBuf,
};

/// A terminal's size, as `TIOCSWINSZ` sets it and programs read it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WindowSize {
	pub cols: u16,
	pub rows: u16,
	pub pixel_width: u16,
	pub pixel_height: u16,
}

impl WindowSize {
	/// A grid of `cols`×`rows` cells of `cell` pixels (width, height), each
	/// value saturated to what the ioctl carries.
	pub(crate) fn of(cols: usize, rows: usize, cell: (usize, usize)) -> Self {
		let fit = |n: usize| n.min(u16::MAX.into()) as u16;
		Self {
			cols: fit(cols),
			rows: fit(rows),
			pixel_width: fit(cols.saturating_mul(cell.0)),
			pixel_height: fit(rows.saturating_mul(cell.1)),
		}
	}
}

/// Both sides of a new PTY, closed when dropped.
pub(crate) struct Pair {
	/// The side the terminal reads output from and writes input to.
	pub master: OwnedFd,
	/// The side the program gets as its terminal.
	pub slave: OwnedFd,
	/// Where the slave lives, such as `/dev/pts/3`.
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "only tests name the slave so far")
	)]
	pub path: PathBuf,
}

/// A C call's result, or the error it left in `errno` when negative.
pub(super) fn check(result: libc::c_int) -> io::Result<libc::c_int> {
	(result >= 0)
		.then_some(result)
		.ok_or_else(io::Error::last_os_error)
}

/// A new PTY pair of `size`.
pub(crate) fn open(size: WindowSize) -> io::Result<Pair> {
	// macOS fails allocations made from several threads at once (errno -6,
	// seen in CI), so it makes them one at a time.
	#[cfg(target_os = "macos")]
	static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
	#[cfg(target_os = "macos")]
	let _turn = ONE_AT_A_TIME
		.lock()
		.unwrap_or_else(std::sync::PoisonError::into_inner);
	// macOS refuses `O_CLOEXEC` here; it gets the flag straight after.
	#[cfg(any(target_os = "linux", target_os = "freebsd"))]
	let flags = libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC;
	#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
	let flags = libc::O_RDWR | libc::O_NOCTTY;
	// SAFETY: takes only flags; a non-negative result is a new descriptor.
	let fd = check(unsafe { libc::posix_openpt(flags) })?;
	// SAFETY: `fd` was just opened and nothing else owns it.
	let master = unsafe { OwnedFd::from_raw_fd(fd) };
	#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
	// SAFETY: sets a descriptor flag on a descriptor we own.
	check(unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) })?;
	// SAFETY: `fd` is a PTY master we own; these take nothing else.
	check(unsafe { libc::grantpt(fd) })?;
	// SAFETY: as above.
	check(unsafe { libc::unlockpt(fd) })?;
	let path = slave_path(&master)?;
	let slave = OpenOptions::new()
		.read(true)
		.write(true)
		.custom_flags(libc::O_NOCTTY)
		.open(&path)?;
	super::termios::set(&slave, &super::termios::sane(super::termios::get(&slave)?))?;
	set_size(&master, size)?;
	Ok(Pair {
		master,
		slave: slave.into(),
		path,
	})
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
	/// libSystem's reentrant `ptsname` (macOS 10.13.4 and later), which the
	/// libc crate does not declare.
	fn ptsname_r(fd: libc::c_int, buf: *mut libc::c_char, len: libc::size_t) -> libc::c_int;
}
#[cfg(not(target_os = "macos"))]
use libc::ptsname_r;

/// Where the slave of `master` lives, from the reentrant `ptsname_r`:
/// plain `ptsname` fills one buffer for the whole process, which threads
/// opening PTYs at once (portable-pty's included) overwrite.
fn slave_path(master: &OwnedFd) -> io::Result<PathBuf> {
	let mut name = [0u8; 128];
	// SAFETY: writes at most `name.len()` bytes, NUL included, into `name`.
	let error = unsafe { ptsname_r(master.as_raw_fd(), name.as_mut_ptr().cast(), name.len()) };
	// Linux and FreeBSD return the error, macOS -1 and sets `errno`.
	match error {
		0 => {}
		-1 => return Err(io::Error::last_os_error()),
		error => return Err(io::Error::from_raw_os_error(error)),
	}
	let name = std::ffi::CStr::from_bytes_until_nul(&name).map_err(io::Error::other)?;
	Ok(PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes())))
}

/// `size` as the ioctls take it.
fn raw(size: WindowSize) -> libc::winsize {
	let (ws_row, ws_col) = (size.rows, size.cols);
	let (ws_xpixel, ws_ypixel) = (size.pixel_width, size.pixel_height);
	libc::winsize {
		ws_row,
		ws_col,
		ws_xpixel,
		ws_ypixel,
	}
}

/// Sets the window size of the terminal `fd` is a side of.
pub(crate) fn set_size(fd: &impl AsFd, size: WindowSize) -> io::Result<()> {
	// SAFETY: `TIOCSWINSZ` reads one `winsize`, which outlives the call.
	check(unsafe { libc::ioctl(fd.as_fd().as_raw_fd(), libc::TIOCSWINSZ, &raw(size)) })?;
	Ok(())
}

/// The window size of the terminal `fd` is a side of.
#[cfg_attr(
	not(test),
	expect(dead_code, reason = "only tests read a size back so far")
)]
pub(crate) fn size(fd: &impl AsFd) -> io::Result<WindowSize> {
	let mut size = raw(WindowSize::default());
	// SAFETY: `TIOCGWINSZ` writes one `winsize`, into `size`.
	check(unsafe { libc::ioctl(fd.as_fd().as_raw_fd(), libc::TIOCGWINSZ, &mut size) })?;
	Ok(WindowSize {
		cols: size.ws_col,
		rows: size.ws_row,
		pixel_width: size.ws_xpixel,
		pixel_height: size.ws_ypixel,
	})
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
