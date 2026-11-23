//! Calling a command library's entry: a real program (uname, ls, cat…)
//! built as a shared library for the in-process shell, where iOS lets no
//! other process run. The entry, `poopterm_entry` in
//! `ios/commands/poopterm_cmd.c`, points the standard descriptors at the
//! ones given for the run, sets the environment, calls the program's
//! `main` and returns its status, `exit` included. A command that starts
//! another (xargs, find -exec) does so through `spawn`, which runs it the
//! same way on the descriptors it is given.
use std::{
	ffi::{CStr, CString, c_char, c_int, c_uint, c_void},
	os::fd::{AsRawFd, BorrowedFd},
};

#[cfg(test)]
use super::dl::Library;

/// `struct poopterm_tty` of `poopterm_cmd.h`.
#[repr(C)]
struct Tty {
	input: c_int,
	output: c_int,
	error: c_int,
	is_tty: c_int,
	cols: u16,
	rows: u16,
	envp: *const *mut c_char,
	spawn: Option<Spawn>,
	/// The terminal `control` asks: a `*const &dyn Device`, or null.
	context: *const c_void,
	control: Option<Control>,
	/// Bit `n` set: descriptor `n` (0 to 2) is the terminal.
	tty_fds: c_uint,
	/// Where the keyboard reads, for `/dev/tty`, or -1.
	keyboard: c_int,
}

/// The `spawn` member's type.
type Spawn = extern "C" fn(*const c_char, *const *mut c_char, *const Tty) -> c_int;

/// The `control` member's type: an operation of `poopterm_cmd.h`'s
/// `POOPTERM_TTY_*` on the terminal `context` is, with its argument.
type Control = extern "C" fn(*const c_void, c_int, *mut c_void) -> c_int;

/// `POOPTERM_TTY_*`: read the settings into a `termios`, set them from
/// one (now, after output drains, or after dropping typed input), and
/// read the window size into a `winsize`.
pub(crate) const GET_SETTINGS: c_int = 1;
pub(crate) const SET_SETTINGS: c_int = 2;
pub(crate) const SET_SETTINGS_DRAIN: c_int = 3;
pub(crate) const SET_SETTINGS_FLUSH: c_int = 4;
pub(crate) const GET_SIZE: c_int = 5;

/// A terminal a command's descriptors may be (the in-process device's):
/// its `termios` settings, which programs read and set, and its size.
pub(crate) trait Device: Sync {
	fn settings(&self) -> libc::termios;
	/// Sets them; `flush` drops what was typed and not read yet.
	fn set_settings(&self, settings: &libc::termios, flush: bool);
	fn size(&self) -> (u16, u16);
}

/// Darwin's errno values `spawn` and `control` answer with.
const ENOENT: c_int = 2;
const ENOEXEC: c_int = 8;
const EINVAL: c_int = 22;
const ENOTTY: c_int = 25;

/// `poopterm_entry`'s type.
type Entry = unsafe extern "C" fn(c_int, *const *mut c_char, *const Tty) -> c_int;

/// Where a command's standard descriptors point while it runs, and the
/// terminal it is told it has.
pub(crate) struct Stdio<'a> {
	pub input: BorrowedFd<'a>,
	pub output: BorrowedFd<'a>,
	pub error: BorrowedFd<'a>,
	/// Answer `isatty` and the window size as a terminal of this many
	/// columns and rows, or say these are no terminal.
	pub tty: Option<(u16, u16)>,
	/// The terminal that answers for the settings and live size.
	pub device: Option<&'a dyn Device>,
	/// Standard input is the terminal's keyboard.
	pub keyboard: bool,
	/// The keyboard, which `/dev/tty` opens even when standard input is a
	/// pipe (`ls | less`).
	pub keys: Option<BorrowedFd<'a>>,
}

impl Stdio<'_> {
	/// Which standard descriptors are the terminal, as `Tty::tty_fds`.
	fn tty_fds(&self) -> c_uint {
		let output = if self.tty.is_some() { 0b110 } else { 0 };
		output | c_uint::from(self.keyboard)
	}
}

/// Runs the command in `library` with `argv` and `env` (`NAME=value`) and
/// returns its exit status, or None if the library has no entry.
#[cfg(test)]
pub(crate) fn call(
	library: &Library,
	argv: &[CString],
	env: &[CString],
	stdio: &Stdio,
) -> Option<i32> {
	let symbol = library.symbol(c"poopterm_entry")?;
	call_entry(symbol, argv, env, stdio)
}

