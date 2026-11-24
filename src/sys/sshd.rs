//! Calling the ssh server library (`ios/packages/dropbear/sshd_engine.c`):
//! Dropbear's protocol serving one connection a call, with the sessions it
//! starts handed to the app instead of forked. `Sessions` is what the app
//! does for them; `Server::serve` is the call.
use std::{
	ffi::{CStr, CString, c_char, c_int, c_long, c_void},
	os::fd::{FromRawFd, IntoRawFd, OwnedFd},
	path::Path,
};

use super::dl::Library;

/// What the app does for the sessions of a connection.
pub(crate) trait Sessions {
	/// Starts a shell (`command` none) or a command; `term` is the
	/// terminal a pty was asked for, none without one. The descriptors are
	/// the server's from now on: keys to write, output to read; and an id.
	/// Otherwise an errno.
	fn open(
		&self,
		command: Option<&str>,
		term: Option<&str>,
		size: (u16, u16),
	) -> Result<(OwnedFd, OwnedFd, i64), i32>;
	fn resize(&self, id: i64, size: (u16, u16));
	/// The exit status of a session that ended.
	fn status(&self, id: i64) -> Option<i32>;
	/// One line of the server's log.
	fn log(&self, line: &str);
}

/// Who may log in, and the host key.
pub(crate) struct Account<'a> {
	pub username: &'a str,
	pub home: &'a str,
	/// Empty: keys only.
	pub password: &'a str,
	pub hostkey: &'a str,
}

type Open = extern "C" fn(
	*mut c_void,
	*const c_char,
	*const c_char,
	c_int,
	c_int,
	*mut c_int,
	*mut c_int,
	*mut c_long,
) -> c_int;

/// `struct poopterm_sshd` of `sshd_engine.h`.
#[repr(C)]
struct Config {
	username: *const c_char,
	home: *const c_char,
	password: *const c_char,
	hostkey: *const c_char,
	open: Open,
	resize: extern "C" fn(*mut c_void, c_long, c_int, c_int),
	status: extern "C" fn(*mut c_void, c_long) -> c_int,
	log: extern "C" fn(*mut c_void, *const c_char),
	ctx: *mut c_void,
}

type Serve = extern "C" fn(c_int, *const Config) -> c_int;

/// The server library, open.
pub(crate) struct Server {
	serve: Serve,
	/// Kept open as long as the function is used.
	_library: Library,
}

// SAFETY: the library handle and the function pointer are plain addresses
// valid in every thread while the library stays open, which `Server` keeps;
// the app calls `serve` from one thread at a time, as the engine's state
// (one connection) requires.
unsafe impl Send for Server {}

/// Why `serve` did not run the connection.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refused {
	/// The host key file is missing.
	NoHostKey,
	Other(i32),
}

impl Server {
	pub(crate) fn open(path: &Path) -> Result<Self, String> {
		let name = CString::new(path.as_os_str().as_encoded_bytes())
			.map_err(|_| "a path with a NUL byte".to_owned())?;
		let library = Library::open(&name)?;
		let symbol = library
			.symbol(c"poopterm_sshd_serve")
			.ok_or_else(|| "not the ssh server library (no poopterm_sshd_serve)".to_owned())?;
		// SAFETY: the library exports `poopterm_sshd_serve` with the type
		// `sshd_engine.h` declares, and a function pointer has a data
		// pointer's size.
		let serve = unsafe { std::mem::transmute::<*mut c_void, Serve>(symbol.as_ptr()) };
		Ok(Self {
			serve,
			_library: library,
		})
	}

