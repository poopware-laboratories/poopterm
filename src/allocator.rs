//! Allocator tuning: mimalloc options set once at startup.
//!
//! mimalloc keeps freed pages committed for a second and purges them only
//! when the allocator runs again, so an idle terminal held on to everything
//! a flood or a cleared scrollback had used. Purging at once gives it back;
//! the throughput benchmark shows no cost (`docs/performance.md`).
//!
//! mimalloc also asks for transparent huge pages, which on Linux kept
//! 15 to 30 MiB more resident, half used, for about 1% of throughput:
//! they are turned off for the terminal (`docs/performance.md`).
// Calls into C or the kernel; docs/unsafe.md lists each block.
#![allow(unsafe_code)]

use std::ffi::{c_int, c_long};

/// `mi_option_purge_delay` in the vendored mimalloc 3 `mimalloc.h`; the test
/// pins it by the option's default value.
const PURGE_DELAY: c_int = 15;
/// `mi_option_allow_thp`, pinned the same way.
#[cfg(target_os = "linux")]
const ALLOW_THP: c_int = 43;

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

/// Turns off transparent huge pages for this process: mimalloc stops
/// asking for them, and the kernel gives none on its own. The shell's
/// child turns them back on (`pty::spawn`). Linux only; called as the
/// program loads, by `NO_HUGE_PAGES_FIRST`.
#[cfg(target_os = "linux")]
pub(crate) fn no_huge_pages() {
	mi_option_set(ALLOW_THP, 0);
	// SAFETY: takes plain integers and only sets a flag of this process.
	unsafe { libc::prctl(libc::PR_SET_THP_DISABLE, 1, 0, 0, 0) };
}

/// Calls [`no_huge_pages`] as the program loads, before mimalloc's own
/// constructor (priority 101), which reads the option once, and before the
/// Rust runtime allocates: huge pages touched before `main` would stay for
/// the life of the process (`tests/huge_pages.rs`). Not in tests, which
/// check both ways.
#[cfg(all(target_os = "linux", not(test)))]
#[used]
// SAFETY: a plain function pointer in `.init_array`, which the loader
// calls once, single-threaded, before `main`; the linker orders sections
// of lower priority numbers first.
#[unsafe(link_section = ".init_array.00100")]
static NO_HUGE_PAGES_FIRST: extern "C" fn() = {
	extern "C" fn first() {
		no_huge_pages();
	}
	first
};

#[cfg(all(test, target_os = "linux"))]
fn allow_thp() -> c_long {
	mi_option_get(ALLOW_THP)
}

#[cfg(test)]
fn purge_delay() -> c_long {
	mi_option_get(PURGE_DELAY)
}

#[cfg(test)]
#[path = "allocator_tests.rs"]
mod tests;
