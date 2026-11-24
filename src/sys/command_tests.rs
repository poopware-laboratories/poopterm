//! Command libraries: a test program built with the system compiler and
//! `ios/commands/poopterm_cmd.c` exactly as real commands are, then called
//! through its entry. Exit statuses, `exit` and `err`, arguments, the
//! environment, the terminal it is told it has, getopt between runs, and
//! the app's own descriptors and environment afterwards.
#![cfg(target_vendor = "apple")]
use std::{
	ffi::CString,
	fs,
	io::{self, Read},
	os::fd::AsFd,
	path::PathBuf,
	process::Command,
	sync::{Mutex, OnceLock},
};

use super::*;

/// Calls swap the process's standard descriptors, so they run one at a
/// time (as the shell runs commands).
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn vars(words: &[&str]) -> Vec<CString> {
	strings(words)
}

#[test]
fn the_path_is_read_from_the_commands_environment() {
	assert_eq!(
		search_path(&vars(&["HOME=/h", "PATH=/a:/b"])),
		Some("/a:/b")
	);
	assert_eq!(search_path(&vars(&["HOME=/h"])), None);
	assert_eq!(
		search_path(&vars(&["MYPATH=/x", "PATHS=/y"])),
		None,
		"only PATH itself"
	);
	assert_eq!(search_path(&vars(&["PATH="])), Some(""));
	assert_eq!(
		search_path(&vars(&["PATH=/old", "PATH=/new"])),
		Some("/new"),
		"the last setting wins, as getenv's does after a later putenv"
	);
}

#[test]
fn a_path_that_is_not_utf8_finds_nothing() {
	let env = [CString::new(b"PATH=/\xff".to_vec()).unwrap()];
	assert_eq!(search_path(&env), None);
}

#[test]
fn nothing_to_run_is_enoent_and_a_non_command_is_enoexec() {
	let dir = std::env::temp_dir().join(format!("poopterm-nested-{}", std::process::id()));
	fs::create_dir_all(&dir).unwrap();
	fs::write(dir.join("script"), "#!/bin/sh\n").unwrap();
	let (_reader, writer) = io::pipe().unwrap();
	let stdio = Stdio {
		input: writer.as_fd(),
		output: writer.as_fd(),
		error: writer.as_fd(),
		tty: None,
		device: None,
		keyboard: false,
		keys: None,
	};
	let env = vars(&[&format!("PATH={}", dir.display())]);
	assert_eq!(
		run_nested("missing", &strings(&["missing"]), &env, &stdio),
		-ENOENT
	);
	assert_eq!(
		run_nested("script", &strings(&["script"]), &env, &stdio),
		-ENOEXEC
	);
	assert_eq!(
		run_nested("script", &[], &[], &stdio),
		-ENOENT,
		"no PATH, no commands"
	);
}

/// While a command runs, what the test harness prints about other tests
/// would land in its output: the tests below run in a process of their
/// own, one at a time.
#[test]
fn each_in_a_process_of_their_own() {
	let output = Command::new(std::env::current_exe().unwrap())
		.args(["sys::command::tests::", "--ignored", "--test-threads=1"])
		.output()
		.unwrap();
	let shown = String::from_utf8_lossy(&output.stdout) + String::from_utf8_lossy(&output.stderr);
	assert!(output.status.success(), "{shown}");
	assert!(shown.contains("34 passed"), "{shown}");
}

