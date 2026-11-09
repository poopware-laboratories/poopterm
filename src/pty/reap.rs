//! Reaping every shell from one thread instead of one thread per tab: a
//! [`Reaper`] watches its children through [`Exits`], waits for each as it
//! ends and hands its [`Exit`] to the code given with it.

use super::{
	exits::Exits,
	spawn::{Child, Exit},
};
use std::{
	collections::HashMap,
	io,
	sync::{Arc, Mutex, PoisonError},
	thread::{self, JoinHandle},
};

/// What to call with a child's end, on the reaper's thread.
type Then = Box<dyn FnOnce(io::Result<Exit>) + Send>;

/// One thread that reaps every child given to it.
pub(crate) struct Reaper {
	shared: Arc<Shared>,
	thread: Option<JoinHandle<()>>,
}

struct Shared {
	exits: Exits,
	/// The children being watched, by pid, with what to call when each ends.
	watched: Mutex<HashMap<libc::pid_t, (Child, Then)>>,
}

impl Reaper {
	/// Starts the reaper's thread.
	pub(crate) fn new() -> io::Result<Self> {
		let shared = Arc::new(Shared {
			exits: Exits::new()?,
			watched: Mutex::default(),
		});
		let reaping = shared.clone();
		let thread = thread::Builder::new()
			.name("pty-reaper".into())
			.spawn(move || reaping.run())?;
		Ok(Self {
			shared,
			thread: Some(thread),
		})
	}

	/// Reaps `child` when it ends and calls `then` with how, on the
	/// reaper's thread. A child that has already ended is reaped at once,
	/// on this one. Should the system refuse to watch it, a thread of its
	/// own waits instead, so no child is left unreaped.
	pub(crate) fn watch(&self, child: Child, then: impl FnOnce(io::Result<Exit>) + Send + 'static) {
		let pid = child.id() as libc::pid_t;
		// In the map before it is watched, so the reaper finds it however
		// soon it ends.
		self.shared.lock().insert(pid, (child, Box::new(then)));
		let added = self.shared.exits.add(pid);
		if matches!(added, Ok(true)) {
			return;
		}
		let Some(entry) = self.shared.lock().remove(&pid) else {
			return;
		};
		match added {
			Ok(_) => finish(entry),
			Err(_) => drop(thread::spawn(move || finish(entry))),
		}
	}
}

impl Drop for Reaper {
	fn drop(&mut self) {
		// Children still running are left to the system once we exit.
		if self.shared.exits.stop().is_ok()
			&& let Some(thread) = self.thread.take()
		{
			let _ = thread.join();
		}
	}
}

impl Shared {
	fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<libc::pid_t, (Child, Then)>> {
		self.watched.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// The reaper's thread: reaps each child as it ends, until stopped.
	fn run(&self) {
		while let Ok(Some(ended)) = self.exits.wait() {
			for pid in ended {
				let entry = self.lock().remove(&pid);
				if let Some(entry) = entry {
					finish(entry);
				}
			}
		}
	}
}

/// Reaps a child that has ended and says how to its code.
fn finish((mut child, then): (Child, Then)) {
	then(child.wait());
}

#[cfg(test)]
#[path = "reap_tests.rs"]
mod tests;
