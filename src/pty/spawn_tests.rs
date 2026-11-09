//! Programs started on our own PTYs, seen from inside: what `sh` reports
//! about its terminal, session, name, environment and signals.
use super::*;
use crate::pty::device::{self, WindowSize};
use std::{
	fs::File,
	io::{Read, Write},
	os::fd::AsRawFd,
	sync::mpsc,
	thread,
	time::Duration,
};

/// A new 80×24 PTY.
fn pty() -> device::Pair {
	device::open(WindowSize {
		cols: 80,
		rows: 24,
		..WindowSize::default()
	})
	.unwrap()
}

/// Runs `command` on a new 80×24 PTY and returns what it printed, read
/// until it exits, and how it ended.
fn run(command: Command) -> (String, Exit) {
	run_on(pty(), command)
}

/// What appears on `master`, read on a thread of its own.
fn watch(master: OwnedFd) -> mpsc::Receiver<Vec<u8>> {
	let mut screen = File::from(master);
	let (send, shown) = mpsc::channel();
	thread::spawn(move || {
		let mut buf = [0; 4096];
		while let Ok(n @ 1..) = screen.read(&mut buf) {
			if send.send(buf[..n].to_vec()).is_err() {
				return;
			}
		}
	});
	shown
}

/// Reads from `shown` until `text` has appeared, and returns all of it.
fn until(shown: &mpsc::Receiver<Vec<u8>>, text: &str) -> String {
	let mut seen = Vec::new();
	while !String::from_utf8_lossy(&seen).contains(text) {
		let bytes = shown
			.recv_timeout(Duration::from_secs(5))
			.unwrap_or_else(|_| panic!("no {text:?} in {:?}", String::from_utf8_lossy(&seen)));
		seen.extend_from_slice(&bytes);
	}
	String::from_utf8_lossy(&seen).into_owned()
}

/// Starts a shell that waits for a line on `pair`, for tests that look at
/// it from outside while it runs.
fn waiting(pair: &device::Pair) -> Child {
	spawn(&Command::new("sh").arg("-c").arg("read line"), &pair.slave).unwrap()
}

/// Runs `command` on `pair`, as [`run`] does.
fn run_on(pair: device::Pair, command: Command) -> (String, Exit) {
	let mut child = spawn(&command, &pair.slave).unwrap();
	drop(pair.slave);
	let shown = watch(pair.master);
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
	let pair = pty();
	let mut child = waiting(&pair);
	let pid = child.id() as libc::pid_t;
	// SAFETY: takes only a process id.
	let session = unsafe { libc::getsid(pid) };
	assert_eq!(session, pid, "the session's id is its leader's pid");
	// SAFETY: as above.
	assert_ne!(unsafe { libc::getsid(0) }, pid, "and it is not ours");
	// SAFETY: takes only a process id.
	let group = unsafe { libc::getpgid(pid) };
	assert_eq!(group, pid, "it leads its process group too");
	child.hang_up().unwrap();
	child.wait().unwrap();
}

#[test]
fn a_terminal_whose_session_ended_is_reusable_only_on_linux() {
	let pair = pty();
	let mut first = waiting(&pair);
	first.hang_up().unwrap();
	first.wait().unwrap();
	// Linux leaves the terminal free for a new session. FreeBSD and macOS
	// revoke it when its session leader exits, so the slave we hold is dead
	// and every new program needs a new pair.
	#[cfg(target_os = "linux")]
	{
		let (output, exit) = run_on(pair, sh("echo again >/dev/tty"));
		assert_eq!((output.as_str(), exit), ("again\n", Exit::Code(0)));
	}
	#[cfg(not(target_os = "linux"))]
	{
		// The revoked descriptor is no tty on FreeBSD, and no descriptor at
		// all on macOS.
		#[cfg(target_os = "freebsd")]
		let revoked = libc::ENOTTY;
		#[cfg(not(target_os = "freebsd"))]
		let revoked = libc::EBADF;
		let error = spawn(&sh("true"), &pair.slave).err().unwrap();
		assert_eq!(error.raw_os_error(), Some(revoked));
	}
}

#[test]
fn one_command_starts_as_many_programs_as_asked() {
	let command = Command::new("sh").arg("-c").arg("exit 5");
	let (one, two) = (pty(), pty());
	let mut first = spawn(&command, &one.slave).unwrap();
	let mut second = spawn(&command, &two.slave).unwrap();
	assert_ne!(first.id(), second.id());
	assert_eq!(first.wait().unwrap(), Exit::Code(5));
	assert_eq!(second.wait().unwrap(), Exit::Code(5));
}

