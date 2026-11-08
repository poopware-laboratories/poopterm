//! Starting a program on a PTY, from POSIX `fork` and `exec`: in a session
//! of its own with the PTY as its controlling terminal and on its standard
//! streams, with default signal handling and no descriptor of ours, and
//! its `exec` failure reported to us instead of a silent exit.

use std::{
	ffi::{OsStr, OsString},
	io,
	os::fd::OwnedFd,
	path::{Path, PathBuf},
};

/// What to start: a program, its arguments, environment and directory.
pub(crate) struct Command {
	program: OsString,
	arg0: OsString,
	args: Vec<OsString>,
	env: Vec<(OsString, OsString)>,
	cwd: Option<PathBuf>,
}

impl Command {
	/// `program`, found in `PATH` unless it has a `/`, named `program` in
	/// its `argv[0]`, with our environment and directory.
	pub(crate) fn new(program: impl AsRef<OsStr>) -> Self {
		let _ = program;
		todo!()
	}

	/// The name the program gets as `argv[0]`, such as `-zsh` for a login
	/// shell.
	pub(crate) fn arg0(self, name: impl AsRef<OsStr>) -> Self {
		let _ = name;
		todo!()
	}

	pub(crate) fn arg(self, arg: impl AsRef<OsStr>) -> Self {
		let _ = arg;
		todo!()
	}

	/// Sets `name` to `value` in the program's environment.
	pub(crate) fn env(self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
		let _ = (name, value);
		todo!()
	}

	pub(crate) fn cwd(self, dir: impl AsRef<Path>) -> Self {
		let _ = dir;
		todo!()
	}
}

/// How a program ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exit {
	/// It exited with this status.
	Code(i32),
	/// A signal killed it.
	Signal(i32),
}

/// A started program, not yet waited for.
pub(crate) struct Child {
	pid: libc::pid_t,
}

impl Child {
	pub(crate) fn id(&self) -> u32 {
		self.pid as u32
	}

	/// Waits for the program to end.
	pub(crate) fn wait(&mut self) -> io::Result<Exit> {
		todo!()
	}

	/// Sends `SIGHUP`, as a terminal does when it closes.
	pub(crate) fn hang_up(&self) -> io::Result<()> {
		todo!()
	}
}

/// Where `program` is: itself when it has a `/`, else the first directory
/// of `path` (as in `PATH`) holding an executable of that name.
pub(crate) fn resolve(
	program: &OsStr,
	path: Option<&OsStr>,
	executable: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
	let _ = (program, path, executable);
	todo!()
}

/// Starts `command` on the terminal whose slave is `terminal`.
pub(crate) fn spawn(command: &Command, terminal: &OwnedFd) -> io::Result<Child> {
	let _ = (command, terminal);
	todo!()
}

#[cfg(test)]
#[path = "spawn_tests.rs"]
mod tests;
