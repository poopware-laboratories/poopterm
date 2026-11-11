//! Trying again while the system has no PTY free. macOS answers ENXIO
//! when all of them (`kern.tty.ptmx_max`, 511) are taken, which a burst of
//! tabs, or of tests, can reach; one is free again as soon as the last
//! program holding a closed one's slave has gone.
use std::{io, thread, time::Duration};

/// Tries before opening a PTY gives up, `PAUSE` apart: two seconds in all.
pub(crate) const TRIES: usize = 40;
pub(crate) const PAUSE: Duration = Duration::from_millis(50);

/// `attempt`'s result, tried up to `tries` times `pause` apart while it
/// fails with ENXIO; any other result is returned at once.
pub(crate) fn when_free<T>(
	tries: usize,
	pause: Duration,
	mut attempt: impl FnMut() -> io::Result<T>,
) -> io::Result<T> {
	for _ in 1..tries {
		match attempt() {
			Err(error) if error.raw_os_error() == Some(libc::ENXIO) => thread::sleep(pause),
			result => return result,
		}
	}
	attempt()
}

#[cfg(test)]
#[path = "retry_tests.rs"]
mod tests;
