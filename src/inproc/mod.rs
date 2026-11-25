//! A terminal device inside the process, for platforms that let a
//! terminal run no other program (iOS): what the user types goes through
//! a line discipline to a built-in shell on a thread of its own, or to the
//! program it runs (`terminal`), and what they print comes back as a PTY's
//! output would. Sessions read and write it through two pipes, as they
//! would a PTY's master.
pub(crate) mod glob;
pub(crate) mod line;
pub(crate) mod native;
pub(crate) mod pipeline;
pub(crate) mod script;
pub(crate) mod shell;
pub(crate) mod sshd;
pub(crate) mod terminal;
#[cfg(test)]
mod tests;
pub(crate) mod tty;

use std::{
	collections::VecDeque,
	io::{self, PipeReader, PipeWriter, Read, Write},
	path::{Path, PathBuf},
	sync::{
		Arc, OnceLock,
		atomic::{AtomicU32, Ordering},
		mpsc,
	},
	thread,
};

use line::{Discipline, Event};
use shell::Shell;

/// How to get the programs the app does not carry, shown when a shell
/// starts and by `help`.
pub(crate) const MORE_PROGRAMS: &str = "\
More programs: `pkg list` shows them and `pkg install NAME` adds one.
On an iPhone or iPad they need JIT: open Poopterm from StikDebug, then `jit enable`.
";

/// The two ends a session uses: where it writes input, and where it reads
/// output, and the window size programs are told. The shell runs until
/// `exit`, or until the input end is closed.
pub(crate) struct Device {
	pub input: PipeWriter,
	pub output: PipeReader,
	pub size: Arc<Size>,
}

/// What a resize calls: the terminal telling its program.
type OnResize = Box<dyn Fn() + Send + Sync>;

/// A terminal's columns and rows, set by the session as its window
/// changes and read by the shell before each line and by programs.
pub(crate) struct Size {
	packed: AtomicU32,
	resized: OnceLock<OnResize>,
}

impl std::fmt::Debug for Size {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("Size").field(&self.get()).finish()
	}
}

impl Size {
	pub(crate) fn new(cols: u16, rows: u16) -> Self {
		Self {
			packed: AtomicU32::new(u32::from(cols) << 16 | u32::from(rows)),
			resized: OnceLock::new(),
		}
	}

	/// Sets the size, each side clamped to what a terminal reports; a
	/// change calls what `on_resize` gave.
	pub(crate) fn set(&self, cols: usize, rows: usize) {
		let clamp = |n: usize| u32::try_from(n.min(usize::from(u16::MAX))).unwrap_or(0);
		let packed = clamp(cols) << 16 | clamp(rows);
		if self.packed.swap(packed, Ordering::Relaxed) != packed
			&& let Some(resized) = self.resized.get()
		{
			resized();
		}
	}

	pub(crate) fn get(&self) -> (u16, u16) {
		let packed = self.packed.load(Ordering::Relaxed);
		((packed >> 16) as u16, packed as u16)
	}

	/// What to call when the size changes; only the first one given stays.
	pub(crate) fn on_resize(&self, resized: OnResize) {
		let _ = self.resized.set(resized);
	}
}

/// The terminal descriptions the app carries (`ios/terminfo.sh`), beside
/// its binary: an iPhone has no `/usr/share/terminfo`, and `less` and the
/// editors cannot start without a description of the terminal. `chosen`
/// is `TERMINFO` when the user set it, which stays.
pub(crate) fn terminfo_dir(exe_dir: Option<&Path>, chosen: Option<&str>) -> Option<PathBuf> {
	if chosen.is_some_and(|dir| !dir.is_empty()) {
		return None;
	}
	let dir = exe_dir?.join("terminfo");
	dir.is_dir().then_some(dir)
}

/// The folder a program finds its data in (`name`, holding `marker`): the
/// app's own `name` folder, else the one its package installs under
/// `support`'s `share`; none when neither exists, or when the user chose
/// one (`chosen`, the program's variable).
pub(crate) fn data_dir(
	exe_dir: Option<&Path>,
	support: &Path,
	(name, marker): (&str, &str),
	chosen: Option<&str>,
) -> Option<PathBuf> {
	if chosen.is_some_and(|dir| !dir.is_empty()) {
		return None;
	}
	let bundled = exe_dir.map(|dir| dir.join(name));
	let installed = support.join("Library/poopterm/packages/share").join(name);
	bundled
		.into_iter()
		.chain([installed])
		.find(|dir| dir.join(marker).exists())
}