const PROGRAM: &str = r#"
#include <errno.h>
#include <getopt.h>
#include <string.h>
#include <termios.h>
#include <fcntl.h>
extern char **environ;
static int runs;
/* Waits for `pid` and prints how it ended. */
static void reap(pid_t pid) {
	int status = -1;
	pid_t got = waitpid(pid, &status, 0);
	printf("[%d %d]", got == pid, WIFEXITED(status) ? WEXITSTATUS(status) : -1);
}
int main(int argc, char **argv) {
	const char *mode = argc > 1 ? argv[1] : "";
	if (!strcmp(mode, "return")) return 7;
	if (!strcmp(mode, "exit")) { printf("before\n"); exit(9); printf("after\n"); }
	/* As git does: its own exit macro, calling the function by name. */
	if (!strcmp(mode, "libexit")) { printf("before\n"); (exit)(3); printf("after\n"); }
#ifndef POOPTERM_NO_ERR_H
	if (!strcmp(mode, "err")) { errno = ENOENT; err(4, "%s", "gone"); }
#endif
	if (!strcmp(mode, "errx")) errx(5, "plain %d", 1);
	if (!strcmp(mode, "args")) {
		for (int i = 0; i < argc; i++) printf("[%s]", argv[i]);
		argv[1][0] = 'X';
		return 0;
	}
	if (!strcmp(mode, "env")) {
		const char *greeting = getenv("GREETING"), *home = getenv("POOPTERM_APP_ONLY");
		printf("%s|%s", greeting ? greeting : "none", home ? home : "none");
		return 0;
	}
	if (!strcmp(mode, "tty")) {
		struct winsize size = {0};
		int got = ioctl(1, TIOCGWINSZ, &size);
		printf("%d %d %d %d", isatty(1), got, size.ws_col, size.ws_row);
		return 0;
	}
	if (!strcmp(mode, "getopt")) {
		int c;
		while ((c = getopt(argc - 1, argv + 1, "ab")) != -1) printf("%c", c);
		printf(" %d", optind);
		return 0;
	}
	if (!strcmp(mode, "upper")) {
		int c;
		while ((c = getchar()) != EOF) putchar(c >= 'a' && c <= 'z' ? c - 32 : c);
		return 0;
	}
	if (!strcmp(mode, "stderr")) { fprintf(stderr, "to error"); return 0; }
	if (!strcmp(mode, "runs")) { printf("%d", ++runs); return 0; }
	if (!strcmp(mode, "spawn") || !strcmp(mode, "quiet") || !strcmp(mode, "joined")) {
		posix_spawn_file_actions_t actions;
		posix_spawn_file_actions_init(&actions);
		if (!strcmp(mode, "quiet"))
			posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0);
		if (!strcmp(mode, "joined"))
			posix_spawn_file_actions_adddup2(&actions, 1, 2);
		pid_t pid = 0;
		int failed = posix_spawnp(&pid, argv[2], &actions, NULL, argv + 2, environ);
		posix_spawn_file_actions_destroy(&actions);
		if (failed) { printf("[failed %s]", strerror(failed)); return 0; }
		printf("<");
		reap(pid);
		return 0;
	}
	if (!strcmp(mode, "fork")) {
		char before[1024], after[1024];
		getcwd(before, sizeof before);
		pid_t pid = fork();
		if (pid == 0) {
			chdir("/");
			dup2(2, 1);
			execvp(argv[2], argv + 2);
			_exit(42);
		}
		getcwd(after, sizeof after);
		printf("<");
		reap(pid);
		printf("[%s]", strcmp(before, after) ? "moved" : "stayed");
		return 0;
	}
	if (!strcmp(mode, "readahead")) {
		/* One character read buffers the rest; the child then reads its own. */
		putchar(getchar());
		pid_t pid;
		posix_spawn_file_actions_t actions;
		posix_spawn_file_actions_init(&actions);
		posix_spawn_file_actions_addopen(&actions, 0, "/dev/null", O_RDONLY, 0);
		char *child[] = {"helper", "upper", NULL};
		posix_spawnp(&pid, "helper", &actions, NULL, child, environ);
		waitpid(pid, NULL, 0);
		int c;
		while ((c = getchar()) != EOF) putchar(c);
		return 0;
	}
	if (!strcmp(mode, "spin")) {
		/* As yes does: writes until something ends it. */
		for (;;) {
			if (write(1, "y\n", 2) < 0) { printf("[write %s]", strerror(errno)); return 3; }
		}
	}
	if (!strcmp(mode, "ownbuffer")) {
		/* Reads stdin through a buffer of its own, and leaves it there. */
		static char mine[4096];
		setvbuf(stdin, mine, _IOFBF, sizeof mine);
		int c;
		while ((c = getchar()) != EOF) putchar(c);
		return 0;
	}
	if (!strcmp(mode, "getpass")) {
		char *secret = getpass("pw: ");
		struct termios after;
		tcgetattr(0, &after);
		printf("[%s][echo %d]", secret, !!(after.c_lflag & ECHO));
		return 0;
	}
	if (!strcmp(mode, "devtty")) {
		FILE *keys = fopen("/dev/tty", "r");
		char line[64] = "";
		if (keys == NULL) { printf("[no keyboard: %s]", strerror(errno)); return 1; }
		fgets(line, sizeof line, keys);
		fclose(keys);
		FILE *screen = fopen("/dev/tty", "w");
		if (screen != NULL) { fprintf(screen, "(to the screen)"); fclose(screen); }
		line[strcspn(line, "\n")] = 0;
		printf("[%s]", line);
		return 0;
	}
	if (!strcmp(mode, "block")) {
		/* As pax does: blocks signals and ends without unblocking them. */
		sigset_t all;
		sigfillset(&all);
		sigprocmask(SIG_BLOCK, &all, NULL);
		return 0;
	}
	if (!strcmp(mode, "isatty")) { printf("%d%d%d", isatty(0), isatty(1), isatty(2)); return 0; }
	if (!strcmp(mode, "raw")) {
		/* As an editor starts: reads the settings, goes raw, reads a key. */
		struct termios settings;
		if (tcgetattr(0, &settings) != 0) { printf("[%s]", strerror(errno)); return 0; }
		printf("[%d]", !!(settings.c_lflag & ICANON));
		cfmakeraw(&settings);
		settings.c_cc[VMIN] = 1;
		printf("[%d]", tcsetattr(0, TCSAFLUSH, &settings));
		tcgetattr(0, &settings);
		printf("[%d]", !!(settings.c_lflag & ICANON));
		struct winsize size = {0};
		ioctl(0, TIOCGWINSZ, &size);
		char key = 0;
		printf("[%dx%d][%ld:%d]", size.ws_col, size.ws_row, (long)read(0, &key, 1), key);
		return 0;
	}
	if (!strcmp(mode, "interrupt")) {
		printf("before\n");
		fflush(stdout);
		raise(SIGINT);
		printf("after\n");
		return 0;
	}
	if (!strcmp(mode, "ignore")) {
		printf("[%d]", signal(SIGINT, SIG_IGN) == SIG_DFL);
		raise(SIGINT);
		printf("survived");
		return 0;
	}
	if (!strcmp(mode, "default")) {
		signal(SIGINT, SIG_IGN);
		printf("[%d]", signal(SIGINT, SIG_DFL) == SIG_IGN);
		struct sigaction old;
		sigaction(SIGINT, NULL, &old);
		printf("[%d]", old.sa_handler == SIG_DFL);
		fflush(stdout);
		raise(SIGTSTP);
		raise(SIGPIPE);
		printf("not here");
		return 0;
	}
	if (!strcmp(mode, "orphan")) {
		int status;
		pid_t got = waitpid(-1, &status, 0);
		printf("[%d %s]", got, strerror(errno));
		return 0;
	}
	return 1;
}
"#;

