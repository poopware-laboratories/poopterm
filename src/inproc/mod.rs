//! A terminal device inside the process, for platforms that let a
//! terminal run no other program (iOS): what the user types goes through
//! a line discipline to a built-in shell on a thread of its own, or to the
//! program it runs (`terminal`), and what they print comes back as a PTY's
//! output would. Sessions read and write it through two pipes, as they
//! would a PTY's master.
pub(crate) mod line;
pub(crate) mod native;
pub(crate) mod pipeline;
pub(crate) mod shell;
pub(crate) mod terminal;
#[cfg(test)]
mod tests;
pub(crate) mod tty;

use std::{
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

/// Starts the device's thread, its shell in `cwd`, with `home` for `~`, on
/// a terminal of `cols`×`rows`.
pub(crate) fn spawn(
	id: u64,
	cwd: PathBuf,
	home: PathBuf,
	(cols, rows): (usize, usize),
) -> io::Result<Device> {
	// ^C reaches a program as SIGINT on the shell's thread; none of the
	// signals a terminal sends may end the app itself.
	crate::sys::signal::quiet_job_signals();
	let (input_reader, input) = io::pipe()?;
	let (output, output_writer) = io::pipe()?;
	let size = Arc::new(Size::new(0, 0));
	size.set(cols, rows);
	let exe = std::env::current_exe().ok();
	let chosen = std::env::var("POOPTERM_COMMANDS").ok();
	let path = command_path(
		exe.as_deref().and_then(Path::parent),
		&home,
		chosen.as_deref(),
	);
	let mut shell = Shell::new(home);
	shell.cwd = cwd;
	shell.env.insert("PATH".to_owned(), path);
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
	let welcome = "Poopterm in-process shell. Type `help` for its built-in commands.\n";
	if !show(welcome.as_bytes()) || !show(shell.prompt().as_bytes()) {
		return;
	}
	for bytes in typed {
		for event in discipline.input(&bytes) {
			let shown = match event {
				Event::Echo(bytes) => terminal.write_raw(&bytes).is_ok(),
				Event::Line(text) => {
					shell.size = size.get();
					let ran = shell.run(&text);
					if !show(&ran.output) || !show(&ran.errors) || ran.exit {
						return;
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
