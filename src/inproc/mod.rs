//! A terminal device inside the process, for platforms that let a
//! terminal run no other program (iOS): what the user types goes through
//! a line discipline to a built-in shell on a thread of its own, and what
//! the shell prints comes back as a PTY's output would. Sessions read and
//! write it through two pipes, as they would a PTY's master.
pub(crate) mod line;
pub(crate) mod shell;
#[cfg(test)]
mod tests;

use std::{
	io::{self, PipeReader, PipeWriter, Read, Write},
	path::PathBuf,
	thread,
};

use line::{Discipline, Event};
use shell::Shell;

/// The two ends a session uses: where it writes input, and where it reads
/// output. The shell runs until `exit`, or until the input end is closed.
pub(crate) struct Device {
	pub input: PipeWriter,
	pub output: PipeReader,
}

/// Starts the device's thread, its shell in `cwd`, with `home` for `~`.
pub(crate) fn spawn(id: u64, cwd: PathBuf, home: PathBuf) -> io::Result<Device> {
	let (input_reader, input) = io::pipe()?;
	let (output, output_writer) = io::pipe()?;
	let mut shell = Shell::new(home);
	shell.cwd = cwd;
	thread::Builder::new()
		.name(format!("inproc-shell-{id}"))
		.spawn(move || run(shell, input_reader, output_writer))?;
	Ok(Device { input, output })
}

/// The device's loop: shows the prompt, then handles input until `exit`,
/// `^D` or the input end closing; closing the output ends the session.
pub(crate) fn run(mut shell: Shell, mut input: impl Read, mut output: impl Write) {
	let mut discipline = Discipline::default();
	let show = |output: &mut dyn Write, bytes: &[u8]| output.write_all(&line::output(bytes)).is_ok();
	let welcome = "Poopterm in-process shell. Type `help` for its commands.\n";
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