/// The test program as a command library, built once per test run.
fn library_path() -> &'static PathBuf {
	static BUILT: OnceLock<PathBuf> = OnceLock::new();
	BUILT.get_or_init(|| {
		let dir = std::env::temp_dir().join(format!("poopterm-command-{}", std::process::id()));
		fs::create_dir_all(&dir).unwrap();
		let source = dir.join("program.c");
		fs::write(&source, PROGRAM).unwrap();
		let shim = concat!(env!("CARGO_MANIFEST_DIR"), "/ios/commands");
		let out = dir.join("program");
		let status = Command::new("cc")
			.args(["-dynamiclib", "-w", "-DPOOPTERM_DEV_TTY", "-include"])
			.arg(format!("{shim}/poopterm_cmd.h"))
			.arg("-o")
			.arg(&out)
			.arg(format!("{shim}/poopterm_cmd.c"))
			.arg(&source)
			.status()
			.unwrap();
		assert!(status.success(), "cc failed");
		// A second copy, a command of its own the first can start.
		fs::copy(&out, dir.join("helper")).unwrap();
		// A third built as a command with its own `err` global is (pr).
		let status = Command::new("cc")
			.args(["-dynamiclib", "-w", "-DPOOPTERM_NO_ERR_H", "-include"])
			.arg(format!("{shim}/poopterm_cmd.h"))
			.arg("-o")
			.arg(dir.join("noerrh"))
			.arg(format!("{shim}/poopterm_cmd.c"))
			.arg(&source)
			.status()
			.unwrap();
		assert!(status.success(), "cc -DPOOPTERM_NO_ERR_H failed");
		out
	})
}

fn open() -> Library {
	let name = CString::new(library_path().as_os_str().as_encoded_bytes()).unwrap();
	Library::open(&name).unwrap()
}