/// The folders bundled programs read their data from: the variable each
/// reads, the folder and a file in it that `data_dir` looks for, and
/// whether the variable names that file rather than the folder.
pub(crate) const DATA_DIRS: [(&str, (&str, &str), bool); 4] = [
	// tcc's headers and runtime sources.
	("TCCDIR", ("tcc", "include"), false),
	// FIGlet's fonts.
	("FIGLET_FONTDIR", ("figlet", "standard.flf"), false),
	// file's compiled magic database.
	("MAGIC", ("file", "magic.mgc"), true),
	// Vim's runtime files: syntax highlighting, file types, indenting.
	("VIMRUNTIME", ("vim", "defaults.vim"), false),
];

/// What a `DATA_DIRS` variable is set to, for the folder found: the folder,
/// or the file in it the program reads.
pub(crate) fn data_value(dir: &Path, marker: &str, names_file: bool) -> String {
	let value = if names_file {
		dir.join(marker)
	} else {
		dir.to_path_buf()
	};
	value.display().to_string()
}

/// Where the shell looks for programs: `POOPTERM_COMMANDS` when set, else
/// the app's `Frameworks` directory, then packages installed under `home`.
pub(crate) fn command_path(exe_dir: Option<&Path>, home: &Path, chosen: Option<&str>) -> String {
	if let Some(chosen) = chosen {
		return chosen.to_owned();
	}
	let packages = home.join("Library/poopterm/packages/bin");
	exe_dir
		.map(|dir| dir.join("Frameworks"))
		.into_iter()
		.chain([packages])
		.map(|dir| dir.display().to_string())
		.collect::<Vec<_>>()
		.join(":")
}

/// Where a new shell starts: `cwd` when it is inside `home`, else `home`
/// (on iOS the app may write only there, and a tab asked to start in the
/// sandbox's root would find nothing it can use).
pub(crate) fn start_directory(cwd: PathBuf, home: &Path) -> PathBuf {
	if cwd.starts_with(home) {
		cwd
	} else {
		home.to_path_buf()
	}
}

/// A shell as a login gives it: in `cwd` (or `home`, see
/// `start_directory`), with `$PATH` to the commands and the folders the
/// bundled programs look in (`DATA_DIRS`, `TERMINFO`), `support` holding the
/// packages.
pub(crate) fn login_shell(cwd: PathBuf, home: PathBuf, support: PathBuf) -> Shell {
	let exe = std::env::current_exe().ok();
	let chosen = std::env::var("POOPTERM_COMMANDS").ok();
	let path = command_path(
		exe.as_deref().and_then(Path::parent),
		&support,
		chosen.as_deref(),
	);
	let mut shell = Shell::new(home.clone());
	shell.cwd = start_directory(cwd, &home);
	shell.env.insert("PATH".to_owned(), path);
	for (variable, folder, names_file) in DATA_DIRS {
		let chosen = std::env::var(variable).ok();
		let exe_dir = exe.as_deref().and_then(Path::parent);
		if let Some(dir) = data_dir(exe_dir, &support, folder, chosen.as_deref()) {
			let value = data_value(&dir, folder.1, names_file);
			shell.env.insert(variable.to_owned(), value);
		}
	}
	shell.support = support;
	let chosen_terminfo = std::env::var("TERMINFO").ok();
	if let Some(dir) = terminfo_dir(
		exe.as_deref().and_then(Path::parent),
		chosen_terminfo.as_deref(),
	) {
		shell
			.env
			.insert("TERMINFO".to_owned(), dir.display().to_string());
	}
	shell
}

