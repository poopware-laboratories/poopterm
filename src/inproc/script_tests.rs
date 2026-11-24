use std::{fs, io::Read, os::fd::AsFd};

use super::*;

fn args(words: &[&str]) -> Vec<CString> {
	words
		.iter()
		.map(|word| CString::new(*word).unwrap())
		.collect()
}

#[test]
fn a_shell_asked_to_run_a_line_gives_the_line() {
	for shell in ["sh", "/bin/sh", "bash", "/usr/bin/zsh", "dash"] {
		assert_eq!(
			script_of(shell, &args(&[shell, "-c", "echo hi"])),
			Some("echo hi"),
			"{shell}"
		);
	}
	assert_eq!(
		script_of("sh", &args(&["sh", "-ec", "set -e; true"])),
		Some("set -e; true")
	);
	assert_eq!(
		script_of("sh", &args(&["sh", "-c", "a", "name", "arg"])),
		Some("a"),
		"extra words follow"
	);
}

#[test]
fn an_exec_of_a_builtin_becomes_a_line_with_quoted_words() {
	assert_eq!(
		builtin_line("echo", &args(&["echo", "hello", "it's $HOME", ""])).as_deref(),
		Some("echo 'hello' 'it'\\''s $HOME' ''")
	);
	assert_eq!(builtin_line("pwd", &args(&["pwd"])).as_deref(), Some("pwd"));
	assert_eq!(
		builtin_line("/bin/echo", &args(&["echo"])),
		None,
		"a path is a file"
	);
	assert_eq!(
		builtin_line("cd", &args(&["cd", "/"])),
		None,
		"cd changes nothing as a process"
	);
	assert_eq!(builtin_line("ls", &args(&["ls"])), None);
}

#[test]
fn anything_else_is_not_a_line_to_run() {
	assert_eq!(
		script_of("sh", &args(&["sh"])),
		None,
		"an interactive shell"
	);
	assert_eq!(
		script_of("sh", &args(&["sh", "script.sh"])),
		None,
		"a script file"
	);
	assert_eq!(
		script_of("sh", &args(&["sh", "-x", "echo"])),
		None,
		"flags without c"
	);
	assert_eq!(script_of("sh", &args(&["sh", "-c"])), None, "no line");
	assert_eq!(
		script_of("ls", &args(&["ls", "-c", "x"])),
		None,
		"not a shell"
	);
	assert_eq!(script_of("", &args(&["", "-c", "x"])), None);
}

#[test]
fn the_environment_is_the_callers_strings_as_a_map() {
	let vars = environment(&args(&["A=1", "B=two words", "C=", "BROKEN", "A=last"]));
	assert_eq!(vars["A"], "last");
	assert_eq!(vars["B"], "two words");
	assert_eq!(vars["C"], "");
	assert!(!vars.contains_key("BROKEN"));
}

/// Runs `line` as `sh -c` would, from `dir`, and returns its status, what
/// it wrote to standard output and to standard error.
fn run_line(line: &str, dir: &Path, env: &[&str]) -> (Option<c_int>, String, String) {
	run_call("/bin/sh", &["sh", "-c", line], dir, env)
}

/// Runs the program call `file` with `words`, as `run_line` does.
fn run_call(
	file: &str,
	words: &[&str],
	dir: &Path,
	env: &[&str],
) -> (Option<c_int>, String, String) {
	let (mut out_reader, out_writer) = std::io::pipe().unwrap();
	let (mut err_reader, err_writer) = std::io::pipe().unwrap();
	let (input, _unused) = std::io::pipe().unwrap();
	let stdio = Stdio {
		input: input.as_fd(),
		output: out_writer.as_fd(),
		error: err_writer.as_fd(),
		tty: None,
		device: None,
		keyboard: false,
		keys: None,
	};
	// The directory is the process's: put it back when done.
	let previous = std::env::current_dir().unwrap();
	std::env::set_current_dir(dir).unwrap();
	let status = run(file, &args(words), &args(env), &stdio);
	std::env::set_current_dir(previous).unwrap();
	drop((out_writer, err_writer));
	let (mut out, mut err) = (String::new(), String::new());
	out_reader.read_to_string(&mut out).unwrap();
	err_reader.read_to_string(&mut err).unwrap();
	(status, out, err)
}

#[test]
fn a_line_runs_in_the_callers_directory_with_its_environment_and_status() {
	let dir = std::env::temp_dir().join(format!("poopterm-script-{}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	let dir = dir.canonicalize().unwrap();
	// What make execs for a recipe without shell characters.
	let (status, out, _) = run_call("echo", &["echo", "a", "$HOME", "*"], &dir, &["HOME=/"]);
	assert_eq!((status, out.as_str()), (Some(0), "a $HOME *\n"));
	let (status, ..) = run_call("false", &["false"], &dir, &[]);
	assert_eq!(status, Some(1));
	let (status, out, _) = run_line("pwd; echo $GREETING", &dir, &["GREETING=hello", "PATH="]);
	assert_eq!(status, Some(0));
	assert_eq!(out, format!("{}\nhello\n", dir.display()));
	let (status, out, _) = run_line("echo to-file > made.txt; cd /; pwd", &dir, &["PATH="]);
	assert_eq!((status, out.as_str()), (Some(0), "/\n"));
	assert_eq!(
		fs::read_to_string(dir.join("made.txt")).unwrap(),
		"to-file\n"
	);
	let (status, _, _) = run_line("exit 9", &dir, &[]);
	assert_eq!(status, Some(9), "the line's own exit status");
	let (status, _, error) = run_line("nosuchprogram", &dir, &["PATH=/nowhere"]);
	assert_eq!(status, Some(127));
	assert!(error.is_empty() || error.contains("not found"));
	let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_call_that_is_no_shell_line_is_left_to_the_caller() {
	let (input, _unused) = std::io::pipe().unwrap();
	let (_reader, writer) = std::io::pipe().unwrap();
	let stdio = Stdio {
		input: input.as_fd(),
		output: writer.as_fd(),
		error: writer.as_fd(),
		tty: None,
		device: None,
		keyboard: false,
		keys: None,
	};
	assert_eq!(run("ls", &args(&["ls", "-l"]), &[], &stdio), None);
}
