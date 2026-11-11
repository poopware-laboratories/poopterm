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

/// Whether the system's THP mode would give a process huge pages: `always`
/// or `madvise` (mimalloc asks for them), not `never`.
#[cfg(target_os = "linux")]
fn system_gives_huge_pages() -> bool {
	let mode = std::fs::read_to_string("/sys/kernel/mm/transparent_hugepage/enabled");
	mode.is_ok_and(|mode| mode.contains("[always]") || mode.contains("[madvise]"))
}

/// KiB of anonymous huge pages this process holds.
#[cfg(target_os = "linux")]
fn anon_huge_kib() -> usize {
	let rollup = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap();
	let huge = rollup
		.lines()
		.find_map(|line| line.strip_prefix("AnonHugePages:"));
	huge
		.unwrap()
		.trim()
		.trim_end_matches("kB")
		.trim()
		.parse()
		.unwrap()
}

/// Run in its own process by the test below: turns huge pages off when
/// asked, touches 64 MiB, and prints the process's flag, mimalloc's option
/// and the KiB of anonymous huge pages the 64 MiB got.
#[test]
#[ignore = "run by huge_pages_are_off_for_the_terminal_and_its_allocator"]
#[cfg(target_os = "linux")]
fn report_huge_pages() {
	if std::env::var_os("POOPTERM_TEST_NO_THP").is_some() {
		no_huge_pages();
	}
	// Pages made huge before the call stay so: count only the new ones.
	let before = anon_huge_kib();
	let big = vec![1u8; 64 << 20];
	let huge = anon_huge_kib().saturating_sub(before);
	// SAFETY: takes no memory; returns the flag or -1.
	let disabled = unsafe { libc::prctl(libc::PR_GET_THP_DISABLE, 0, 0, 0, 0) };
	println!("disabled={disabled} allow={} huge={huge}", allow_thp());
	drop(big);
}

/// What `report_huge_pages` prints, run without or with `no_huge_pages`.
#[cfg(target_os = "linux")]
fn huge_pages_report(off: bool) -> String {
	let mut command = Command::new(std::env::current_exe().unwrap());
	command.args([
		"--exact",
		"allocator::tests::report_huge_pages",
		"--ignored",
		"--nocapture",
	]);
	command.env_remove("MIMALLOC_ALLOW_THP");
	command.env_remove("POOPTERM_TEST_NO_THP");
	if off {
		command.env("POOPTERM_TEST_NO_THP", "1");
	}
	let output = command.output().unwrap();
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(output.status.success(), "{stdout}");
	let line = stdout.lines().find(|line| line.starts_with("disabled="));
	line
		.unwrap_or_else(|| panic!("no report in {stdout}"))
		.to_owned()
}

#[test]
#[cfg(target_os = "linux")]
fn huge_pages_are_off_for_the_terminal_and_its_allocator() {
	// Off: the kernel gives none, whatever the system's mode, and mimalloc
	// asks for none.
	assert_eq!(huge_pages_report(true), "disabled=1 allow=0 huge=0");
	// By default mimalloc allows them (1 pins the option's index), and the
	// kernel gives them where its mode allows; unless this process
	// inherited the flag from whatever ran the tests.
	let default = huge_pages_report(false);
	let inherited = default.starts_with("disabled=1");
	assert!(
		default.starts_with("disabled=0 allow=1") || inherited,
		"{default}"
	);
	let huge: usize = default.rsplit('=').next().unwrap().parse().unwrap();
	if system_gives_huge_pages() && !inherited {
		assert!(huge >= 2 << 10, "no huge pages by default: {default}");
	}
}
