//! Starting a program on a PTY, from POSIX `fork` and `exec`: in a session
//! of its own with the PTY as its controlling terminal and on its standard
//! streams, with default signal handling and no descriptor of ours, and
//! its `exec` failure reported to us instead of a silent exit.
//!
//! Everything the child needs is allocated before `fork`; after it, the
//! child makes only async-signal-safe calls, as POSIX requires of a child
//! of a threaded process. A close-on-exec pipe carries `errno` back if any
//! step fails, so reading nothing from it means `exec` succeeded.

use std::{
	ffi::{CString, OsStr, OsString},
	fs, io,
	os::{
		fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
		unix::{ffi::OsStrExt, fs::PermissionsExt},
	},
	path::{Path, PathBuf},
	ptr,
};

use super::device::check;

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
		let program = program.as_ref().to_os_string();
		Self {
			arg0: program.clone(),
			program,
			args: Vec::new(),
			env: Vec::new(),
			cwd: None,
		}
	}

	/// The name the program gets as `argv[0]`, such as `-zsh` for a login
	/// shell.
	pub(crate) fn arg0(mut self, name: impl AsRef<OsStr>) -> Self {
		self.arg0 = name.as_ref().to_os_string();
		self
	}

	pub(crate) fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
		self.args.push(arg.as_ref().to_os_string());
		self
	}

	/// Sets `name` to `value` in the program's environment.
	pub(crate) fn env(mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
		self
			.env
			.push((name.as_ref().to_os_string(), value.as_ref().to_os_string()));
		self
	}

	pub(crate) fn cwd(mut self, dir: impl AsRef<Path>) -> Self {
		self.cwd = Some(dir.as_ref().to_path_buf());
		self
	}

	/// Our environment with this command's changes, later ones winning.
	fn environment(&self) -> Vec<(OsString, OsString)> {
		let mut vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
		for (name, value) in &self.env {
			vars.retain(|(other, _)| other != name);
			vars.push((name.clone(), value.clone()));
		}
		vars
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
		wait_for(self.pid)
	}

	/// Sends `SIGHUP`, as a terminal does when it closes.
	pub(crate) fn hang_up(&self) -> io::Result<()> {
		// SAFETY: sends a signal to the process we started; takes no memory.
		check(unsafe { libc::kill(self.pid, libc::SIGHUP) }).map(drop)
	}
}

/// Where `program` is: itself when it has a `/`, else the first directory
/// of `path` (as in `PATH`) holding an executable of that name.
pub(crate) fn resolve(
	program: &OsStr,
	path: Option<&OsStr>,
	executable: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
	if program.is_empty() {
		return None;
	}
	if program.as_bytes().contains(&b'/') {
		return Some(PathBuf::from(program));
	}
	path?
		.as_bytes()
		.split(|&byte| byte == b':')
		.map(|dir| match dir {
			b"" => Path::new("."),
			dir => Path::new(OsStr::from_bytes(dir)),
		})
		.map(|dir| dir.join(program))
		.find(|candidate| executable(candidate))
}

/// Starts `command` on the terminal whose slave is `terminal`.
pub(crate) fn spawn(command: &Command, terminal: &OwnedFd) -> io::Result<Child> {
	let prepared = Prepared::new(command)?;
	let (report_read, report_write) = report_pipe()?;
	// SAFETY: the child runs only `Prepared::start`, which makes
	// async-signal-safe calls on memory allocated before the fork.
	let pid = check(unsafe { libc::fork() })?;
	if pid == 0 {
		// SAFETY: we are the child of the fork above.
		unsafe { prepared.start(terminal.as_raw_fd(), report_write.as_raw_fd()) }
	}
	drop(report_write);
	if let Err(error) = read_report(&report_read) {
		// The child exits with 127 after reporting; reap it.
		let _ = wait_for(pid);
		return Err(error);
	}
	Ok(Child { pid })
}

/// Signals numbered up to this one get their default handling back.
#[cfg(target_os = "linux")]
const LAST_SIGNAL: libc::c_int = 64;
#[cfg(target_os = "freebsd")]
const LAST_SIGNAL: libc::c_int = 128;
#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
const LAST_SIGNAL: libc::c_int = 31;

/// `TIOCSCTTY` from macOS's `<sys/ttycom.h>`: `_IO('t', 97)`, which the libc
/// crate does not export there.
#[cfg(target_os = "macos")]
const TIOCSCTTY: libc::c_ulong = 0x2000_7461;
#[cfg(not(target_os = "macos"))]
const TIOCSCTTY: libc::c_ulong = libc::TIOCSCTTY as libc::c_ulong;

