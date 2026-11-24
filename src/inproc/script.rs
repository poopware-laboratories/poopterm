//! `sh -c "line"`, for programs that start a shell to run a line (make's
//! recipes, `system`, git's aliases): no shell program exists to start,
//! so the line runs in the app's own shell, on the descriptors the caller
//! gave, in the caller's directory and environment. What it prints is
//! collected and written when it ends.
use std::{
	collections::BTreeMap,
	ffi::{CString, c_int},
	io::Write,
	path::{Path, PathBuf},
};

use super::shell::Shell;
use crate::sys::command::Stdio;

/// The shells a program may ask for: they all run the same shell here.
const SHELLS: [&str; 4] = ["sh", "bash", "dash", "zsh"];

/// The built-ins a program may exec by name, as `/bin/echo` would run.
const EXECUTABLE_BUILTINS: [&str; 5] = ["echo", "pwd", "true", "false", "clear"];

/// `word` as one shell word, in single quotes.
fn quote(word: &str) -> String {
	format!("'{}'", word.replace('\'', "'\\''"))
}

/// The line that runs the built-in `file` names, from a plain name (a path
/// names a file, not a built-in) with its `argv` words.
pub(crate) fn builtin_line(file: &str, argv: &[CString]) -> Option<String> {
	EXECUTABLE_BUILTINS.contains(&file).then_some(())?;
	let mut line = file.to_owned();
	for word in argv.get(1..)? {
		line.push(' ');
		line.push_str(&quote(word.to_str().ok()?));
	}
	Some(line)
}

/// The line of `sh -c LINE` (flags like `-ec` count), if `argv` is one.
pub(crate) fn script_of<'a>(file: &str, argv: &'a [CString]) -> Option<&'a str> {
	let name = Path::new(file).file_name()?.to_str()?;
	if !SHELLS.contains(&name) {
		return None;
	}
	let [_, flags, script, ..] = argv else {
		return None;
	};
	let flags = flags.to_bytes();
	(flags.starts_with(b"-") && flags[1..].contains(&b'c')).then_some(())?;
	script.to_str().ok()
}

/// The `NAME=value` strings of `env` as a map.
pub(crate) fn environment(env: &[CString]) -> BTreeMap<String, String> {
	env
		.iter()
		.filter_map(|var| var.to_str().ok()?.split_once('='))
		.map(|(name, value)| (name.to_owned(), value.to_owned()))
		.collect()
}

/// Runs `sh -c` of `argv` as a shell would: its exit status, or None when
/// `file` and `argv` are no such call.
pub(crate) fn run(file: &str, argv: &[CString], env: &[CString], stdio: &Stdio) -> Option<c_int> {
	let line = builtin_line(file, argv);
	let script = match &line {
		Some(line) => line,
		None => script_of(file, argv)?,
	};
	let cwd = std::env::current_dir().ok()?;
	let vars = environment(env);
	let home = vars.get("HOME").map_or_else(|| cwd.clone(), PathBuf::from);
	let mut shell = Shell::new(home);
	shell.cwd = cwd;
	shell.env = vars;
	let ran = shell.run(script);
	let mut output = std::fs::File::from(stdio.output.try_clone_to_owned().ok()?);
	let mut error = std::fs::File::from(stdio.error.try_clone_to_owned().ok()?);
	// A caller that stopped reading closes its end: nothing more to say.
	let _ = output.write_all(&ran.output);
	let _ = error.write_all(&ran.errors);
	Some(shell.status & 0xff)
}

#[cfg(test)]
#[path = "script_tests.rs"]
mod tests;