/// As `call`, with the entry already found (in a library our own loader
/// loaded, `jit::loader`).
pub(crate) fn call_entry(
	symbol: std::ptr::NonNull<c_void>,
	argv: &[CString],
	env: &[CString],
	stdio: &Stdio,
) -> Option<i32> {
	// A program's `main` may change its arguments (getopt reorders them), so
	// each gets a buffer of its own, and the arrays end in null as C's do.
	let mut args: Vec<Vec<u8>> = argv
		.iter()
		.map(|arg| arg.as_bytes_with_nul().to_vec())
		.collect();
	let mut vars: Vec<Vec<u8>> = env
		.iter()
		.map(|var| var.as_bytes_with_nul().to_vec())
		.collect();
	let pointers = |strings: &mut [Vec<u8>]| -> Vec<*mut c_char> {
		strings
			.iter_mut()
			.map(|string| string.as_mut_ptr().cast::<c_char>())
			.chain([std::ptr::null_mut()])
			.collect()
	};
	let (argv_pointers, env_pointers) = (pointers(&mut args), pointers(&mut vars));
	let (cols, rows) = stdio.tty.unwrap_or((0, 0));
	// `control` gets a thin pointer to the fat one, alive through the call.
	let device = stdio.device;
	let tty = Tty {
		input: stdio.input.as_raw_fd(),
		output: stdio.output.as_raw_fd(),
		error: stdio.error.as_raw_fd(),
		is_tty: c_int::from(stdio.tty.is_some()),
		cols,
		rows,
		envp: env_pointers.as_ptr(),
		spawn: Some(spawn),
		context: device
			.as_ref()
			.map_or(std::ptr::null(), |device| std::ptr::from_ref(device).cast()),
		control: device.is_some().then_some(control as Control),
		tty_fds: stdio.tty_fds(),
		keyboard: stdio.keys.map_or(-1, |keys| keys.as_raw_fd()),
	};
	let argc = c_int::try_from(args.len()).ok()?;
	// What `spawn` hands a command this one starts: read from here, since a
	// library built before these fields passes back a shorter `Tty`.
	let outer = OUTER.replace(Outer {
		context: tty.context,
		keyboard: stdio.keyboard,
	});
	// SAFETY: every command library exports `poopterm_entry` with this type
	// (`poopterm_cmd.h`), and a function pointer has a data pointer's size.
	let entry = unsafe { std::mem::transmute::<*mut c_void, Entry>(symbol.as_ptr()) };
	// SAFETY: `argv` and `envp` are null-terminated arrays of NUL-terminated
	// strings we own, writable as `main` may need, and with `tty` they live
	// through the call; the descriptors are borrowed and open throughout, and
	// the entry puts back the standard ones before it returns.
	let status = unsafe { entry(argc, argv_pointers.as_ptr(), &tty) };
	OUTER.set(outer);
	Some(status)
}

/// The terminal of the command running on this thread, for those it starts.
#[derive(Clone, Copy)]
struct Outer {
	/// `Tty::context`: a `*const &dyn Device`, or null.
	context: *const c_void,
	keyboard: bool,
}

thread_local! {
	static OUTER: std::cell::Cell<Outer> = const {
		std::cell::Cell::new(Outer {
			context: std::ptr::null(),
			keyboard: false,
		})
	};
}

/// The `$PATH` of an environment of `NAME=value` strings.
fn search_path(env: &[CString]) -> Option<&str> {
	env
		.iter()
		.rev()
		.find_map(|var| var.to_bytes().strip_prefix(b"PATH="))
		.and_then(|path| std::str::from_utf8(path).ok())
}

/// Runs the command `file` names, from inside another, with `argv`, `env`
/// and `stdio`: its status (the low byte, as a process's), or minus
/// ENOENT when there is none and ENOEXEC when it is no command.
pub(super) fn run_nested(file: &str, argv: &[CString], env: &[CString], stdio: &Stdio) -> c_int {
	let cwd = std::env::current_dir().unwrap_or_default();
	let Some(found) = crate::inproc::native::find(file, search_path(env), &cwd) else {
		return -ENOENT;
	};
	let Ok(opened) = crate::inproc::native::open(&found) else {
		return -ENOEXEC;
	};
	let Some(entry) = opened.entry() else {
		return -ENOEXEC;
	};
	call_entry(entry, argv, env, stdio).map_or(-ENOEXEC, |status| status & 0xff)
}