fn strings(words: &[&str]) -> Vec<CString> {
	words
		.iter()
		.map(|word| CString::new(*word).unwrap())
		.collect()
}

/// Runs the program with `args`, `env` and `stdin`, and returns its
/// status, what it wrote to standard output, and to standard error.
fn run_with(
	args: &[&str],
	env: &[&str],
	stdin: &[u8],
	tty: Option<(u16, u16)>,
) -> (i32, String, String) {
	let _guard = ONE_AT_A_TIME
		.lock()
		.unwrap_or_else(|poison| poison.into_inner());
	let library = open();
	let (input, mut feed) = io::pipe().unwrap();
	io::Write::write_all(&mut feed, stdin).unwrap();
	drop(feed);
	let (mut out_reader, out_writer) = io::pipe().unwrap();
	let (mut err_reader, err_writer) = io::pipe().unwrap();
	let mut argv = vec!["program"];
	argv.extend(args);
	let stdio = Stdio {
		input: input.as_fd(),
		output: out_writer.as_fd(),
		error: err_writer.as_fd(),
		tty,
		device: None,
		keyboard: false,
		keys: None,
	};
	let status = call(&library, &strings(&argv), &strings(env), &stdio).unwrap();
	drop((out_writer, err_writer));
	let (mut out, mut err) = (String::new(), String::new());
	out_reader.read_to_string(&mut out).unwrap();
	err_reader.read_to_string(&mut err).unwrap();
	(status, out, err)
}

