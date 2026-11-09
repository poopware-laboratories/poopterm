//! Reaping every shell from one thread instead of one thread per tab: a
//! [`Reaper`] watches its children through [`Exits`], waits for each as it
//! ends and hands its [`Exit`] to the code given with it.

use super::{
	exits::Exits,
	spawn::{Child, Exit},
};
use std::io;

/// What to call with a child's end, on the reaper's thread.
type Then = Box<dyn FnOnce(io::Result<Exit>) + Send>;

/// One thread that reaps every child given to it.
pub(crate) struct Reaper {
	_todo: std::sync::Mutex<Option<(Exits, Then)>>,
}

impl Reaper {
	/// Starts the reaper's thread.
	pub(crate) fn new() -> io::Result<Self> {
		todo!()
	}

	/// Reaps `child` when it ends and calls `then` with how. Should the
	/// system refuse to watch it, a thread of its own waits instead, so no
	/// child is left unreaped.
	pub(crate) fn watch(&self, child: Child, then: impl FnOnce(io::Result<Exit>) + Send + 'static) {
		let _ = (child, then);
		todo!()
	}
}

#[cfg(test)]
#[path = "reap_tests.rs"]
mod tests;
