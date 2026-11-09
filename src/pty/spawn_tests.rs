//! Programs started on our own PTYs, seen from inside: what `sh` reports
//! about its terminal, session, name, environment and signals.
use super::*;
use crate::pty::device::{self, WindowSize};
use std::{fs::File, io::Read, sync::mpsc, thread, time::Duration};

/// Runs `sh -c script` on a new 80×24 PTY and returns what it printed,
/// read until it exits, and how it ended.
fn run(command: Command) -> (String, Exit) {
	let pair = device::open(WindowSize {
		cols: 80,
		rows: 24,
		..WindowSize::default()
	})
	.unwrap();
	let mut child = spawn(&command, &pair.slave).unwrap();
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
	let exit = child.wait().unwrap();
	let mut output = Vec::new();
	// What the program wrote is read while it runs and just after: macOS
	// and FreeBSD drop what is left once its last descriptor closes.
	while let Ok(bytes) = shown.recv_timeout(Duration::from_millis(300)) {
		output.extend_from_slice(&bytes);
	}
	(String::from_utf8_lossy(&output).replace("\r\n", "\n"), exit)
}

fn sh(script: &str) -> Command {
	// Keep the shell alive briefly, so its output is read before it exits.
	Command::new("sh")
		.arg("-c")
		.arg(format!("{script}; sleep 0.2"))
}

#[test]
fn the_pty_is_the_programs_controlling_terminal() {
	// /dev/tty opens only for a process with a controlling terminal.
	let (output, exit) = run(sh("echo through-dev-tty >/dev/tty"));
	assert_eq!(exit, Exit::Code(0));
	assert_eq!(output, "through-dev-tty\n");
}

#[test]
fn the_program_leads_a_session_of_its_own() {
	let (output, _) = run(sh("ps -o sess= -o pid= -p $$"));
	let ids: Vec<&str> = output.split_whitespace().collect();
	assert_eq!(ids.len(), 2, "{output:?}");
	assert_eq!(ids[0], ids[1], "the session's id is its leader's pid");
}

#[test]
fn standard_streams_are_the_terminal() {
	let (output, _) = run(sh("test -t 0 && test -t 1 && test -t 2 && echo all-three"));
	assert_eq!(output, "all-three\n");
}

#[test]
fn the_program_sees_the_terminals_size() {
	let (output, _) = run(sh("stty size"));
	assert_eq!(output, "24 80\n");
}

#[test]
fn arguments_arrive_as_given() {
	let script = r#"printf '[%s]' "$@"; sleep 0.2"#;
	let command = Command::new("sh")
		.arg("-c")
		.arg(script)
		.arg("zero")
		.arg("a b")
		.arg("$HOME")
		.arg("");
	let (output, _) = run(command);
	assert_eq!(output, "[a b][$HOME][]");
}

#[test]
fn argv0_can_name_a_login_shell() {
	let command = Command::new("/bin/sh")
		.arg0("-sh")
		.arg("-c")
		.arg("ps -o args= -p $$; sleep 0.2");
	let (output, _) = run(command);
	assert!(output.starts_with("-sh -c"), "{output:?}");
}

#[test]
fn the_environment_is_ours_with_changes() {
	let command = sh(r#"echo "$POOPTERM_SPAWN_TEST|$HOME""#).env("POOPTERM_SPAWN_TEST", "set here");
	let (output, _) = run(command);
	let home = std::env::var("HOME").unwrap_or_default();
	assert_eq!(output, format!("set here|{home}\n"));
}

#[test]
fn a_later_value_replaces_an_earlier_one() {
	let command = sh(r#"env | grep -c '^POOPTERM_TWICE='; echo "$POOPTERM_TWICE""#)
		.env("POOPTERM_TWICE", "first")
		.env("POOPTERM_TWICE", "second");
	let (output, _) = run(command);
	assert_eq!(output, "1\nsecond\n");
}

#[test]
fn the_program_starts_in_the_directory_asked() {
	let dir = std::env::temp_dir().canonicalize().unwrap();
	let (output, _) = run(sh("pwd -P").cwd(&dir));
	assert_eq!(output.trim_end(), dir.to_str().unwrap());
}

#[test]
fn exit_codes_and_signals_come_back() {
	assert_eq!(
		run(Command::new("sh").arg("-c").arg("exit 7")).1,
		Exit::Code(7)
	);
	assert_eq!(
		run(Command::new("sh").arg("-c").arg("kill -9 $$")).1,
		Exit::Signal(9)
	);
}

#[test]
fn a_program_that_cannot_start_is_an_error_not_a_child() {
	let pair = device::open(WindowSize::default()).unwrap();
	let missing = Command::new("/nonexistent/poopterm-no-such-shell");
	let error = spawn(&missing, &pair.slave).err().unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::ENOENT));
	let not_executable = Command::new("Cargo.toml");
	let error = spawn(&not_executable, &pair.slave).err().unwrap();
	assert!(error.raw_os_error().is_some(), "{error}");
}

