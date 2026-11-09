//! How a tab starts its shell, seen from the shell: a script stands in for
//! it and prints what it was given.
use super::*;
use crate::pty::{
	device::{self, WindowSize},
	spawn::{self, Exit},
};
use std::{ffi::OsString, io::Read, os::unix::fs::PermissionsExt, sync::mpsc, time::Duration};

/// A stand-in shell named `name` in a directory of its own: it prints its
/// `$0`, arguments, directory and the terminal's variables, one per line.
fn fake_shell(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("poopterm-shell-{}-{name}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let shell = dir.join(name);
	let script = concat!(
		"#!/bin/sh\n",
		"echo \"0=$0\"\n",
		"echo \"args=$*\"\n",
		"echo \"cwd=$(pwd -P)\"\n",
		"echo \"TERM=$TERM\"\n",
		"echo \"COLORTERM=$COLORTERM\"\n",
		"echo \"TERM_PROGRAM=$TERM_PROGRAM\"\n",
		"echo \"TERM_PROGRAM_VERSION=$TERM_PROGRAM_VERSION\"\n",
		"echo \"EXTRA=$POOPTERM_TEST_EXTRA\"\n",
		"sleep 0.2\n",
	);
	std::fs::write(&shell, script).unwrap();
	std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
	shell
}

fn term_env(term: &str, extra: &[(&str, &str)]) -> TermEnv {
	TermEnv {
		term: term.into(),
		extra: extra
			.iter()
			.map(|(name, value)| (name.to_string(), OsString::from(value)))
			.collect(),
	}
}

/// Runs `shell` as a tab would, in `cwd`, and returns what it printed.
fn run(shell: &Path, env: &TermEnv, cwd: &Path) -> Vec<String> {
	let pair = device::open(WindowSize::default()).unwrap();
	let command = shell_command(shell.to_str().unwrap(), env, cwd);
	let mut child = spawn::spawn(&command, &pair.slave).unwrap();
	drop(pair.slave);
	let mut screen = File::from(pair.master);
	let (send, shown) = mpsc::channel();
	thread::spawn(move || {
		let mut buf = [0; 4096];
		while let Ok(n @ 1..) = screen.read(&mut buf) {
			if send.send(buf[..n].to_vec()).is_err() {
				return;
			}
		}
	});
	assert_eq!(child.wait().unwrap(), Exit::Code(0));
	let mut output = Vec::new();
	while let Ok(bytes) = shown.recv_timeout(Duration::from_millis(300)) {
		output.extend_from_slice(&bytes);
	}
	String::from_utf8_lossy(&output)
		.lines()
		.map(|line| line.trim_end_matches('\r').to_owned())
		.collect()
}

/// The value printed for `key`, as `key=value`.
fn value<'a>(lines: &'a [String], key: &str) -> &'a str {
	let prefix = format!("{key}=");
	lines
		.iter()
		.find_map(|line| line.strip_prefix(&prefix))
		.unwrap_or_else(|| panic!("no {key} in {lines:?}"))
}

fn here() -> PathBuf {
	std::env::temp_dir().canonicalize().unwrap()
}

#[test]
fn the_shell_learns_the_terminal_it_runs_in() {
	let shell = fake_shell("sh");
	let lines = run(&shell, &term_env("xterm-poopterm", &[]), &here());
	assert_eq!(value(&lines, "TERM"), "xterm-poopterm");
	assert_eq!(value(&lines, "COLORTERM"), "truecolor");
	assert_eq!(value(&lines, "TERM_PROGRAM"), "poopterm");
	assert_eq!(
		value(&lines, "TERM_PROGRAM_VERSION"),
		env!("CARGO_PKG_VERSION")
	);
}

#[test]
fn the_terms_own_variables_reach_the_shell() {
	let shell = fake_shell("extra");
	let env = term_env(
		"xterm-256color",
		&[("POOPTERM_TEST_EXTRA", "integration on")],
	);
	let lines = run(&shell, &env, &here());
	assert_eq!(value(&lines, "EXTRA"), "integration on");
}

#[test]
fn the_shell_starts_in_the_tabs_directory() {
	let shell = fake_shell("dir");
	let dir = std::env::current_dir().unwrap().canonicalize().unwrap();
	let lines = run(&shell, &term_env("xterm", &[]), &dir);
	assert_eq!(value(&lines, "cwd"), dir.to_str().unwrap());
}

#[test]
fn the_shell_is_named_by_its_path() {
	let shell = fake_shell("named");
	let lines = run(&shell, &term_env("xterm", &[]), &here());
	assert_eq!(value(&lines, "0"), shell.to_str().unwrap());
}

#[test]
fn a_shell_without_integration_starts_as_a_login_shell() {
	for name in ["zsh", "fish", "sh"] {
		let shell = fake_shell(name);
		let lines = run(&shell, &term_env("xterm", &[]), &here());
		assert_eq!(value(&lines, "args"), "-l", "{name}");
	}
}

#[test]
fn bash_with_integration_reads_its_rcfile_interactively() {
	let shell = fake_shell("bash");
	let env = term_env("xterm", &[("POOPTERM_BASH_RCFILE", "/tmp/poopterm-rc")]);
	let lines = run(&shell, &env, &here());
	assert_eq!(value(&lines, "args"), "--rcfile /tmp/poopterm-rc -i");
}

#[test]
fn bash_without_its_rcfile_starts_as_a_login_shell() {
	let shell = fake_shell("bash");
	let lines = run(&shell, &term_env("xterm", &[("OTHER", "x")]), &here());
	assert_eq!(value(&lines, "args"), "-l");
}

#[test]
fn a_shell_handed_to_the_reaper_leaves_no_zombie() {
	let pair = device::open(WindowSize::default()).unwrap();
	let child = spawn::spawn(
		&spawn::Command::new("sh").arg("-c").arg("exit 0"),
		&pair.slave,
	)
	.unwrap();
	let pid = child.id() as libc::pid_t;
	reap::reap_later(child);
	assert!(gone_soon(pid), "{pid} is still a zombie");
}

/// Whether `pid` stops existing, reaped, within five seconds.
fn gone_soon(pid: libc::pid_t) -> bool {
	(0..500).any(|_| {
		thread::sleep(Duration::from_millis(10));
		// SAFETY: signal 0 sends nothing; a reaped pid no longer exists.
		let alive = unsafe { libc::kill(pid, 0) } == 0;
		!alive
	})
}

#[test]
fn many_shells_handed_to_the_reaper_all_go() {
	let pairs: Vec<_> = (0..10)
		.map(|_| device::open(WindowSize::default()).unwrap())
		.collect();
	let mut pids = Vec::new();
	for pair in &pairs {
		let command = spawn::Command::new("sh").arg("-c").arg("sleep 0.05");
		let child = spawn::spawn(&command, &pair.slave).unwrap();
		pids.push(child.id() as libc::pid_t);
		reap::reap_later(child);
	}
	for pid in pids {
		assert!(gone_soon(pid), "{pid} is still a zombie");
	}
}
