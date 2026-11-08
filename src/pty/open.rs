//! Opening PTYs one at a time, so threads opening them together each get
//! their own.

use std::error::Error;

use portable_pty::{PtyPair, PtySize, native_pty_system};

/// Opens a PTY of `size`.
pub(crate) fn openpty(size: PtySize) -> Result<PtyPair, Box<dyn Error>> {
	one_at_a_time(|| native_pty_system().openpty(size)).map_err(|e| e.into())
}

/// Runs `open`.
fn one_at_a_time<T>(open: impl FnOnce() -> T) -> T {
	open()
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