#[test]
fn a_directory_that_does_not_exist_is_an_error() {
	let pair = device::open(WindowSize::default()).unwrap();
	let command = Command::new("sh").cwd("/nonexistent/poopterm-no-such-dir");
	let error = spawn(&command, &pair.slave).err().unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::ENOENT));
}

#[test]
fn hanging_up_ends_a_shell_waiting_for_input() {
	let pair = device::open(WindowSize::default()).unwrap();
	let mut child = spawn(&Command::new("sh").arg("-c").arg("read line"), &pair.slave).unwrap();
	thread::sleep(Duration::from_millis(100));
	child.hang_up().unwrap();
	assert_eq!(child.wait().unwrap(), Exit::Signal(libc::SIGHUP));
	drop(pair);
}

#[test]
fn the_childs_id_is_the_programs_pid() {
	let pair = device::open(WindowSize::default()).unwrap();
	let script = Command::new("sh").arg("-c").arg("exit $(($$ % 200))");
	let mut child = spawn(&script, &pair.slave).unwrap();
	let expected = Exit::Code((child.id() % 200) as i32);
	assert_eq!(child.wait().unwrap(), expected);
}

#[cfg(target_os = "linux")]
#[test]
fn signals_are_neither_ignored_nor_blocked() {
	// Rust ignores SIGPIPE in us; a shell must not inherit that, nor a mask.
	let (output, _) = run(sh("grep -E '^Sig(Ign|Blk)' /proc/self/status"));
	for line in output.lines() {
		let mask = line.split_whitespace().nth(1).unwrap();
		assert!(mask.chars().all(|digit| digit == '0'), "{line}");
	}
	assert_eq!(output.lines().count(), 2, "{output:?}");
}

#[cfg(target_os = "linux")]
#[test]
fn none_of_our_descriptors_reach_the_program() {
	// One we would leak if nothing closed it: inheritable, unlike std's.
	let file = File::open("Cargo.toml").unwrap();
	// SAFETY: duplicates a descriptor we own, without close-on-exec.
	let leaky = unsafe { libc::dup(std::os::fd::AsRawFd::as_raw_fd(&file)) };
	assert!(leaky > 2);
	let (output, _) = run(sh("ls /proc/self/fd"));
	// SAFETY: closes the duplicate made above.
	unsafe { libc::close(leaky) };
	let fds: Vec<&str> = output.split_whitespace().collect();
	// 0, 1, 2, and the directory ls reads.
	assert!(fds.len() <= 4, "{fds:?}");
	assert!(!fds.contains(&leaky.to_string().as_str()), "{fds:?}");
}

fn executable_in(dirs: &'static [&'static str]) -> impl Fn(&Path) -> bool {
	move |path| dirs.iter().any(|dir| path.parent() == Some(Path::new(dir)))
}

#[test]
fn a_program_is_found_in_the_first_directory_of_path_holding_it() {
	let path = OsStr::new("/a:/b:/c");
	let found = resolve(OsStr::new("zsh"), Some(path), executable_in(&["/b", "/c"]));
	assert_eq!(found, Some(PathBuf::from("/b/zsh")));
}

#[test]
fn a_program_with_a_slash_is_taken_as_it_is() {
	let found = resolve(OsStr::new("./bin/zsh"), Some(OsStr::new("/a")), |_| false);
	assert_eq!(found, Some(PathBuf::from("./bin/zsh")));
}

#[test]
fn a_program_nowhere_in_path_is_not_found() {
	assert_eq!(
		resolve(OsStr::new("zsh"), Some(OsStr::new("/a:/b")), |_| false),
		None
	);
	assert_eq!(resolve(OsStr::new("zsh"), None, |_| true), None);
	assert_eq!(
		resolve(OsStr::new(""), Some(OsStr::new("/a")), |_| true),
		None
	);
}

#[test]
fn an_empty_path_entry_is_the_current_directory() {
	let found = resolve(
		OsStr::new("zsh"),
		Some(OsStr::new("/a::/b")),
		executable_in(&["."]),
	);
	assert_eq!(found, Some(PathBuf::from("./zsh")));
}
