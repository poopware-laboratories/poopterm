//! The built `poopterm` holds no transparent huge pages on Linux, not even
//! those its allocator would make before `main`, seen from outside while
//! it runs.
#![cfg(target_os = "linux")]
use std::{
	io::Read,
	process::{Child, Command, Stdio},
};

/// `poopterm` writing a workload's bytes into a pipe nobody reads, so it
/// blocks, alive, once the pipe is full; returned once it has written.
fn blocked_writer() -> Child {
	let mut child = Command::new(env!("CARGO_BIN_EXE_poopterm"))
		.args(["--bench-corpus", "log", "--mib", "16"])
		.env_remove("MIMALLOC_ALLOW_THP")
		.stdout(Stdio::piped())
		.stderr(Stdio::null())
		.spawn()
		.expect("poopterm runs");
	// A first byte means `main` is running and its allocator has memory.
	let mut first = [0u8];
	let stdout = child.stdout.as_mut().unwrap();
	stdout.read_exact(&mut first).expect("output");
	child
}

/// The value after `field` in `/proc/<pid>/<file>`, its first word.
fn proc_field(pid: u32, file: &str, field: &str) -> String {
	let text = std::fs::read_to_string(format!("/proc/{pid}/{file}")).unwrap();
	let line = text.lines().find_map(|line| line.strip_prefix(field));
	let line = line.unwrap_or_else(|| panic!("no {field} in {file}: {text}"));
	line.split_whitespace().next().unwrap().to_owned()
}

#[test]
fn the_terminal_runs_without_huge_pages_from_its_first_allocation() {
	let mut child = blocked_writer();
	let pid = child.id();
	let thp = proc_field(pid, "status", "THP_enabled:");
	let huge = proc_field(pid, "smaps_rollup", "AnonHugePages:");
	child.kill().unwrap();
	child.wait().unwrap();
	assert_eq!(thp, "0", "huge pages are off for the process");
	// None at all: those mimalloc touched before `main` would be 2 MiB
	// each, kept for the life of the process.
	assert_eq!(huge, "0", "KiB of huge pages");
}
