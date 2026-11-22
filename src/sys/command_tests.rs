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
	assert!(shown.contains("13 passed"), "{shown}");
}

const PROGRAM: &str = r#"
#include <errno.h>
#include <getopt.h>
#include <string.h>
#include <termios.h>
static int runs;
int main(int argc, char **argv) {
	const char *mode = argc > 1 ? argv[1] : "";
	if (!strcmp(mode, "return")) return 7;
	if (!strcmp(mode, "exit")) { printf("before\n"); exit(9); printf("after\n"); }
	if (!strcmp(mode, "err")) { errno = ENOENT; err(4, "%s", "gone"); }
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
			.args(["-dynamiclib", "-w", "-include"])
			.arg(format!("{shim}/poopterm_cmd.h"))
			.arg("-o")
			.arg(&out)
			.arg(format!("{shim}/poopterm_cmd.c"))
			.arg(&source)
			.status()
			.unwrap();
		assert!(status.success(), "cc failed");
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
	};
	assert_eq!(call(&libc, &strings(&["x"]), &[], &stdio), None);
}