#[test]
fn a_stopped_program_has_not_ended() {
	let pair = pty();
	let mut child = waiting(&pair);
	let pid = child.id() as libc::pid_t;
	// SAFETY: signals only the child we started.
	assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
	let resume = thread::spawn(move || {
		thread::sleep(Duration::from_millis(100));
		// SAFETY: as above; the child is not reaped until `wait` returns.
		unsafe { libc::kill(pid, libc::SIGCONT) };
		// SAFETY: as above.
		unsafe { libc::kill(pid, libc::SIGTERM) };
	});
	assert_eq!(child.wait().unwrap(), Exit::Signal(libc::SIGTERM));
	resume.join().unwrap();
}

#[test]
fn arguments_past_the_systems_limit_fail_with_e2big() {
	// 256 arguments just under Linux's 128 KiB each make 32 MiB, past
	// ARG_MAX on every system we build for.
	let big = "x".repeat(128_000);
	let mut command = Command::new("/bin/sh");
	for _ in 0..256 {
		command = command.arg(&big);
	}
	let pair = device::open(WindowSize::default()).unwrap();
	let error = spawn(&command, &pair.slave).err().unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::E2BIG));
}

#[test]
fn control_d_typed_on_the_master_ends_the_input() {
	assert_eq!(typed_once_ready(&pty(), "exec cat", b"\x04"), Exit::Code(0));
}

#[test]
fn hanging_up_only_signals_so_a_program_may_ignore_it() {
	let pair = pty();
	let script = "trap '' HUP; echo ready; read line; exit 9";
	let mut child = spawn(&Command::new("sh").arg("-c").arg(script), &pair.slave).unwrap();
	let shown = watch(pair.master.try_clone().unwrap());
	until(&shown, "ready");
	child.hang_up().unwrap();
	// Still there to read the line, then exits as it chose to.
	let mut master = File::from(pair.master.try_clone().unwrap());
	master.write_all(b"done\n").unwrap();
	assert_eq!(child.wait().unwrap(), Exit::Code(9));
}

#[test]
fn the_program_can_change_its_terminals_settings() {
	let script = "stty -echo && stty | grep -c -- -echo";
	let (output, exit) = run(sh(script));
	assert_eq!((output.as_str(), exit), ("1\n", Exit::Code(0)));
}

