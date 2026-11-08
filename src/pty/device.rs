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
	pub path: PathBuf,
}

/// A C call's result, or the error it left in `errno` when negative.
fn check(result: libc::c_int) -> io::Result<libc::c_int> {
	(result >= 0)
		.then_some(result)
		.ok_or_else(io::Error::last_os_error)
}

/// A new PTY pair of `size`.
pub(crate) fn open(size: WindowSize) -> io::Result<Pair> {
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
	set_size(&master, size)?;
	Ok(Pair {
		master,
		slave: slave.into(),
		path,
	})
}

/// Where the slave of `master` lives. Linux has the reentrant
/// `ptsname_r`; elsewhere `ptsname` fills a static buffer, so our calls
/// take turns and copy the name out first.
fn slave_path(master: &OwnedFd) -> io::Result<PathBuf> {
	#[cfg(target_os = "linux")]
	{
		let mut name = [0u8; 128];
		// SAFETY: writes at most `name.len()` bytes, NUL included, into `name`.
		let error =
			unsafe { libc::ptsname_r(master.as_raw_fd(), name.as_mut_ptr().cast(), name.len()) };
		if error != 0 {
			return Err(io::Error::from_raw_os_error(error));
		}
		let name = std::ffi::CStr::from_bytes_until_nul(&name).map_err(io::Error::other)?;
		Ok(PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes())))
	}
	#[cfg(not(target_os = "linux"))]
	{
		static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
		let _turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
		// SAFETY: `master` is an unlocked PTY master. The result is null or a
		// C string in a static buffer, read before the turn ends.
		let name = unsafe { libc::ptsname(master.as_raw_fd()) };
		if name.is_null() {
			return Err(io::Error::last_os_error());
		}
		// SAFETY: not null, so a NUL-terminated string, unchanged meanwhile.
		let name = unsafe { std::ffi::CStr::from_ptr(name) };
		Ok(PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes())))
	}
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
