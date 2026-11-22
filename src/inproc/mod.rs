//! A terminal device inside the process, for platforms that let a
//! terminal run no other program (iOS): what the user types goes through
//! a line discipline to a built-in shell on a thread of its own, and what
//! the shell prints comes back as a PTY's output would. Sessions read and
//! write it through two pipes, as they would a PTY's master.
pub(crate) mod line;
pub(crate) mod native;
pub(crate) mod pipeline;
pub(crate) mod shell;
#[cfg(test)]
mod tests;

use std::{
	io::{self, PipeReader, PipeWriter, Read, Write},
	path::{Path, PathBuf},
	sync::{
		Arc,
		atomic::{AtomicU32, Ordering},
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

/// A terminal's columns and rows, set by the session as its window
/// changes and read by the shell before each line.
#[derive(Debug)]
pub(crate) struct Size(AtomicU32);

impl Size {
	pub(crate) fn new(cols: u16, rows: u16) -> Self {
		Self(AtomicU32::new(u32::from(cols) << 16 | u32::from(rows)))
	}

	/// Sets the size, each side clamped to what a terminal reports.
	pub(crate) fn set(&self, cols: usize, rows: usize) {
		let clamp = |n: usize| u32::try_from(n.min(usize::from(u16::MAX))).unwrap_or(0);
		self
			.0
			.store(clamp(cols) << 16 | clamp(rows), Ordering::Relaxed);
	}

	pub(crate) fn get(&self) -> (u16, u16) {
		let packed = self.0.load(Ordering::Relaxed);
		((packed >> 16) as u16, packed as u16)
	}
}

/// Where the shell looks for programs: `POOPTERM_COMMANDS` when set, else
/// the `commands` directory beside the app's binary (bundled with the app,
/// so signed with it on iOS), then the packages installed under `home`.
pub(crate) fn command_path(exe_dir: Option<&Path>, home: &Path, chosen: Option<&str>) -> String {
	if let Some(chosen) = chosen {
		return chosen.to_owned();
	}
	let packages = home.join("Library/poopterm/packages/bin");
	exe_dir
		.map(|dir| dir.join("commands"))
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
		.spawn(move || run(shell, &shared, input_reader, output_writer))?;
	Ok(Device {
		input,
		output,
		size,
	})
}

/// The device's loop: shows the prompt, then handles input until `exit`,
/// `^D` or the input end closing; closing the output ends the session.
pub(crate) fn run(mut shell: Shell, size: &Size, mut input: impl Read, mut output: impl Write) {
	let mut discipline = Discipline::default();
	let show = |output: &mut dyn Write, bytes: &[u8]| output.write_all(&line::output(bytes)).is_ok();
	let welcome = "Poopterm in-process shell. Type `help` for its built-in commands.\n";
	if !show(&mut output, welcome.as_bytes()) || !show(&mut output, shell.prompt().as_bytes()) {
		return;
	}
	let mut buf = [0u8; 4096];
	loop {
		let n = match input.read(&mut buf) {
			Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
			Ok(0) | Err(_) => return,
			Ok(n) => n,
		};
		for event in discipline.input(&buf[..n]) {
			let shown = match event {
				Event::Echo(bytes) => output.write_all(&bytes).is_ok(),
				Event::Line(text) => {
					shell.size = size.get();
					let ran = shell.run(&text);
					if !show(&mut output, &ran.output) || ran.exit {
						return;
					}
					show(&mut output, shell.prompt().as_bytes())
				}
				Event::Interrupt => {
					shell.status = 130;
					show(&mut output, shell.prompt().as_bytes())
				}
				Event::Eof => return,
			};
			if !shown {
				return;
			}
		}
	}
}