/// Starts the device's thread, its shell in `cwd` (or `home`, see
/// `start_directory`), with `home` for `~` and `support` holding the
/// packages, on a terminal of `cols`×`rows`.
pub(crate) fn spawn(
	id: u64,
	cwd: PathBuf,
	home: PathBuf,
	support: PathBuf,
	size: (usize, usize),
) -> io::Result<Device> {
	spawn_with(id, cwd, home, support, size, &[])
}

/// `spawn`, with `extra` variables set in the shell's environment (the
/// `TERM` an ssh client asked for).
pub(crate) fn spawn_with(
	id: u64,
	cwd: PathBuf,
	home: PathBuf,
	support: PathBuf,
	(cols, rows): (usize, usize),
	extra: &[(String, String)],
) -> io::Result<Device> {
	// ^C reaches a program as SIGINT on the shell's thread; none of the
	// signals a terminal sends may end the app itself.
	crate::sys::signal::quiet_job_signals();
	let (input_reader, input) = io::pipe()?;
	let (output, output_writer) = io::pipe()?;
	let size = Arc::new(Size::new(0, 0));
	size.set(cols, rows);
	let mut shell = login_shell(cwd, home, support);
	shell.env.extend(extra.iter().cloned());
	let shared = size.clone();
	thread::Builder::new()
		.name(format!("inproc-shell-{id}"))
		.spawn(move || run(shell, shared, input_reader, output_writer))?;
	Ok(Device {
		input,
		output,
		size,
	})
}

/// The device's loop: shows the prompt, then handles input until `exit`,
/// `^D` or the input end closing; closing the output ends the session.
/// Keys are read on a thread of their own, so they reach a program while
/// it runs (`terminal`), and what is left goes to the line being edited.
pub(crate) fn run(
	mut shell: Shell,
	size: Arc<Size>,
	input: impl Read + Send + 'static,
	output: impl Write + Send + 'static,
) {
	let terminal = terminal::Terminal::new(Box::new(output), size.clone());
	shell.terminal = Some(terminal.clone());
	let (keys, typed) = mpsc::channel();
	let reader = terminal.clone();
	let _ = thread::Builder::new()
		.name("inproc-keyboard".to_owned())
		.spawn(move || read_keys(input, &reader, &keys));
	let mut discipline = Discipline::default();
	let show = |bytes: &[u8]| terminal.write_raw(&line::output(bytes)).is_ok();
	let welcome =
		format!("Poopterm in-process shell. Type `help` for its built-in commands.\n{MORE_PROGRAMS}");
	if !show(welcome.as_bytes()) || !show(shell.prompt().as_bytes()) {
		return;
	}
	// Keys typed after ^C to a program that has since ended were meant for
	// the shell: they are edited at the next prompt, before newer keys.
	let mut queued: VecDeque<Vec<u8>> = VecDeque::new();
	loop {
		let bytes = match queued.pop_front() {
			Some(bytes) => bytes,
			None => match typed.recv() {
				Ok(bytes) => bytes,
				Err(_) => return,
			},
		};
		for event in discipline.input(&bytes) {
			let shown = match event {
				Event::Echo(bytes) => terminal.write_raw(&bytes).is_ok(),
				Event::Line(text) => {
					shell.size = size.get();
					let ran = shell.run(&text);
					if !show(&ran.output) || !show(&ran.errors) || ran.exit {
						return;
					}
					let leftover = terminal.take_leftover();
					if !leftover.is_empty() {
						queued.push_back(leftover);
					}
					show(shell.prompt().as_bytes())
				}
				Event::Interrupt => {
					shell.status = 130;
					show(shell.prompt().as_bytes())
				}
				Event::Eof => return,
			};
			if !shown {
				return;
			}
		}
	}
}

/// Reads keys until the input ends, giving the program in the foreground
/// its share and sending the rest to the shell; the end hangs up.
fn read_keys(mut input: impl Read, terminal: &terminal::Terminal, keys: &mpsc::Sender<Vec<u8>>) {
	let mut buf = [0u8; 4096];
	loop {
		let n = match input.read(&mut buf) {
			Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
			Ok(0) | Err(_) => break,
			Ok(n) => n,
		};
		if let Some(rest) = terminal.keyboard(&buf[..n])
			&& keys.send(rest).is_err()
		{
			break;
		}
	}
	terminal.hang_up();
}
