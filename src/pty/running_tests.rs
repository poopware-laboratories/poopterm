//! Telling a running program from the shell at its prompt: the decision
//! on plain values, and the foreground group and names read from real
//! terminals and processes.
use super::{
	device::{self, WindowSize},
	running::{UNNAMED, running_program},
	spawn::{self, Command},
};
use crate::sys::process;
use std::{
	fs::File,
	time::{Duration, Instant},
};

fn named(name: &str) -> impl FnOnce(i32) -> Option<String> {
	move |_| Some(name.to_owned())
}

#[test]
fn the_shell_at_its_prompt_is_not_a_running_program() {
	assert_eq!(running_program(Some(42), Some(42), named("zsh")), None);
}

#[test]
fn another_group_in_the_foreground_is_named() {
	assert_eq!(
		running_program(Some(42), Some(77), named("vim")).as_deref(),
		Some("vim")
	);
	let mut asked = None;
	let name = |group| {
		asked = Some(group);
		Some("top".to_owned())
	};
	assert_eq!(
		running_program(Some(42), Some(77), name).as_deref(),
		Some("top")
	);
	assert_eq!(asked, Some(77), "the group leader's name is the one asked");
}

#[test]
fn a_program_without_a_name_is_still_running() {
	assert_eq!(
		running_program(Some(42), Some(77), |_| None).as_deref(),
		Some(UNNAMED)
	);
}

#[test]
fn unknown_shells_or_groups_close_without_asking() {
	assert_eq!(
		running_program(None, Some(77), named("vim")),
		None,
		"shell gone"
	);
	assert_eq!(
		running_program(Some(42), None, named("vim")),
		None,
		"no foreground group"
	);
	// A pid that no group can match is refused rather than wrapped.
	assert_eq!(
		running_program(Some(u32::MAX), Some(-1), named("vim")),
		None
	);
}

#[test]
fn a_terminal_without_a_session_has_no_foreground_group() {
	let pair = device::open(WindowSize::default()).unwrap();
	assert!(device::foreground(&pair.master).is_err());
	let not_a_terminal = File::open("/dev/null").unwrap();
	assert!(device::foreground(&not_a_terminal).is_err());
}

/// Waits up to two seconds for `master`'s foreground group to be other
/// than `shell`.
fn foreground_other_than(master: &impl std::os::fd::AsFd, shell: i32) -> Option<i32> {
	let start = Instant::now();
	while start.elapsed() < Duration::from_secs(2) {
		match device::foreground(master) {
			Ok(group) if group != shell => return Some(group),
			_ => std::thread::sleep(Duration::from_millis(10)),
		}
	}
	None
}

/// An interactive shell (-i), which has job control as at a prompt:
/// each job runs in a group of its own that the shell gives the terminal.
/// `ENV` names the startup file it would read.
fn interactive(pair: &device::Pair, script: &str) -> spawn::Child {
	let shell = Command::new("/bin/sh")
		.env("ENV", "/dev/null")
		.arg("-i")
		.arg("-c")
		.arg(script);
	crate::busy_tests::retry_busy(|| spawn::spawn(&shell, &pair.slave)).unwrap()
}

/// Waits up to five seconds for `master`'s foreground group to be `group`.
fn foreground_becomes(master: &impl std::os::fd::AsFd, group: i32) -> bool {
	let start = Instant::now();
	while start.elapsed() < Duration::from_secs(5) {
		if device::foreground(master).ok() == Some(group) {
			return true;
		}
		std::thread::sleep(Duration::from_millis(10));
	}
	false
}

/// What `running_program` says for `shell` on `pair` now.
fn running(pair: &device::Pair, shell: &spawn::Child) -> Option<String> {
	let foreground = device::foreground(&pair.master).ok();
	running_program(Some(shell.id()), foreground, process::name)
}

fn end(mut shell: spawn::Child) {
	shell.hang_up().unwrap();
	shell.wait().unwrap();
}

const NAMES: bool = cfg!(any(target_os = "linux", target_os = "macos"));

