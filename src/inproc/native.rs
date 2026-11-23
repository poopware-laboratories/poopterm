//! Real programs for the in-process shell: `uname`, `ls`, `cat`… built from
//! their own sources as command libraries (`ios/commands`), found on
//! `$PATH` and run inside this process, one at a time, since the standard
//! descriptors, environment and working directory they use are the
//! process's.
use std::{
	collections::BTreeMap,
	ffi::CString,
	fs::File,
	io::{self, Read, Write},
	os::fd::{AsFd, OwnedFd},
	path::{Path, PathBuf},
	sync::{Arc, Mutex},
	thread,
};

use super::terminal::Terminal;
use crate::sys::command::{self, Device, Stdio};
#[cfg(not(target_vendor = "apple"))]
use crate::sys::dl::Library;

/// Most bytes kept of what one command prints; the rest is read and
/// dropped, so `yes` cannot fill memory or block forever.
pub(crate) const MAX_OUTPUT: usize = 16 << 20;

/// Commands share the process's descriptors, environment and directory.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// The command library `name` stands for: a path when it has a `/` (from
/// `cwd`), else the first file of that name in a directory of `path`
/// (`$PATH`, colon-separated).
pub(crate) fn find(name: &str, path: Option<&str>, cwd: &Path) -> Option<PathBuf> {
	if name.is_empty() {
		return None;
	}
	if name.contains('/') {
		let file = cwd.join(name);
		return file.is_file().then_some(file);
	}
	path?
		.split(':')
		.filter(|dir| !dir.is_empty())
		.find_map(|dir| {
			let directory = Path::new(dir);
			let framework = directory.join(format!("{name}.framework")).join(name);
			if framework.is_file() {
				Some(framework)
			} else {
				let file = directory.join(name);
				file.is_file().then_some(file)
			}
		})
}

/// How a command ran: its status and what it printed.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Ran {
	pub status: i32,
	pub output: Vec<u8>,
	/// Standard error, when kept apart (`merge` false); otherwise in `output`.
	pub errors: Vec<u8>,
}

/// What a command is run with.
pub(crate) struct Run<'a> {
	pub argv: &'a [String],
	pub env: &'a BTreeMap<String, String>,
	pub cwd: &'a Path,
	/// Standard input, or None for an empty one.
	pub input: Option<&'a [u8]>,
	/// The terminal's columns and rows when output goes to it, None when
	/// it goes to a file.
	pub tty: Option<(u16, u16)>,
	/// Standard error with standard output, in the order written.
	pub merge: bool,
	/// The device's terminal, if any: output that goes to the terminal
	/// (`tty`) is shown as it is printed, an input not given is the
	/// keyboard, and the program is in the foreground (`^C` interrupts it).
	pub terminal: Option<&'a Arc<Terminal>>,
}

/// Runs the command library at `library`. Standard input is what the run
/// gives (a pipe fed by a thread of its own, so a program that reads less
/// than all of it still ends), else the keyboard when there is a terminal,
/// else empty.
pub(crate) fn run(library: &Path, run: &Run) -> Result<Ran, String> {
	let argv = c_strings(run.argv.iter().map(String::as_str))?;
	let pwd = format!("PWD={}", run.cwd.display());
	let vars: Vec<String> = run
		.env
		.iter()
		.map(|(key, value)| format!("{key}={value}"))
		.collect();
	let env = c_strings(vars.iter().map(String::as_str).chain([pwd.as_str()]))?;
	let _guard = ONE_AT_A_TIME
		.lock()
		.unwrap_or_else(|poison| poison.into_inner());
	// Opened for each run and closed after (or, loaded by us, its data put
	// back), so its globals start over.
	let library = open(library)?;
	let entry = library
		.entry()
		.ok_or_else(|| "not a command library (no poopterm_entry)".to_owned())?;
	let failed = |error: io::Error| error.to_string();
	let terminal = run.terminal;
	let keys = match terminal {
		Some(terminal) => Some(terminal.attach().map_err(failed)?),
		None => None,
	};
	let foreground = Foreground(terminal, terminal.map(|terminal| terminal.settings()));
	let stdin: OwnedFd = match (run.input, &keys) {
		(Some(bytes), _) => {
			let (reader, mut writer) = io::pipe().map_err(failed)?;
			let bytes = bytes.to_vec();
			// A reader that stops early closes its end; the write then fails.
			thread::spawn(move || drop(writer.write_all(&bytes)));
			reader.into()
		}
		(None, Some(keys)) => keys.try_clone().map_err(failed)?.into(),
		(None, None) => File::open("/dev/null").map_err(failed)?.into(),
	};
	let (out_reader, out_writer) = io::pipe().map_err(failed)?;
	let (err_reader, err_writer) = io::pipe().map_err(failed)?;
	let shown = terminal.filter(|_| run.tty.is_some()).cloned();
	let out = thread::spawn(move || match shown {
		Some(terminal) => show(out_reader, &terminal),
		None => drain(out_reader),
	});
	let err = thread::spawn(move || drain(err_reader));
	let previous = std::env::current_dir().ok();
	std::env::set_current_dir(run.cwd).map_err(|error| format!("{}: {error}", run.cwd.display()))?;
	let stdio = Stdio {
		input: stdin.as_fd(),
		output: out_writer.as_fd(),
		error: if run.merge {
			out_writer.as_fd()
		} else {
			err_writer.as_fd()
		},
		tty: run.tty,
		device: terminal.map(|terminal| &**terminal as &dyn Device),
		keyboard: run.input.is_none() && terminal.is_some(),
		keys: keys.as_ref().map(AsFd::as_fd),
	};
	let status = command::call_entry(entry, &argv, &env, &stdio);
	drop((out_writer, err_writer, stdin, keys));
	drop(foreground);
	if let Some(dir) = previous {
		let _ = std::env::set_current_dir(dir);
	}
	let output = out.join().unwrap_or_default();
	let errors = err.join().unwrap_or_default();
	let status = status.ok_or_else(|| "too many arguments".to_owned())?;
	Ok(Ran {
		status,
		output,
		errors,
	})
}

