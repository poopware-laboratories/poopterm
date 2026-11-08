//! Opening PTYs one at a time, so threads opening them together each get
//! their own.
//!
//! FreeBSD's openpty(3) finds the slave through ptsname(3), which writes
//! `/dev/` and then the device's name into one static buffer for the whole
//! process, and opens the slave from it. Another thread opening a PTY in
//! between rewrites that buffer, so the open gets `/dev/` itself (EISDIR)
//! or the other thread's slave, leaving a master no program writes to.

use std::{
	error::Error,
	sync::{Mutex, PoisonError},
};

use portable_pty::{PtyPair, PtySize, native_pty_system};

/// Held while a PTY is being opened.
static OPENING: Mutex<()> = Mutex::new(());

/// Opens a PTY of `size`, never while another thread opens one.
pub(crate) fn openpty(size: PtySize) -> Result<PtyPair, Box<dyn Error>> {
	one_at_a_time(|| native_pty_system().openpty(size)).map_err(|e| e.into())
}

/// Runs `open` while holding [`OPENING`]. Nothing is left half-done when
/// `open` panics, so a poisoned lock is taken anyway.
fn one_at_a_time<T>(open: impl FnOnce() -> T) -> T {
	let _turn = OPENING.lock().unwrap_or_else(PoisonError::into_inner);
	open()
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
