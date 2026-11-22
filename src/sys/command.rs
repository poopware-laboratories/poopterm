//! Calling a command library's entry: a real program (uname, ls, cat…)
//! built as a shared library for the in-process shell, where iOS lets no
//! other process run. The entry, `poopterm_entry` in
//! `ios/commands/poopterm_cmd.c`, points the standard descriptors at the
//! ones given for the run, sets the environment, calls the program's
//! `main` and returns its status, `exit` included.
use std::{
	ffi::{CString, c_char, c_int, c_void},
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
}

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

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