/// Above this many, descriptors are not marked one by one (where the kernel
/// cannot mark them all at once).
const MOST_DESCRIPTORS: libc::c_int = 1 << 16;

/// Everything the child uses, built before the fork.
struct Prepared {
	path: CString,
	cwd: Option<CString>,
	/// Owns the strings `argv` and `envp` point into.
	_strings: Vec<CString>,
	argv: Vec<*const libc::c_char>,
	envp: Vec<*const libc::c_char>,
	no_signals: libc::sigset_t,
	default_action: libc::sigaction,
	descriptors: libc::c_int,
}

impl Prepared {
	fn new(command: &Command) -> io::Result<Self> {
		let vars = command.environment();
		let search = vars
			.iter()
			.find(|(name, _)| name == "PATH")
			.map(|(_, value)| value.as_os_str());
		let path = resolve(&command.program, search, is_executable)
			.ok_or_else(|| io::Error::from_raw_os_error(libc::ENOENT))?;
		// Found from our directory, so made absolute before the child's
		// `chdir` can change what a relative path means.
		let path = std::path::absolute(path)?;
		let argv: Vec<CString> = std::iter::once(&command.arg0)
			.chain(&command.args)
			.map(|arg| c_string(arg.as_bytes()))
			.collect::<io::Result<_>>()?;
		let envp: Vec<CString> = vars
			.iter()
			.map(|(name, value)| {
				let mut entry = name.as_bytes().to_vec();
				entry.push(b'=');
				entry.extend_from_slice(value.as_bytes());
				c_string(&entry)
			})
			.collect::<io::Result<_>>()?;
		let pointers = |strings: &[CString]| {
			strings
				.iter()
				.map(|string| string.as_ptr())
				.chain(std::iter::once(ptr::null()))
				.collect()
		};
		// SAFETY: both are plain C structs for which all zeroes is valid,
		// and all zeroes in `sa_sigaction` is `SIG_DFL`.
		let (mut no_signals, default_action) = unsafe {
			(
				std::mem::zeroed::<libc::sigset_t>(),
				std::mem::zeroed::<libc::sigaction>(),
			)
		};
		// SAFETY: initializes the set we own.
		unsafe { libc::sigemptyset(&mut no_signals) };
		// SAFETY: takes only a constant name.
		let open_max = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
		Ok(Self {
			path: c_string(path.as_os_str().as_bytes())?,
			cwd: command
				.cwd
				.as_ref()
				.map(|dir| c_string(dir.as_os_str().as_bytes()))
				.transpose()?,
			argv: pointers(&argv),
			envp: pointers(&envp),
			_strings: argv.into_iter().chain(envp).collect(),
			no_signals,
			default_action,
			descriptors: descriptor_limit(open_max),
		})
	}

	/// Becomes the program, or reports why not on `report` and exits.
	///
	/// # Safety
	///
	/// Only in the child of a fork, where it must be the only thing run.
	unsafe fn start(&self, terminal: RawFd, report: RawFd) -> ! {
		// SAFETY: every call below is async-signal-safe and uses only
		// descriptors we hold and memory `self` owns.
		unsafe {
			let errno = self.become_program(terminal);
			let bytes = errno.to_ne_bytes();
			libc::write(report, bytes.as_ptr().cast(), bytes.len());
			libc::_exit(127)
		}
	}

	/// Each step of the child's setup; returns `errno` if one fails.
	///
	/// # Safety
	///
	/// As [`Self::start`].
	unsafe fn become_program(&self, terminal: RawFd) -> libc::c_int {
		let failed = || {
			io::Error::last_os_error()
				.raw_os_error()
				.unwrap_or(libc::EIO)
		};
		// SAFETY: see `start`.
		unsafe {
			if libc::setsid() < 0 {
				return failed();
			}
			if libc::ioctl(terminal, TIOCSCTTY as _, 0) < 0 {
				return failed();
			}
			// The terminal is above 2: Rust's runtime opens 0, 1 and 2 at
			// startup if they are closed, so a new descriptor never takes
			// them, and `dup2` clears close-on-exec on each copy.
			for stream in 0..3 {
				if libc::dup2(terminal, stream) < 0 {
					return failed();
				}
			}
			self.close_ours_on_exec();
			libc::sigprocmask(libc::SIG_SETMASK, &self.no_signals, ptr::null_mut());
			for signal in 1..=LAST_SIGNAL {
				// SIGKILL, SIGSTOP and unused numbers fail; that is fine.
				libc::sigaction(signal, &self.default_action, ptr::null_mut());
			}
			if let Some(dir) = &self.cwd
				&& libc::chdir(dir.as_ptr()) < 0
			{
				return failed();
			}
			libc::execve(self.path.as_ptr(), self.argv.as_ptr(), self.envp.as_ptr());
			failed()
		}
	}