	/// Serves the connection on `socket` until it ends, which closes it;
	/// one connection at a time. A refusal leaves the socket to the caller
	/// (returned in `Err`).
	pub(crate) fn serve(
		&self,
		socket: OwnedFd,
		account: &Account,
		sessions: &dyn Sessions,
	) -> Result<(), (Refused, OwnedFd)> {
		let strings = [
			account.username,
			account.home,
			account.password,
			account.hostkey,
		]
		.map(|text| CString::new(text.replace('\0', "")).unwrap_or_default());
		// `ctx` is a thin pointer to the fat one, alive through the call.
		let context = std::ptr::from_ref(&sessions);
		let config = Config {
			username: strings[0].as_ptr(),
			home: strings[1].as_ptr(),
			password: strings[2].as_ptr(),
			hostkey: strings[3].as_ptr(),
			open,
			resize,
			status,
			log,
			ctx: context.cast_mut().cast(),
		};
		let raw = socket.into_raw_fd();
		// SAFETY-free call: `serve` is an `extern "C" fn` value.
		let result = (self.serve)(raw, &config);
		match result {
			0 => Ok(()),
			code => {
				// SAFETY: a refused call never took the descriptor: it
				// is still the one `into_raw_fd` released.
				let socket = unsafe { OwnedFd::from_raw_fd(raw) };
				Err((
					if code == -libc::ENOENT {
						Refused::NoHostKey
					} else {
						Refused::Other(code)
					},
					socket,
				))
			}
		}
	}
}

/// The sessions behind a callback's context.
///
/// # Safety
///
/// `ctx` is the context `serve` passed: a pointer to a `&dyn Sessions`
/// that lives until `serve` returns.
unsafe fn sessions<'a>(ctx: *mut c_void) -> &'a dyn Sessions {
	// SAFETY: as the caller guarantees.
	unsafe { *ctx.cast::<&dyn Sessions>() }
}

/// A C string as text; lossy.
fn text(string: *const c_char) -> Option<String> {
	if string.is_null() {
		return None;
	}
	// SAFETY: the server passes NUL-terminated strings, valid for the call.
	Some(
		unsafe { CStr::from_ptr(string) }
			.to_string_lossy()
			.into_owned(),
	)
}

extern "C" fn open(
	ctx: *mut c_void,
	command: *const c_char,
	term: *const c_char,
	cols: c_int,
	rows: c_int,
	in_fd: *mut c_int,
	out_fd: *mut c_int,
	id: *mut c_long,
) -> c_int {
	if ctx.is_null() || in_fd.is_null() || out_fd.is_null() || id.is_null() {
		return -libc::EINVAL;
	}
	let clamp = |n: c_int| u16::try_from(n.clamp(1, i32::from(u16::MAX))).unwrap_or(80);
	// SAFETY: the context `serve` gave, alive during the call.
	let sessions = unsafe { sessions(ctx) };
	match sessions.open(
		text(command).as_deref(),
		text(term).as_deref(),
		(clamp(cols), clamp(rows)),
	) {
		Ok((input, output, session)) => {
			// SAFETY: the server's own `int`s for the descriptors.
			unsafe { in_fd.write(input.into_raw_fd()) };
			// SAFETY: as above.
			unsafe { out_fd.write(output.into_raw_fd()) };
			// SAFETY: the server's own `long`.
			unsafe { id.write(session as c_long) };
			0
		}
		Err(errno) => -errno.abs(),
	}
}

extern "C" fn resize(ctx: *mut c_void, id: c_long, cols: c_int, rows: c_int) {
	if ctx.is_null() {
		return;
	}
	let clamp = |n: c_int| u16::try_from(n.clamp(1, i32::from(u16::MAX))).unwrap_or(80);
	// SAFETY: the context `serve` gave, alive during the call.
	unsafe { sessions(ctx) }.resize(id, (clamp(cols), clamp(rows)));
}

extern "C" fn status(ctx: *mut c_void, id: c_long) -> c_int {
	if ctx.is_null() {
		return -1;
	}
	// SAFETY: the context `serve` gave, alive during the call.
	unsafe { sessions(ctx) }.status(id).unwrap_or(-1)
}

extern "C" fn log(ctx: *mut c_void, line: *const c_char) {
	if ctx.is_null() {
		return;
	}
	if let Some(line) = text(line) {
		// SAFETY: the context `serve` gave, alive during the call.
		unsafe { sessions(ctx) }.log(&line);
	}
}
