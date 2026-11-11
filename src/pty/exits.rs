//! Learning that programs ended, many at a time, from one blocking call:
//! a pidfd per program in an epoll set on Linux (`pidfd_open(2)`,
//! `epoll(7)`), `EVFILT_PROC` with `NOTE_EXIT` in a kqueue on macOS and
//! FreeBSD (`kqueue(2)`). A pipe in the same set lets another thread stop
//! the wait; it is never read, so a stopped set stays stopped.

use super::{device::check, spawn::report_pipe};
use std::{
	io,
	os::fd::{AsRawFd, FromRawFd, OwnedFd},
};

/// The programs being watched for their end.
pub(crate) struct Exits {
	/// The epoll set or kqueue.
	set: OwnedFd,
	/// The pipe that stops [`Self::wait`]: its read end is in `set`.
	stop: (io::PipeReader, io::PipeWriter),
	/// Each watched program's pidfd, closed once it is reported.
	#[cfg(target_os = "linux")]
	pub(super) pidfds: std::sync::Mutex<std::collections::HashMap<libc::pid_t, OwnedFd>>,
}

/// How many ends one wait takes in at most; more come with the next.
const BATCH: usize = 16;

/// The epoll key of the stop pipe; pids are positive.
#[cfg(target_os = "linux")]
const STOP: u64 = u64::MAX;

impl Exits {
	pub(crate) fn new() -> io::Result<Self> {
		let exits = Self {
			set: new_set()?,
			stop: report_pipe()?,
			#[cfg(target_os = "linux")]
			pidfds: Default::default(),
		};
		exits.listen()?;
		Ok(exits)
	}

	/// Blocks until watched programs end and returns them, each once, or
	/// `None` once [`Self::stop`] was called.
	pub(crate) fn wait(&self) -> io::Result<Option<Vec<libc::pid_t>>> {
		loop {
			match self.wait_once()? {
				Some(ended) if ended.is_empty() => {}
				ended => return Ok(ended),
			}
		}
	}

	/// Makes [`Self::wait`] return `None`, now and from then on.
	pub(crate) fn stop(&self) -> io::Result<()> {
		// One byte per stop never fills it.
		io::Write::write_all(&mut &self.stop.1, &[1])
	}
}

/// Calls `call` again while a signal interrupts it.
fn retrying(mut call: impl FnMut() -> libc::c_int) -> io::Result<usize> {
	loop {
		match check(call()) {
			Ok(n) => return Ok(n as usize),
			Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
			Err(error) => return Err(error),
		}
	}
}

#[cfg(target_os = "linux")]
fn new_set() -> io::Result<OwnedFd> {
	// SAFETY: takes only flags; a non-negative result is a new descriptor.
	let fd = check(unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) })?;
	// SAFETY: `fd` was just made and nothing else owns it.
	Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(target_os = "linux")]
impl Exits {
	/// Puts `fd` in the set, keyed by `key`.
	fn register(&self, fd: &impl AsRawFd, key: u64) -> io::Result<()> {
		let set = self.set.as_raw_fd();
		let mut event = libc::epoll_event {
			events: libc::EPOLLIN as u32,
			u64: key,
		};
		// SAFETY: reads one event we own; both descriptors are open.
		check(unsafe { libc::epoll_ctl(set, libc::EPOLL_CTL_ADD, fd.as_raw_fd(), &mut event) })
			.map(drop)
	}

	fn listen(&self) -> io::Result<()> {
		self.register(&self.stop.0, STOP)
	}

	/// Watches `pid`, a child of ours not yet waited for. `false` means it
	/// has already ended, which macOS reports now instead of later.
	pub(crate) fn add(&self, pid: libc::pid_t) -> io::Result<bool> {
		// SAFETY: takes a pid and no flags; a pidfd closes on exec.
		let fd = check(unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as libc::c_int)?;
		// SAFETY: `fd` was just made and nothing else owns it.
		let pidfd = unsafe { OwnedFd::from_raw_fd(fd) };
		let mut pidfds = self
			.pidfds
			.lock()
			.unwrap_or_else(|poison| poison.into_inner());
		self.register(&pidfd, pid as u64)?;
		pidfds.insert(pid, pidfd);
		Ok(true)
	}