/// A new directory holding `script`, an executable named `name`.
fn script_dir(name: &str, script: &str) -> PathBuf {
	use std::os::unix::fs::PermissionsExt;
	let dir = std::env::temp_dir().join(format!("poopterm-run-{}-{name}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join(name), script).unwrap();
	let mode = std::fs::Permissions::from_mode(0o755);
	std::fs::set_permissions(dir.join(name), mode).unwrap();
	dir
}

#[test]
fn a_script_in_the_commands_path_runs_with_its_arguments() {
	let dir = script_dir("poopterm-greet", "#!/bin/sh\necho \"hi $1\"; sleep 0.2\n");
	let search = format!("{}:/bin:/usr/bin", dir.display());
	let command = Command::new("poopterm-greet")
		.arg("there")
		.env("PATH", search);
	let (output, exit) = run(command);
	std::fs::remove_dir_all(dir).unwrap();
	assert_eq!((output.as_str(), exit), ("hi there\n", Exit::Code(0)));
}

#[test]
fn a_relative_program_is_ours_even_in_another_directory() {
	let dir = script_dir("poopterm-ours", "#!/bin/sh\necho ours; sleep 0.2\n");
	// The script named from our directory; from `src`, one level deeper,
	// the same `..`s would end somewhere else.
	let ours = std::env::current_dir().unwrap();
	let up: PathBuf = ours.components().skip(1).map(|_| "..").collect();
	let program = up
		.join(dir.strip_prefix("/").unwrap())
		.join("poopterm-ours");
	let (output, exit) = run(Command::new(program).cwd("src"));
	std::fs::remove_dir_all(dir).unwrap();
	assert_eq!((output.as_str(), exit), ("ours\n", Exit::Code(0)));
}

#[test]
fn a_long_value_arrives_whole() {
	let value = "x".repeat(100_000);
	let command = sh(r#"echo "${#POOPTERM_LONG}""#).env("POOPTERM_LONG", &value);
	assert_eq!(run(command).0, "100000\n");
}

#[cfg(target_os = "linux")]
#[test]
fn all_three_streams_are_the_slave_we_gave() {
	let pair = pty();
	let path = pair.path.to_str().unwrap().to_owned();
	let script = "for fd in 0 1 2; do readlink /proc/$$/fd/$fd; done";
	let (output, _) = run_on(pair, sh(script));
	assert_eq!(output, format!("{path}\n{path}\n{path}\n"));
}

#[test]
fn the_program_is_the_terminals_foreground_process_group() {
	let pair = pty();
	let mut child = waiting(&pair);
	// SAFETY: asks the master we own; takes no memory.
	let foreground = unsafe { libc::tcgetpgrp(pair.master.as_raw_fd()) };
	assert_eq!(foreground, child.id() as libc::pid_t);
	child.hang_up().unwrap();
	child.wait().unwrap();
}

#[test]
fn the_controlling_terminal_is_the_slave_we_gave() {
	let pair = pty();
	let path = pair.path.clone();
	let (output, _) = run_on(pair, sh("tty"));
	assert_eq!(output.trim_end(), path.to_str().unwrap());
}

#[test]
fn a_line_typed_on_the_master_reaches_the_program() {
	let pair = pty();
	// The line waits in the terminal's input queue until the shell reads it.
	File::from(pair.master.try_clone().unwrap())
		.write_all(b"typed\n")
		.unwrap();
	let (output, exit) = run_on(pair, sh(r#"read line; echo "got:$line""#));
	assert_eq!(exit, Exit::Code(0));
	assert!(output.ends_with("got:typed\n"), "{output:?}");
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
	// busybox ps (Alpine) has no -p, so Linux asks /proc.
	#[cfg(target_os = "linux")]
	let script = r"tr '\0' ' ' </proc/$$/cmdline; echo; sleep 0.2";
	#[cfg(not(target_os = "linux"))]
	let script = "ps -o args= -p $$; sleep 0.2";
	let command = Command::new("/bin/sh").arg0("-sh").arg("-c").arg(script);
	let (output, _) = run(command);
	assert!(output.starts_with("-sh -c"), "{output:?}");
}

#[test]
fn bytes_that_are_not_utf8_arrive_unchanged() {
	use std::os::unix::ffi::OsStringExt;
	let command = Command::new("sh")
		.arg("-c")
		.arg(r#"printf %s "$1" | od -An -tx1; sleep 0.2"#)
		.arg("zero")
		.arg(OsString::from_vec(b"\xff\xfe".to_vec()));
	let (output, _) = run(command);
	assert_eq!(output.split_whitespace().collect::<Vec<_>>(), ["ff", "fe"]);
}

#[test]
fn many_arguments_all_arrive() {
	let mut command = Command::new("sh")
		.arg("-c")
		.arg(r#"echo "$#"; sleep 0.2"#)
		.arg("zero");
	for n in 0..2000 {
		command = command.arg(n.to_string());
	}
	assert_eq!(run(command).0, "2000\n");
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
	for (script, exit) in [
		("exit 0", Exit::Code(0)),
		("exit 7", Exit::Code(7)),
		("exit 255", Exit::Code(255)),
		("kill -9 $$", Exit::Signal(libc::SIGKILL)),
		("kill -TERM $$", Exit::Signal(libc::SIGTERM)),
	] {
		let (_, ended) = run(Command::new("sh").arg("-c").arg(script));
		assert_eq!(ended, exit, "{script}");
	}
}

/// Why `spawn` refused `program`, given by path.
fn exec_error(program: &Path) -> Option<i32> {
	let pair = device::open(WindowSize::default()).unwrap();
	spawn(&Command::new(program), &pair.slave)
		.err()?
		.raw_os_error()
}

#[test]
fn exec_errors_are_the_ones_execve_gave() {
	use std::os::unix::fs::PermissionsExt;
	let dir = std::env::temp_dir().join(format!("poopterm-exec-{}", std::process::id()));
	std::fs::create_dir_all(&dir).unwrap();
	let plain = dir.join("plain");
	let garbage = dir.join("garbage");
	std::fs::write(&plain, "#!/bin/sh\n").unwrap();
	std::fs::write(&garbage, [0x7f, b'E', b'L', b'F', 0, 1, 2, 3]).unwrap();
	std::fs::set_permissions(&garbage, std::fs::Permissions::from_mode(0o755)).unwrap();
	assert_eq!(exec_error(&dir), Some(libc::EACCES), "a directory");
	assert_eq!(exec_error(&plain), Some(libc::EACCES), "no x bit");
	assert_eq!(exec_error(&garbage), Some(libc::ENOEXEC), "not a program");
	std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_directory_that_is_a_file_is_an_error() {
	let pair = device::open(WindowSize::default()).unwrap();
	let command = Command::new("sh").cwd("Cargo.toml");
	let error = spawn(&command, &pair.slave).err().unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::ENOTDIR));
}

#[test]
fn a_terminal_that_is_not_a_tty_is_an_error() {
	let file = OwnedFd::from(File::open("Cargo.toml").unwrap());
	let error = spawn(&Command::new("sh"), &file).err().unwrap();
	assert_eq!(error.raw_os_error(), Some(libc::ENOTTY));
}

#[test]
fn a_terminal_already_controlling_a_session_is_refused() {
	let pair = pty();
	let mut first = waiting(&pair);
	let second = spawn(&Command::new("sh").arg("-c").arg("true"), &pair.slave);
	assert_eq!(
		second.err().and_then(|error| error.raw_os_error()),
		Some(libc::EPERM)
	);
	first.hang_up().unwrap();
	first.wait().unwrap();
}

#[test]
fn signals_we_ignore_are_not_ignored_in_the_program() {
	// Rust ignores SIGPIPE in us. A shell keeps signals ignored at entry
	// ignored, so this exits 0 if the program inherited that.
	let (_, exit) = run(Command::new("sh").arg("-c").arg("kill -PIPE $$; exit 0"));
	assert_eq!(exit, Exit::Signal(libc::SIGPIPE));
}

#[test]
fn signals_we_block_are_not_blocked_in_the_program() {
	// fork copies the calling thread's mask, so block one here.
	// SAFETY: plain sets on our stack, changing only this thread's mask.
	let old = unsafe {
		let mut block = std::mem::zeroed::<libc::sigset_t>();
		let mut old = std::mem::zeroed::<libc::sigset_t>();
		libc::sigemptyset(&mut block);
		libc::sigaddset(&mut block, libc::SIGUSR1);
		libc::pthread_sigmask(libc::SIG_BLOCK, &block, &mut old);
		old
	};
	let (_, exit) = run(Command::new("sh").arg("-c").arg("kill -USR1 $$; exit 0"));
	// SAFETY: restores this thread's mask from the copy above.
	unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut()) };
	assert_eq!(exit, Exit::Signal(libc::SIGUSR1));
}

#[test]
fn waiting_twice_is_an_error_not_a_hang() {
	let pair = device::open(WindowSize::default()).unwrap();
	let mut child = spawn(&Command::new("sh").arg("-c").arg("exit 3"), &pair.slave).unwrap();
	assert_eq!(child.wait().unwrap(), Exit::Code(3));
	let again = child.wait().err().unwrap();
	assert_eq!(again.raw_os_error(), Some(libc::ECHILD));
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

#[test]
fn none_of_our_descriptors_reach_the_program() {
	// One we would leak if nothing closed it: inheritable, unlike std's.
	let file = File::open("Cargo.toml").unwrap();
	// SAFETY: duplicates a descriptor we own, without close-on-exec.
	let leaky = unsafe { libc::dup(std::os::fd::AsRawFd::as_raw_fd(&file)) };
	assert!(leaky > 2);
	// macOS marks them one by one, so this checks that path there.
	#[cfg(target_os = "linux")]
	let script = "ls /proc/self/fd";
	#[cfg(not(target_os = "linux"))]
	let script = "ls /dev/fd";
	let (output, _) = run(sh(script));
	// SAFETY: closes the duplicate made above.
	unsafe { libc::close(leaky) };
	let fds: Vec<&str> = output.split_whitespace().collect();
	// 0, 1, 2, and the directory ls reads; macOS's ls keeps one more.
	#[cfg(target_os = "linux")]
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

#[test]
fn control_c_typed_on_the_master_interrupts_the_program() {
	// Only the terminal's foreground group gets it, so this needs both the
	// controlling terminal and default SIGINT handling.
	let exit = typed_once_ready(&pty(), "read line", b"\x03");
	assert_eq!(exit, Exit::Signal(libc::SIGINT));
}

/// Runs `script` on `pair`, types `keys` on the master once it is running
/// (macOS loses control characters typed while it starts), and says how it
/// ended. A program still running after 10 s is killed, so a test fails
/// instead of hanging.
fn typed_once_ready(pair: &device::Pair, script: &str, keys: &[u8]) -> Exit {
	let script = format!("echo ready; {script}");
	let mut child = spawn(&Command::new("sh").arg("-c").arg(script), &pair.slave).unwrap();
	let shown = watch(pair.master.try_clone().unwrap());
	until(&shown, "ready");
	File::from(pair.master.try_clone().unwrap())
		.write_all(keys)
		.unwrap();
	let pid = child.id() as libc::pid_t;
	let (ended, watchdog) = mpsc::channel::<()>();
	thread::spawn(move || {
		if watchdog.recv_timeout(Duration::from_secs(10)).is_err() {
			// SAFETY: signals only the child we started, not yet reaped.
			unsafe { libc::kill(pid, libc::SIGKILL) };
		}
	});
	let exit = child.wait().unwrap();
	let _ = ended.send(());
	exit
}

#[test]
fn closing_the_master_hangs_up_the_session() {
	let device::Pair { master, slave, .. } = pty();
	let mut child = spawn(&Command::new("sh").arg("-c").arg("read line"), &slave).unwrap();
	drop(slave);
	drop(master);
	assert_eq!(child.wait().unwrap(), Exit::Signal(libc::SIGHUP));
}

#[test]
fn resizing_the_terminal_tells_the_program() {
	let pair = pty();
	let script = "trap 'stty size; exit 0' WINCH; echo ready; while :; do sleep 0.05; done";
	let mut child = spawn(&Command::new("sh").arg("-c").arg(script), &pair.slave).unwrap();
	let size = WindowSize {
		cols: 100,
		rows: 30,
		..WindowSize::default()
	};
	let shown = watch(pair.master.try_clone().unwrap());
	until(&shown, "ready");
	device::set_size(&pair.master, size).unwrap();
	assert!(until(&shown, "30 100").ends_with("30 100\r\n"));
	assert_eq!(child.wait().unwrap(), Exit::Code(0));
}

#[test]
fn standard_error_reaches_the_terminal_too() {
	let (output, _) = run(sh("echo to-stderr >&2"));
	assert_eq!(output, "to-stderr\n");
}

#[test]
fn a_relative_directory_is_taken_from_ours() {
	let ours = std::env::current_dir().unwrap().canonicalize().unwrap();
	let (output, _) = run(sh("pwd -P").cwd("src"));
	assert_eq!(output.trim_end(), ours.join("src").to_str().unwrap());
}

#[test]
fn values_that_are_not_utf8_or_empty_arrive() {
	use std::os::unix::ffi::OsStringExt;
	let script = r#"printf %s "$POOPTERM_BYTES" | od -An -tx1; echo "${POOPTERM_EMPTY+set}""#;
	let command = sh(script)
		.env("POOPTERM_BYTES", OsString::from_vec(b"\xc3\x28".to_vec()))
		.env("POOPTERM_EMPTY", "");
	let (output, _) = run(command);
	assert_eq!(
		output.split_whitespace().collect::<Vec<_>>(),
		["c3", "28", "set"]
	);
}

#[test]
fn hanging_up_a_program_already_waited_for_is_an_error() {
	let pair = device::open(WindowSize::default()).unwrap();
	let mut child = spawn(&Command::new("sh").arg("-c").arg("exit 0"), &pair.slave).unwrap();
	child.wait().unwrap();
	assert_eq!(
		child.hang_up().err().unwrap().raw_os_error(),
		Some(libc::ESRCH)
	);
}

#[test]
fn spawns_on_many_threads_at_once_each_get_their_own_result() {
	// Each report pipe must reach only its own child, and every child its
	// own terminal, while other threads fork.
	let threads: Vec<_> = (0..8)
		.map(|thread| {
			thread::spawn(move || {
				for round in 0..4 {
					let code = thread * 10 + round;
					let pair = device::open(WindowSize::default()).unwrap();
					let command = Command::new("sh").arg("-c").arg(format!("exit {code}"));
					let mut child = spawn(&command, &pair.slave).unwrap();
					assert_eq!(child.wait().unwrap(), Exit::Code(code));
					// A fresh terminal: the BSDs revoke one whose session ended.
					let other = device::open(WindowSize::default()).unwrap();
					let missing = Command::new("/nonexistent/poopterm-thread");
					let error = spawn(&missing, &other.slave).err().unwrap();
					assert_eq!(error.raw_os_error(), Some(libc::ENOENT));
				}
			})
		})
		.collect();
	for thread in threads {
		thread.join().unwrap();
	}
}