/// NUL-terminated strings from a null-terminated array.
///
/// # Safety
///
/// `list` is null or a null-terminated array of NUL-terminated strings.
unsafe fn strings_of(list: *const *mut c_char) -> Vec<CString> {
	let mut strings = Vec::new();
	if list.is_null() {
		return strings;
	}
	for index in 0.. {
		// SAFETY: the array goes on up to and including its null entry, so
		// `index` is inside it.
		let at = unsafe { list.add(index) };
		// SAFETY: `at` points at one of the array's entries.
		let item = unsafe { *at };
		if item.is_null() {
			break;
		}
		// SAFETY: each entry is a NUL-terminated string.
		strings.push(unsafe { CStr::from_ptr(item) }.to_owned());
	}
	strings
}

/// The shim's `spawn`: a command asked to start `file`.
extern "C" fn spawn(file: *const c_char, argv: *const *mut c_char, tty: *const Tty) -> c_int {
	if file.is_null() || tty.is_null() {
		return -ENOENT;
	}
	// SAFETY: the shim passes a NUL-terminated name.
	let Ok(file) = unsafe { CStr::from_ptr(file) }.to_str() else {
		return -ENOENT;
	};
	// SAFETY: the shim passes its own `tty`, alive through the call.
	let tty = unsafe { &*tty };
	// SAFETY: `argv` and `envp` are C's null-terminated string arrays.
	let argv = unsafe { strings_of(argv) };
	// SAFETY: as above.
	let env = unsafe { strings_of(tty.envp) };
	// SAFETY: the descriptors are the calling command's, open while it
	// waits for this one.
	let input = unsafe { BorrowedFd::borrow_raw(tty.input) };
	// SAFETY: as above.
	let output = unsafe { BorrowedFd::borrow_raw(tty.output) };
	// SAFETY: as above.
	let error = unsafe { BorrowedFd::borrow_raw(tty.error) };
	let outer = OUTER.get();
	let device = match outer.context.is_null() {
		true => None,
		// SAFETY: a non-null context is the `&dyn Device` that `call` points
		// at, on the stack of the outer call, which is still running.
		false => Some(unsafe { *outer.context.cast::<&dyn Device>() }),
	};
	let stdio = Stdio {
		input,
		output,
		error,
		tty: (tty.is_tty != 0).then_some((tty.cols, tty.rows)),
		device,
		// The child reads the keyboard if it kept its parent's input.
		keyboard: outer.keyboard && tty.input == 0,
		keys: None,
	};
	run_nested(file, &argv, &env, &stdio)
}

/// The shim's `control`: a command reads or sets the terminal's settings,
/// or reads its size. 0, or minus an errno.
extern "C" fn control(context: *const c_void, op: c_int, arg: *mut c_void) -> c_int {
	if context.is_null() || arg.is_null() {
		return -EINVAL;
	}
	// SAFETY: the shim passes back the context `call` gave it, a pointer to
	// a `&dyn Device` that outlives the command.
	let device = unsafe { *context.cast::<&dyn Device>() };
	match op {
		GET_SETTINGS => {
			// SAFETY: for this operation the shim passes a `struct termios`
			// it owns, writable, which `libc::termios` mirrors.
			unsafe { arg.cast::<libc::termios>().write(device.settings()) };
			0
		}
		SET_SETTINGS | SET_SETTINGS_DRAIN | SET_SETTINGS_FLUSH => {
			// SAFETY: as above, the `termios` to set, read only.
			let settings = unsafe { arg.cast::<libc::termios>().read() };
			device.set_settings(&settings, op == SET_SETTINGS_FLUSH);
			0
		}
		GET_SIZE => {
			let (cols, rows) = device.size();
			let size = libc::winsize {
				ws_row: rows,
				ws_col: cols,
				ws_xpixel: 0,
				ws_ypixel: 0,
			};
			// SAFETY: for this operation the shim passes a `struct winsize`.
			unsafe { arg.cast::<libc::winsize>().write(size) };
			0
		}
		_ => -ENOTTY,
	}
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
