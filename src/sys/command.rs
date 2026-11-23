//! Calling a command library's entry: a real program (uname, ls, cat…)
//! built as a shared library for the in-process shell, where iOS lets no
//! other process run. The entry, `poopterm_entry` in
//! `ios/commands/poopterm_cmd.c`, points the standard descriptors at the
//! ones given for the run, sets the environment, calls the program's
//! `main` and returns its status, `exit` included. A command that starts
//! another (xargs, find -exec) does so through `spawn`, which runs it the
//! same way on the descriptors it is given.
use std::{
	ffi::{CStr, CString, c_char, c_int, c_void},
	os::fd::{AsRawFd, BorrowedFd},
};

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
}

/// The `spawn` member's type.
type Spawn = extern "C" fn(*const c_char, *const *mut c_char, *const Tty) -> c_int;

/// Darwin's errno values `spawn` answers with.
const ENOENT: c_int = 2;
const ENOEXEC: c_int = 8;

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
}

/// Runs the command in `library` with `argv` and `env` (`NAME=value`) and
/// returns its exit status, or None if the library has no entry.
pub(crate) fn call(
	library: &Library,
	argv: &[CString],
	env: &[CString],
	stdio: &Stdio,
) -> Option<i32> {
	let symbol = library.symbol(c"poopterm_entry")?;
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
	let tty = Tty {
		input: stdio.input.as_raw_fd(),
		output: stdio.output.as_raw_fd(),
		error: stdio.error.as_raw_fd(),
		is_tty: c_int::from(stdio.tty.is_some()),
		cols,
		rows,
		envp: env_pointers.as_ptr(),
		spawn: Some(spawn),
	};
	let argc = c_int::try_from(args.len()).ok()?;
	// SAFETY: every command library exports `poopterm_entry` with this type
	// (`poopterm_cmd.h`), and a function pointer has a data pointer's size.
	let entry = unsafe { std::mem::transmute::<*mut c_void, Entry>(symbol.as_ptr()) };
	// SAFETY: `argv` and `envp` are null-terminated arrays of NUL-terminated
	// strings we own, writable as `main` may need, and with `tty` they live
	// through the call; the descriptors are borrowed and open throughout, and
	// the entry puts back the standard ones before it returns.
	Some(unsafe { entry(argc, argv_pointers.as_ptr(), &tty) })
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
	let Ok(name) = CString::new(found.as_os_str().as_encoded_bytes()) else {
		return -ENOENT;
	};
	let Ok(library) = Library::open(&name) else {
		return -ENOEXEC;
	};
	call(&library, argv, env, stdio).map_or(-ENOEXEC, |status| status & 0xff)
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
	let stdio = Stdio {
		input,
		output,
		error,
		tty: (tty.is_tty != 0).then_some((tty.cols, tty.rows)),
	};
	run_nested(file, &argv, &env, &stdio)
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
