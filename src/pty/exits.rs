//! Learning that programs ended, many at a time, from one blocking call:
//! a pidfd per program in an epoll set on Linux (`pidfd_open(2)`,
//! `epoll(7)`), `EVFILT_PROC` with `NOTE_EXIT` in a kqueue on macOS and
//! FreeBSD (`kqueue(2)`). A pipe in the same set lets another thread stop
//! the wait.

use std::io;

/// The programs being watched for their end.
pub(crate) struct Exits {
	_todo: (),
}

impl Exits {
	pub(crate) fn new() -> io::Result<Self> {
		todo!()
	}

	/// Watches `pid`, a child of ours not yet waited for. `false` means it
	/// has already ended, which macOS reports now instead of later.
	pub(crate) fn add(&self, pid: libc::pid_t) -> io::Result<bool> {
		let _ = pid;
		todo!()
	}

	/// Blocks until watched programs end and returns them, each once, or
	/// `None` once [`Self::stop`] was called.
	pub(crate) fn wait(&self) -> io::Result<Option<Vec<libc::pid_t>>> {
		todo!()
	}

	/// Makes [`Self::wait`] return `None`, now and from then on.
	pub(crate) fn stop(&self) -> io::Result<()> {
		todo!()
	}
}

#[cfg(test)]
#[path = "exits_tests.rs"]
mod tests;
