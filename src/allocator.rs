//! Allocator tuning: mimalloc options set once at startup.
//!
//! mimalloc keeps freed pages committed for a second and purges them only
//! when the allocator runs again, so an idle terminal held on to everything
//! a flood or a cleared scrollback had used. Purging at once gives it back;
//! the throughput benchmark shows no cost (`docs/performance.md`).

use std::ffi::{c_int, c_long};

/// `mi_option_purge_delay` in the vendored mimalloc 3 `mimalloc.h`; the test
/// pins it by the option's default value.
const PURGE_DELAY: c_int = 15;

// SAFETY: both functions are part of mimalloc's public C API, linked in by
// the `mimalloc` crate that provides the global allocator. They take plain
// integers, accept any option index, and are thread-safe.
unsafe extern "C" {
	safe fn mi_option_set(option: c_int, value: c_long);
	#[cfg(test)]
	safe fn mi_option_get(option: c_int) -> c_long;
}

/// Purges freed memory at once instead of after a delay.
pub(crate) fn purge_eagerly() {
	mi_option_set(PURGE_DELAY, 0);
}

#[cfg(test)]
fn purge_delay() -> c_long {
	mi_option_get(PURGE_DELAY)
}

#[cfg(test)]
#[path = "allocator_tests.rs"]
mod tests;
