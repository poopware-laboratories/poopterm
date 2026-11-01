//! Tests for the allocator tuning: the option really is mimalloc's purge
//! delay, and freed memory leaves the process once it is set.

use super::*;
#[cfg(target_os = "linux")]
use std::process::Command;

#[test]
fn eager_purging_sets_mimalloc_s_purge_delay_to_zero() {
	// Only this test changes the option, so nothing races the first read.
	// mimalloc's default delay is 1000 ms; any other value means the index
	// names a different option (or MIMALLOC_PURGE_DELAY is set).
	if std::env::var_os("MIMALLOC_PURGE_DELAY").is_none() {
		assert_eq!(purge_delay(), 1000);
	}
	purge_eagerly();
	assert_eq!(purge_delay(), 0);
}

/// Resident KiB of this process, from `/proc/self/statm` (pages).
#[cfg(target_os = "linux")]
fn resident_kib() -> usize {
	let statm = std::fs::read_to_string("/proc/self/statm").unwrap();
	let pages: usize = statm.split_whitespace().nth(1).unwrap().parse().unwrap();
	pages * 4
}

/// Run in its own process by the test below: allocates and frees 256 MiB
/// in row-sized blocks, then prints the KiB it still holds over its start.
#[test]
#[ignore = "run by freed_rows_leave_the_process_only_with_eager_purging"]
#[cfg(target_os = "linux")]
fn hold_after_free() {
	if std::env::var_os("POOPTERM_TEST_PURGE").is_some() {
		purge_eagerly();
	}
	let start = resident_kib();
	let rows: Vec<Vec<u8>> = (0..4096).map(|i| vec![i as u8 | 1; 64 << 10]).collect();
	assert!(
		resident_kib() > start + (200 << 10),
		"the rows were never touched"
	);
	drop(rows);
	println!("held={}", resident_kib().saturating_sub(start));
}

#[cfg(target_os = "linux")]
fn held_kib(eager: bool) -> usize {
	let mut command = Command::new(std::env::current_exe().unwrap());
	command.args([
		"--exact",
		"allocator::tests::hold_after_free",
		"--ignored",
		"--nocapture",
	]);
	command.env_remove("MIMALLOC_PURGE_DELAY");
	if eager {
		command.env("POOPTERM_TEST_PURGE", "1");
	}
	let output = command.output().unwrap();
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(output.status.success(), "{stdout}");
	let held = stdout.lines().find_map(|line| line.strip_prefix("held="));
	held
		.unwrap_or_else(|| panic!("no report in {stdout}"))
		.parse()
		.unwrap()
}

#[test]
#[cfg(target_os = "linux")]
fn freed_rows_leave_the_process_only_with_eager_purging() {
	// The default keeps the freed pages committed for reuse: this is the
	// memory a cleared scrollback never gave back.
	let lazy = held_kib(false);
	assert!(lazy > 128 << 10, "default mimalloc held only {lazy} KiB");
	let eager = held_kib(true);
	assert!(eager < 16 << 10, "eager purging still held {eager} KiB");
}