	/// Marks every descriptor above 2 close-on-exec, all at once where the
	/// kernel can.
	///
	/// # Safety
	///
	/// As [`Self::start`].
	unsafe fn close_ours_on_exec(&self) {
		// SAFETY: see `start`; these take only numbers.
		unsafe {
			#[cfg(target_os = "linux")]
			if libc::syscall(
				libc::SYS_close_range,
				3,
				libc::c_uint::MAX,
				libc::CLOSE_RANGE_CLOEXEC,
			) == 0
			{
				return;
			}
			#[cfg(target_os = "freebsd")]
			if libc::close_range(
				3,
				libc::c_uint::MAX,
				libc::CLOSE_RANGE_CLOEXEC as libc::c_int,
			) == 0
			{
				return;
			}
		}
		mark_close_on_exec(3..self.descriptors);
	}
}

/// Marks each descriptor in `fds` close-on-exec, one by one; numbers that
/// are not open are skipped. Async-signal-safe.
fn mark_close_on_exec(fds: std::ops::Range<RawFd>) {
	for fd in fds {
		// SAFETY: sets a descriptor flag and takes no memory; a number that
		// is not open fails with `EBADF`, which is fine.
		unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
	}
}

/// How many descriptors to mark one by one, from `sysconf(_SC_OPEN_MAX)`,
/// which is -1 when there is no limit.
fn descriptor_limit(open_max: libc::c_long) -> libc::c_int {
	if open_max < 0 {
		MOST_DESCRIPTORS
	} else {
		open_max.clamp(3, MOST_DESCRIPTORS.into()) as libc::c_int
	}
}

fn c_string(bytes: &[u8]) -> io::Result<CString> {
	CString::new(bytes).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))
}

fn is_executable(path: &Path) -> bool {
	fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// A pipe whose ends both close on exec.
fn report_pipe() -> io::Result<(OwnedFd, OwnedFd)> {
	let mut fds = [0; 2];
	#[cfg(any(target_os = "linux", target_os = "freebsd"))]
	// SAFETY: writes two descriptors into the array we own.
	check(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) })?;
	#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
	// SAFETY: as above.
	check(unsafe { libc::pipe(fds.as_mut_ptr()) })?;
	// SAFETY: both were just opened and nothing else owns them.
	let ends = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
	#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
	for end in [&ends.0, &ends.1] {
		// SAFETY: sets a descriptor flag on a descriptor we own.
		check(unsafe { libc::fcntl(end.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
	}
	Ok(ends)
}

/// Nothing once `exec` closed the pipe, else the error the child sent.
fn read_report(pipe: &OwnedFd) -> io::Result<()> {
	let mut bytes = [0u8; 4];
	loop {
		// SAFETY: reads into the array we own, at most its length.
		let n = unsafe { libc::read(pipe.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
		match n {
			0 => return Ok(()),
			4 => return Err(io::Error::from_raw_os_error(i32::from_ne_bytes(bytes))),
			n if n < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted => {}
			n if n < 0 => return Err(io::Error::last_os_error()),
			_ => return Err(io::Error::from(io::ErrorKind::InvalidData)),
		}
	}
}

/// Waits for `pid` to end and says how.
fn wait_for(pid: libc::pid_t) -> io::Result<Exit> {
	let mut status = 0;
	loop {
		// SAFETY: writes the status into the integer we own.
		if unsafe { libc::waitpid(pid, &mut status, 0) } < 0 {
			let error = io::Error::last_os_error();
			if error.kind() == io::ErrorKind::Interrupted {
				continue;
			}
			return Err(error);
		}
		if libc::WIFEXITED(status) {
			return Ok(Exit::Code(libc::WEXITSTATUS(status)));
		}
		if libc::WIFSIGNALED(status) {
			return Ok(Exit::Signal(libc::WTERMSIG(status)));
		}
	}
}

#[cfg(test)]
#[path = "spawn_setup_tests.rs"]
mod setup_tests;
#[cfg(test)]
#[path = "spawn_tests.rs"]
mod tests;