/// Every command goes through our loader, not dyld (`POOPTERM_LOADER=1`
/// does the same): for testing the loader on real programs.
#[cfg(target_vendor = "apple")]
pub(crate) static FORCE_LOADER: std::sync::atomic::AtomicBool =
	std::sync::atomic::AtomicBool::new(false);

/// A command library opened for a run: by dyld, and where JIT allows, by
/// our loader when dyld refuses it (unsigned, downloaded by `pkg` on iOS).
#[cfg(target_vendor = "apple")]
pub(crate) fn open(path: &Path) -> Result<crate::jit::Opened, String> {
	let force = FORCE_LOADER.load(std::sync::atomic::Ordering::Relaxed)
		|| std::env::var_os("POOPTERM_LOADER").is_some_and(|value| value == "1");
	crate::jit::open(path, force)
}

/// A command library opened for a run, by the dynamic linker.
#[cfg(not(target_vendor = "apple"))]
pub(crate) fn open(path: &Path) -> Result<Library, String> {
	let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
		.map_err(|_| "a path with a NUL byte".to_owned())?;
	Library::open(&name)
}

/// Arguments or variables as C strings; one with a NUL byte cannot be.
fn c_strings<'a>(words: impl Iterator<Item = &'a str>) -> Result<Vec<CString>, String> {
	words
		.map(|word| CString::new(word).map_err(|_| format!("{word:?}: has a NUL byte")))
		.collect()
}

/// While a program runs in the terminal's foreground; its end gives the
/// keys back to the shell. The settings it leaves stay (`stty -echo` must
/// last), unless `^C` ended it before it could put its own back.
struct Foreground<'a>(Option<&'a Arc<Terminal>>, Option<libc::termios>);

impl Drop for Foreground<'_> {
	fn drop(&mut self) {
		if let Some(terminal) = self.0 {
			terminal.detach();
			if let Some(settings) = self.1.as_ref().filter(|_| terminal.interrupted()) {
				terminal.set_settings(settings, false);
			}
		}
	}
}

/// Shows everything until the end on `terminal` as it comes; nothing is
/// kept.
fn show(mut reader: impl Read, terminal: &Terminal) -> Vec<u8> {
	let mut buf = [0u8; 16 << 10];
	loop {
		match reader.read(&mut buf) {
			Ok(0) => return Vec::new(),
			Ok(n) => {
				// A screen gone still has the program's output read, so it
				// ends instead of blocking.
				let _ = terminal.write(&buf[..n]);
			}
			Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
			Err(_) => return Vec::new(),
		}
	}
}

/// Everything until the end, keeping the first `MAX_OUTPUT` bytes.
pub(super) fn drain(mut reader: impl Read) -> Vec<u8> {
	let mut kept = Vec::new();
	let mut buf = [0u8; 16 << 10];
	loop {
		match reader.read(&mut buf) {
			Ok(0) => return kept,
			Ok(n) => {
				let room = MAX_OUTPUT.saturating_sub(kept.len());
				kept.extend_from_slice(&buf[..n.min(room)]);
			}
			Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
			Err(_) => return kept,
		}
	}
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