	/// One wait: what ended, which another thread may have taken first, or
	/// `None` once stopped.
	fn wait_once(&self) -> io::Result<Option<Vec<libc::pid_t>>> {
		let mut events = [libc::epoll_event { events: 0, u64: 0 }; BATCH];
		// SAFETY: writes at most `BATCH` events into the array we own.
		let ready = retrying(|| unsafe {
			libc::epoll_wait(
				self.set.as_raw_fd(),
				events.as_mut_ptr(),
				BATCH as libc::c_int,
				-1,
			)
		})?;
		let keys: Vec<u64> = events[..ready].iter().map(|event| event.u64).collect();
		if keys.contains(&STOP) {
			return Ok(None);
		}
		let mut pidfds = self
			.pidfds
			.lock()
			.unwrap_or_else(|poison| poison.into_inner());
		// Closing its pidfd takes a program out of the set: reported once.
		let ended = keys
			.into_iter()
			.map(|key| key as libc::pid_t)
			.filter(|pid| pidfds.remove(pid).is_some())
			.collect();
		Ok(Some(ended))
	}
}

#[cfg(not(target_os = "linux"))]
fn new_set() -> io::Result<OwnedFd> {
	// SAFETY: takes nothing.
	let raw = check(unsafe { libc::kqueue() })?;
	// SAFETY: a non-negative result is a new descriptor, ours alone.
	let set = unsafe { OwnedFd::from_raw_fd(raw) };
	// SAFETY: sets a descriptor flag on a descriptor we own.
	check(unsafe { libc::fcntl(set.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
	Ok(set)
}

/// A kevent for `ident` in `filter`, with nothing else set.
#[cfg(not(target_os = "linux"))]
fn kevent(ident: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
	// SAFETY: all zeroes is a valid kevent: no data, a null `udata`.
	let mut event: libc::kevent = unsafe { std::mem::zeroed() };
	event.ident = ident as _;
	event.filter = filter as _;
	event.flags = flags as _;
	event.fflags = fflags as _;
	event
}

#[cfg(not(target_os = "linux"))]
use crate::sys::process::exists;

#[cfg(not(target_os = "linux"))]
impl Exits {
	/// Applies one change to the kqueue.
	fn change(&self, event: &libc::kevent) -> io::Result<()> {
		let (set, none) = (self.set.as_raw_fd(), std::ptr::null_mut());
		// SAFETY: reads one change we own and writes no events.
		check(unsafe { libc::kevent(set, event, 1, none, 0, std::ptr::null()) }).map(drop)
	}

	fn listen(&self) -> io::Result<()> {
		let stop = self.stop.0.as_raw_fd() as usize;
		self.change(&kevent(stop, libc::EVFILT_READ, libc::EV_ADD, 0))
	}

	/// Watches `pid`, a child of ours not yet waited for. `false` means it
	/// has already ended, which macOS reports now instead of later.
	pub(crate) fn add(&self, pid: libc::pid_t) -> io::Result<bool> {
		// Pid 0 is the kernel's own (`kernel_task`, `swapper`) and below it
		// are process groups: none is a program of ours.
		if pid <= 0 {
			return Err(io::Error::from_raw_os_error(libc::ESRCH));
		}
		let flags = libc::EV_ADD | libc::EV_ONESHOT;
		let watch = kevent(pid as usize, libc::EVFILT_PROC, flags, libc::NOTE_EXIT);
		match self.change(&watch) {
			Ok(()) => Ok(true),
			// macOS refuses a program that has exited but is not reaped;
			// unlike one that does not exist, it still takes signal 0.
			Err(error) if error.raw_os_error() == Some(libc::ESRCH) && exists(pid) => Ok(false),
			Err(error) => Err(error),
		}
	}

	/// One wait: what ended, or `None` once stopped.
	fn wait_once(&self) -> io::Result<Option<Vec<libc::pid_t>>> {
		let mut events = [kevent(0, 0, 0, 0); BATCH];
		let (set, out) = (self.set.as_raw_fd(), events.as_mut_ptr());
		// SAFETY: writes at most `BATCH` events into the array we own; a
		// null timeout waits for as long as it takes.
		let ready = retrying(|| unsafe {
			libc::kevent(
				set,
				std::ptr::null(),
				0,
				out,
				BATCH as libc::c_int,
				std::ptr::null(),
			)
		})?;
		let events = &events[..ready];
		if events.iter().any(|event| event.filter == libc::EVFILT_READ) {
			return Ok(None);
		}
		// `EV_ONESHOT` took each out of the queue: reported once.
		let ended = events
			.iter()
			.filter(|event| event.filter == libc::EVFILT_PROC)
			.map(|event| event.ident as libc::pid_t)
			.collect();
		Ok(Some(ended))
	}
}

#[cfg(test)]
#[path = "exits_tests.rs"]
mod tests;