#[test]
fn a_job_the_shell_starts_takes_the_foreground_from_it() {
	let pair = device::open(WindowSize::default()).unwrap();
	let shell = interactive(&pair, "sleep 5; :");
	let pid = shell.id() as i32;
	let group = foreground_other_than(&pair.master, pid).expect("sleep took the terminal");
	assert_ne!(group, pid);
	let name = running(&pair, &shell);
	if NAMES {
		assert_eq!(name.as_deref(), Some("sleep"));
	} else {
		assert!(name.is_some());
	}
	end(shell);
}

#[test]
fn a_finished_job_gives_the_terminal_back_to_the_shell() {
	let pair = device::open(WindowSize::default()).unwrap();
	// `read` is built in, so the shell itself waits for input after it.
	let shell = interactive(&pair, "sleep 0.3; read line");
	let pid = shell.id() as i32;
	assert!(
		foreground_other_than(&pair.master, pid).is_some(),
		"sleep ran first"
	);
	assert!(
		foreground_becomes(&pair.master, pid),
		"the shell has it again"
	);
	assert_eq!(running(&pair, &shell), None, "nothing to ask about");
	end(shell);
}

#[test]
fn a_pipeline_is_named_after_its_first_program() {
	let pair = device::open(WindowSize::default()).unwrap();
	let shell = interactive(&pair, "sleep 5 | cat; :");
	let pid = shell.id() as i32;
	foreground_other_than(&pair.master, pid).expect("the pipeline took the terminal");
	if NAMES {
		assert_eq!(running(&pair, &shell).as_deref(), Some("sleep"));
	}
	end(shell);
}

#[test]
fn our_own_process_has_a_name_and_group_ids_do_not() {
	let ours = process::name(std::process::id() as i32);
	if cfg!(any(target_os = "linux", target_os = "macos")) {
		let ours = ours.expect("our own name");
		assert!(ours.starts_with("poopterm"), "{ours}");
		assert!(!ours.contains('/') && !ours.ends_with('\n'), "{ours:?}");
	}
	assert_eq!(process::name(0), None);
	assert_eq!(process::name(-1), None);
}

#[test]
fn a_reaped_process_has_no_name() {
	let pair = device::open(WindowSize::default()).unwrap();
	let command = Command::new("/bin/sh").arg("-c").arg("exit 0");
	let mut child = crate::busy_tests::retry_busy(|| spawn::spawn(&command, &pair.slave)).unwrap();
	let pid = child.id() as i32;
	child.wait().unwrap();
	assert_eq!(process::name(pid), None);
}

#[test]
fn names_lose_their_directory_and_the_newline_proc_ends_them_with() {
	for (raw, name) in [
		("vim\n", Some("vim")),
		("/usr/bin/vim", Some("vim")),
		("  htop \n", Some("htop")),
		("python3.12", Some("python3.12")),
		("", None),
		("\n", None),
		("/usr/bin/", None),
	] {
		assert_eq!(process::short_name(raw).as_deref(), name, "{raw:?}");
	}
}

#[test]
fn a_shell_without_job_control_runs_everything_in_its_own_group() {
	// `sh -c` runs `sleep` in its own group, the one the terminal follows;
	// so does `exec`, which makes the shell the program. Neither asks, as a
	// shell would not know to stop it either.
	for script in ["sleep 5; :", "exec sleep 5"] {
		let pair = device::open(WindowSize::default()).unwrap();
		let command = Command::new("/bin/sh").arg("-c").arg(script);
		let mut shell = crate::busy_tests::retry_busy(|| spawn::spawn(&command, &pair.slave)).unwrap();
		std::thread::sleep(std::time::Duration::from_millis(100));
		let foreground = device::foreground(&pair.master).ok();
		assert_eq!(foreground, Some(shell.id() as i32), "{script}");
		let running = running_program(Some(shell.id()), foreground, process::name);
		assert_eq!(running, None, "{script}");
		shell.hang_up().unwrap();
		shell.wait().unwrap();
	}
}