fn run(args: &[&str]) -> (i32, String, String) {
	run_with(args, &[], b"", None)
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn the_status_main_returns_is_the_commands() {
	assert_eq!(run(&["return"]).0, 7);
	assert_eq!(run(&["no-such-mode"]).0, 1);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn exit_ends_the_command_not_the_app_and_flushes_its_output() {
	assert_eq!(run(&["exit"]), (9, "before\n".into(), String::new()));
	// Still here, and the library runs again.
	assert_eq!(run(&["return"]).0, 7);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn err_and_errx_print_to_standard_error_and_end_with_their_status() {
	let (status, out, err) = run(&["err"]);
	assert_eq!((status, out.as_str()), (4, ""));
	assert!(
		err.ends_with(": gone: No such file or directory\n"),
		"{err}"
	);
	let (status, _, err) = run(&["errx"]);
	assert_eq!(status, 5);
	assert!(err.ends_with(": plain 1\n"), "{err}");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn arguments_arrive_in_order_and_may_be_changed() {
	let (status, out, _) = run(&["args", "two words", ""]);
	assert_eq!(status, 0);
	assert_eq!(out, "[program][args][two words][]");
	// The change stayed in the run's own copy.
	assert_eq!(run(&["args"]).1, "[program][args]");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn the_command_sees_only_the_environment_it_is_given() {
	// SAFETY-free check of the app's own environment: it is untouched after.
	let before: Vec<_> = std::env::vars_os().collect();
	let (_, out, _) = run_with(&["env"], &["GREETING=hello there"], b"", None);
	assert_eq!(out, "hello there|none");
	assert_eq!(run_with(&["env"], &[], b"", None).1, "none|none");
	assert_eq!(std::env::vars_os().collect::<Vec<_>>(), before);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn a_terminal_answers_isatty_and_the_window_size() {
	assert_eq!(
		run_with(&["tty"], &[], b"", Some((132, 43))).1,
		"1 0 132 43"
	);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn without_a_terminal_isatty_is_false_and_the_size_fails() {
	assert_eq!(run_with(&["tty"], &[], b"", None).1, "0 -1 0 0");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn getopt_starts_over_on_each_run() {
	assert_eq!(run(&["getopt", "-a", "-b", "rest"]).1, "ab 3");
	assert_eq!(run(&["getopt", "-b"]).1, "b 2");
	// An unknown option is reported once per run, not carried over.
	let (_, out, err) = run(&["getopt", "-z"]);
	assert_eq!(out, "? 2");
	assert!(err.contains("illegal option -- z"), "{err}");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn standard_input_comes_from_the_descriptor_given_and_ends_with_it() {
	assert_eq!(run_with(&["upper"], &[], b"shout\nit", None).1, "SHOUT\nIT");
	// Nothing read before is left in stdio's buffer for the next run.
	assert_eq!(run_with(&["upper"], &[], b"", None).1, "");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn standard_error_goes_to_its_own_descriptor() {
	assert_eq!(run(&["stderr"]), (0, String::new(), "to error".into()));
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn the_apps_standard_descriptors_are_put_back() {
	let identity = |fd: i32| {
		let meta = fs::metadata(format!("/dev/fd/{fd}")).ok();
		meta.map(|meta| std::os::unix::fs::MetadataExt::ino(&meta))
	};
	let before: Vec<_> = (0..3).map(identity).collect();
	run(&["stderr"]);
	run(&["exit"]);
	assert_eq!((0..3).map(identity).collect::<Vec<_>>(), before);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn reopening_a_library_starts_its_globals_over() {
	assert_eq!(run(&["runs"]).1, "1");
	assert_eq!(run(&["runs"]).1, "1");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn a_library_without_an_entry_is_refused() {
	let libc = Library::open(c"/usr/lib/libSystem.B.dylib").unwrap();
	let (_reader, writer) = io::pipe().unwrap();
	let stdio = Stdio {
		input: writer.as_fd(),
		output: writer.as_fd(),
		error: writer.as_fd(),
		tty: None,
		device: None,
		keyboard: false,
		keys: None,
	};
	assert_eq!(call(&libc, &strings(&["x"]), &[], &stdio), None);
}

/// A terminal for tests: keeps the settings it is given.
struct Keeper(Mutex<libc::termios>, Mutex<Vec<bool>>);

impl Device for Keeper {
	fn settings(&self) -> libc::termios {
		*self.0.lock().unwrap()
	}
	fn set_settings(&self, settings: &libc::termios, _flush: bool) {
		*self.0.lock().unwrap() = *settings;
		// Whether echo was on each time the settings were set.
		self
			.1
			.lock()
			.unwrap()
			.push(settings.c_lflag & libc::ECHO != 0);
	}
	fn size(&self) -> (u16, u16) {
		(100, 30)
	}
}

/// Runs the program on a terminal that `device` is, reading `keys` from
/// the keyboard: its status and what it printed.
fn run_on(args: &[&str], device: &dyn Device, keys: &[u8]) -> (i32, String) {
	let _guard = ONE_AT_A_TIME
		.lock()
		.unwrap_or_else(|poison| poison.into_inner());
	let library = open();
	let (input, mut feed) = io::pipe().unwrap();
	io::Write::write_all(&mut feed, keys).unwrap();
	drop(feed);
	let (mut out_reader, out_writer) = io::pipe().unwrap();
	let mut argv = vec!["program"];
	argv.extend(args);
	let stdio = Stdio {
		input: input.as_fd(),
		output: out_writer.as_fd(),
		error: out_writer.as_fd(),
		tty: Some((80, 24)),
		device: Some(device),
		keyboard: true,
		keys: Some(input.as_fd()),
	};
	let status = call(&library, &strings(&argv), &[], &stdio).unwrap();
	drop(out_writer);
	let mut out = String::new();
	out_reader.read_to_string(&mut out).unwrap();
	(status, out)
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn getpass_asks_on_the_device_with_echo_off_and_puts_it_back() {
	let keeper = Keeper(Mutex::new(crate::sys::termios::fresh()), Mutex::default());
	let (status, out) = run_on(&["getpass"], &keeper, b"hunter2\n");
	assert_eq!((status, out.as_str()), (0, "pw: \n[hunter2][echo 1]"));
	assert_eq!(
		*keeper.1.lock().unwrap(),
		[false, true],
		"off for the typing, on after"
	);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn dev_tty_is_the_devices_keyboard_to_read_and_its_screen_to_write() {
	let keeper = Keeper(Mutex::new(crate::sys::termios::fresh()), Mutex::default());
	let (_, out) = run_on(&["devtty"], &keeper, b"typed line\n");
	assert_eq!(out, "(to the screen)[typed line]");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn dev_tty_fails_where_there_is_no_terminal() {
	let (status, out, _) = run_with(&["devtty"], &[], b"", None);
	assert_eq!(status, 1);
	assert!(out.starts_with("[no keyboard: "), "{out}");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn each_standard_descriptor_says_whether_it_is_the_terminal() {
	let keeper = Keeper(Mutex::new(crate::sys::termios::fresh()), Mutex::default());
	assert_eq!(run_on(&["isatty"], &keeper, b"").1, "111");
	assert_eq!(
		run_with(&["isatty"], &[], b"", Some((80, 24))).1,
		"011",
		"input from a pipe"
	);
	assert_eq!(run_with(&["isatty"], &[], b"", None).1, "000");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn a_program_reads_and_sets_the_devices_settings_and_live_size() {
	let keeper = Keeper(Mutex::new(crate::sys::termios::fresh()), Mutex::default());
	let (status, out) = run_on(&["raw"], &keeper, b"q");
	assert_eq!((status, out.as_str()), (0, "[1][0][0][100x30][1:113]"));
	let left = keeper.settings();
	assert_eq!(
		left.c_lflag & (libc::ICANON | libc::ECHO),
		0,
		"the device kept raw mode"
	);
	assert_eq!(left.c_cc[libc::VMIN], 1);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn without_a_device_the_settings_are_no_terminals() {
	assert_eq!(
		run_with(&["raw"], &[], b"", None).1,
		"[Inappropriate ioctl for device]"
	);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn sigint_ends_the_command_with_its_status_not_the_app() {
	crate::sys::signal::quiet_job_signals();
	assert_eq!(run(&["interrupt"]), (130, "before\n".into(), String::new()));
	assert_eq!(run(&["return"]).0, 7, "the app and the library live on");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn an_ignored_sigint_is_ignored_and_the_apps_handler_comes_back() {
	crate::sys::signal::quiet_job_signals();
	let before = handler(libc::SIGINT);
	assert_eq!(
		run(&["ignore"]).1,
		"[1]survived",
		"it reads the default it was given"
	);
	assert_eq!(handler(libc::SIGINT), before, "not left ignored");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn defaults_asked_for_end_or_stop_only_the_command() {
	crate::sys::signal::quiet_job_signals();
	// SIGTSTP would stop the app and SIGPIPE end it; the first does
	// nothing, the second ends the command as a broken pipe ends a program.
	assert_eq!(
		run(&["default"]),
		(128 + libc::SIGPIPE, "[1][1]".into(), String::new())
	);
	assert!(crate::sys::signal::caught(libc::SIGTSTP));
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn a_buffer_a_command_gave_stdin_is_not_used_after_it_is_unloaded() {
	assert_eq!(run_with(&["ownbuffer"], &[], b"first", None).1, "first");
	// Each run reopens the library, so its static buffer moved or went:
	// stdin must not still point there.
	assert_eq!(run_with(&["upper"], &[], b"after", None).1, "AFTER");
	assert_eq!(run_with(&["ownbuffer"], &[], b"again", None).1, "again");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn sigint_from_another_thread_ends_a_command_stuck_writing() {
	crate::sys::signal::quiet_job_signals();
	let _guard = ONE_AT_A_TIME
		.lock()
		.unwrap_or_else(|poison| poison.into_inner());
	let library = open();
	let (input, _feed) = io::pipe().unwrap();
	let (mut out_reader, out_writer) = io::pipe().unwrap();
	let me = crate::sys::signal::Thread::current();
	// Reads what it prints, as the screen does, then interrupts it.
	let reader = std::thread::spawn(move || {
		let mut buf = [0u8; 4096];
		let mut total = 0;
		while total < 1 << 16 {
			total += out_reader.read(&mut buf).unwrap();
		}
		assert!(me.signal(libc::SIGINT));
		while out_reader.read(&mut buf).unwrap() > 0 {}
	});
	let stdio = Stdio {
		input: input.as_fd(),
		output: out_writer.as_fd(),
		error: out_writer.as_fd(),
		tty: Some((80, 24)),
		device: None,
		keyboard: false,
		keys: None,
	};
	// Left blocked by a command before, as pax leaves them.
	assert_eq!(
		call(&library, &strings(&["program", "block"]), &[], &stdio),
		Some(0)
	);
	let status = call(&library, &strings(&["program", "spin"]), &[], &stdio).unwrap();
	drop(out_writer);
	reader.join().unwrap();
	assert_eq!(status, 130);
}

/// The handler installed for `signal`.
fn handler(signal: libc::c_int) -> libc::sighandler_t {
	let mut current = std::mem::MaybeUninit::<libc::sigaction>::uninit();
	// SAFETY: with a null new action, only the current one is written.
	let got = unsafe { libc::sigaction(signal, std::ptr::null(), current.as_mut_ptr()) };
	assert_eq!(got, 0);
	// SAFETY: filled by the successful call above.
	unsafe { current.assume_init() }.sa_sigaction
}

/// The test program's directory as a `$PATH`.
fn path() -> String {
	format!("PATH={}", library_path().parent().unwrap().display())
}

fn spawned(args: &[&str], stdin: &[u8]) -> (i32, String, String) {
	run_with(args, &[&path()], stdin, None)
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn a_command_starts_another_and_waits_for_its_status() {
	let (status, out, _) = spawned(&["spawn", "helper", "return"], b"");
	assert_eq!((status, out.as_str()), (0, "<[1 7]"));
	let (_, out, _) = spawned(&["spawn", "helper", "args", "x"], b"");
	assert_eq!(out, "[helper][args][x]<[1 0]", "its output comes first");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn the_child_ending_with_exit_or_err_does_not_end_the_parent() {
	assert_eq!(
		spawned(&["spawn", "helper", "exit"], b"").1,
		"before\n<[1 9]"
	);
	let (status, out, err) = spawned(&["spawn", "helper", "err"], b"");
	assert_eq!((status, out.as_str()), (0, "<[1 4]"));
	assert!(err.contains("gone"), "{err}");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn file_actions_point_the_childs_descriptors() {
	assert_eq!(
		spawned(&["spawn", "helper", "upper"], b"shared").1,
		"SHARED<[1 0]",
		"without actions it reads the parent's input"
	);
	assert_eq!(
		spawned(&["quiet", "helper", "upper"], b"shared").1,
		"<[1 0]",
		"opened on /dev/null"
	);
	let (_, out, err) = spawned(&["joined", "helper", "stderr"], b"");
	assert_eq!((out.as_str(), err.as_str()), ("to error<[1 0]", ""));
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn starting_what_is_not_on_the_path_fails_with_enoent() {
	let (status, out, _) = spawned(&["spawn", "no-such-command"], b"");
	assert_eq!(
		(status, out.as_str()),
		(0, "[failed No such file or directory]")
	);
	let (_, out, _) = run_with(&["spawn", "helper"], &["PATH=/nowhere"], b"", None);
	assert_eq!(out, "[failed No such file or directory]");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn a_command_cannot_start_itself() {
	let (status, out, err) = spawned(&["spawn", "program", "return"], b"");
	assert_eq!((status, out.as_str()), (0, "<[1 126]"));
	assert!(err.contains("cannot run inside itself"), "{err}");
	assert_eq!(run(&["return"]).0, 7, "and still runs after");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn fork_and_exec_run_the_child_then_return_as_the_parent() {
	let (status, out, err) = spawned(&["fork", "helper", "args", "y"], b"");
	assert_eq!(status, 0);
	assert_eq!(
		err, "[helper][args][y]",
		"the child's own dup2 applied to it"
	);
	assert_eq!(
		out, "<[1 0][stayed]",
		"and was undone for the parent, as was its chdir"
	);
	let (_, out, err) = spawned(&["fork", "no-such-command"], b"");
	assert_eq!(
		(out.as_str(), err.as_str()),
		("<[1 42][stayed]", ""),
		"exec failed: _exit"
	);
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn waiting_without_children_fails_with_echild() {
	let (_, out, _) = spawned(&["orphan"], b"");
	assert_eq!(out, "[-1 No child processes]");
	// Children of a run are forgotten by the next.
	spawned(&["spawn", "helper", "return"], b"");
	assert_eq!(spawned(&["orphan"], b"").1, "[-1 No child processes]");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn input_read_ahead_before_a_child_is_still_there_after_it() {
	assert_eq!(spawned(&["readahead"], b"abc\ndef\n").1, "abc\ndef\n");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn without_err_h_errx_still_ends_only_the_command() {
	let (status, out, err) = spawned(&["spawn", "noerrh", "errx"], b"");
	assert_eq!(
		(status, out.as_str()),
		(0, "<[1 5]"),
		"the parent carries on"
	);
	assert!(err.ends_with("noerrh: plain 1\n"), "{err}");
}

#[test]
#[ignore = "run by each_in_a_process_of_their_own"]
fn calling_the_exit_function_itself_ends_only_the_command() {
	assert_eq!(run(&["libexit"]), (3, "before\n".into(), String::new()));
	assert_eq!(run(&["return"]).0, 7, "and the app is still here");
}
