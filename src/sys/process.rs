//! Other processes: whether one exists, hanging one up, and waiting for a
//! child to end. Process IDs of 0 or below name process groups to `kill`
//! (0 is our own), so they are refused rather than passed on.
use std::io;

/// How a program ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Exit {
	/// It exited with this status.
	Code(i32),
	/// A signal killed it.
	Signal(i32),
}

/// Whether process `pid` exists, a zombie included, and we may signal it;
/// false for IDs of 0 or below.
#[cfg(any(not(target_os = "linux"), test))]
pub(crate) fn exists(pid: i32) -> bool {
	// SAFETY: signal 0 sends nothing; it only checks the pid.
	pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

/// Process `pid`'s command name, as `ps -o comm` shows it without its
/// directory; `None` when it has ended, or the system does not say.
pub(crate) fn name(pid: i32) -> Option<String> {
	command_name(pid).as_deref().and_then(short_name)
}

/// A command name as the system gave it, without a directory or the
/// newline `/proc` ends it with; `None` when nothing is left.
pub(crate) fn short_name(raw: &str) -> Option<String> {
	let name = raw.rsplit('/').next().unwrap_or_default().trim();
	(!name.is_empty()).then(|| name.to_owned())
}

#[cfg(target_os = "linux")]
fn command_name(pid: i32) -> Option<String> {
	std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()
}

#[cfg(target_os = "macos")]
fn command_name(pid: i32) -> Option<String> {
	let mut name = [0u8; 256];
	// SAFETY: writes at most `name.len()` bytes into `name`, returning how
	// many; 0 or less is an error.
	let length = unsafe { libc::proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32) };
	let length = usize::try_from(length).ok().filter(|&n| n > 0)?;
	Some(String::from_utf8_lossy(&name[..length.min(name.len())]).into_owned())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn command_name(_pid: i32) -> Option<String> {
	None
}

/// Sends `SIGHUP` to process `pid`, as a terminal does when it closes.
pub(crate) fn hang_up(pid: i32) -> io::Result<()> {
	if pid <= 0 {
		return Err(io::ErrorKind::InvalidInput.into());
	}
	// SAFETY: sends a signal to one process; takes no memory.
	check(unsafe { libc::kill(pid, libc::SIGHUP) }).map(drop)
}

/// Waits for child `pid` to end, through interruptions, and says how.
pub(crate) fn wait(pid: i32) -> io::Result<Exit> {
	loop {
		match wait_once(pid, 0) {
			Ok(Some(exit)) => return Ok(exit),
			Err(error) if error.kind() != io::ErrorKind::Interrupted => return Err(error),
			// Stopped or interrupted: still running.
			_ => {}
		}
	}
}

/// How child `pid` ended, or None while it runs; never blocks.
pub(crate) fn try_wait(pid: i32) -> io::Result<Option<Exit>> {
	wait_once(pid, libc::WNOHANG)
}

fn wait_once(pid: i32, flags: i32) -> io::Result<Option<Exit>> {
	let mut status = 0;
	// SAFETY: writes the status into the integer we own.
	let reaped = check(unsafe { libc::waitpid(pid, &mut status, flags) })?;
	Ok(if reaped == 0 { None } else { exit_of(status) })
}

/// How a `waitpid` status says the program ended, if it did.
fn exit_of(status: i32) -> Option<Exit> {
	if libc::WIFEXITED(status) {
		Some(Exit::Code(libc::WEXITSTATUS(status)))
	} else if libc::WIFSIGNALED(status) {
		Some(Exit::Signal(libc::WTERMSIG(status)))
	} else {
		None
	}
}

/// A call's result, or the error it left in `errno` when it returned -1.
fn check(result: i32) -> io::Result<i32> {
	if result == -1 {
		Err(io::Error::last_os_error())
	} else {
		Ok(result)
	}
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
