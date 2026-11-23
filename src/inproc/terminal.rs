//! The in-process device's terminal, shared by the thread reading the
//! keyboard and the shell's thread running programs: what a program
//! prints reaches the screen as it prints it, keys reach the program
//! running (through `tty`'s line discipline, under its own `termios`
//! settings), `^C` interrupts it, and a resize tells it so.
use std::{
	fmt,
	io::{self, PipeReader, PipeWriter, Write},
	sync::{
		Arc, Mutex, MutexGuard,
		atomic::{AtomicBool, Ordering},
	},
};

use super::{
	Size,
	tty::{self, Event, Line, Mode},
};
use crate::sys::{command::Device, signal::Thread};

/// The program running in the foreground, while it runs.
struct Foreground {
	/// The write end of its keyboard (its standard input, or `/dev/tty`);
	/// none once the user ended input with `^D`.
	keys: Option<PipeWriter>,
	/// The thread running it, which `^C` and resizes signal.
	thread: Thread,
	line: Line,
}

pub(crate) struct Terminal {
	output: Mutex<Box<dyn Write + Send>>,
	settings: Mutex<libc::termios>,
	foreground: Mutex<Option<Foreground>>,
	size: Arc<Size>,
	/// `^C` interrupted the program in the foreground: the rest of the
	/// line the shell was running stops, as a shell's does.
	interrupted: AtomicBool,
}

impl fmt::Debug for Terminal {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("Terminal")
	}
}

/// Locks `mutex`, through a panic of another thread that held it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

impl Terminal {
	/// A terminal writing to `output`, `size` big, with a login terminal's
	/// settings; resizing `size` tells the program in the foreground.
	pub(crate) fn new(output: Box<dyn Write + Send>, size: Arc<Size>) -> Arc<Self> {
		let terminal = Arc::new(Self {
			output: Mutex::new(output),
			settings: Mutex::new(crate::sys::termios::fresh()),
			foreground: Mutex::new(None),
			size: size.clone(),
			interrupted: AtomicBool::new(false),
		});
		let weak = Arc::downgrade(&terminal);
		size.on_resize(Box::new(move || {
			if let Some(terminal) = weak.upgrade() {
				terminal.signal(libc::SIGWINCH);
			}
		}));
		terminal
	}

	/// The line discipline's settings now.
	pub(crate) fn mode(&self) -> Mode {
		Mode::of(&lock(&self.settings))
	}

	/// Shows bytes as they are (the shell's own prompt and echo).
	pub(crate) fn write_raw(&self, bytes: &[u8]) -> io::Result<()> {
		let mut output = lock(&self.output);
		output.write_all(bytes)?;
		output.flush()
	}

	/// Shows what a program wrote, as its settings say (`\n` as `\r\n`).
	pub(crate) fn write(&self, bytes: &[u8]) -> io::Result<()> {
		let shown = tty::output(&self.mode(), bytes);
		self.write_raw(&shown)
	}

	/// The program on this thread is now in the foreground: keys go to it
	/// through the returned pipe, and `^C` to this thread. Keys it does not
	/// read are dropped once the pipe is full, so they never hold up `^C`.
	pub(crate) fn attach(&self) -> io::Result<PipeReader> {
		let (reader, keys) = io::pipe()?;
		crate::sys::fd::set_nonblocking(&keys)?;
		self.interrupted.store(false, Ordering::SeqCst);
		*lock(&self.foreground) = Some(Foreground {
			keys: Some(keys),
			thread: Thread::current(),
			line: Line::default(),
		});
		Ok(reader)
	}

	/// The program in the foreground ended; keys go to the shell again.
	pub(crate) fn detach(&self) {
		lock(&self.foreground).take();
	}

	/// Whether `^C` interrupted the last program.
	pub(crate) fn interrupted(&self) -> bool {
		self.interrupted.load(Ordering::SeqCst)
	}

	/// Whether `^C` interrupted the last program, cleared by asking.
	pub(crate) fn take_interrupted(&self) -> bool {
		self.interrupted.swap(false, Ordering::SeqCst)
	}

	/// Keys typed: the program in the foreground gets them, under its
	/// settings, and what the shell should read instead is returned.
	pub(crate) fn keyboard(&self, bytes: &[u8]) -> Option<Vec<u8>> {
		let mode = self.mode();
		let (events, keys) = {
			let mut foreground = lock(&self.foreground);
			let Some(running) = foreground.as_mut() else {
				return Some(bytes.to_vec());
			};
			let events = running.line.feed(&mode, bytes);
			if events.contains(&Event::EndOfFile) {
				running.keys = None;
			}
			// A copy to write through after the lock is let go: a program
			// that reads nothing must not keep the shell from detaching it.
			let keys = running.keys.as_ref().and_then(|keys| keys.try_clone().ok());
			(events, keys)
		};
		for event in events {
			match event {
				Event::Echo(echo) => drop(self.write(&echo)),
				Event::Send(sent) => {
					if let Some(mut keys) = keys.as_ref() {
						// A program that has ended closed its end: dropped.
						drop(keys.write_all(&sent));
					}
				}
				Event::Interrupt => self.interrupt(libc::SIGINT),
				Event::Quit => self.interrupt(libc::SIGQUIT),
				Event::EndOfFile => {}
			}
		}
		None
	}

	/// The keyboard is gone (the tab closed): the program reads the end.
	pub(crate) fn hang_up(&self) {
		if let Some(running) = lock(&self.foreground).as_mut() {
			running.keys = None;
		}
		self.signal(libc::SIGHUP);
	}

	fn interrupt(&self, signal: libc::c_int) {
		self.interrupted.store(true, Ordering::SeqCst);
		self.signal(signal);
	}

	/// Sends `signal` to the program in the foreground, if any.
	fn signal(&self, signal: libc::c_int) {
		if let Some(running) = lock(&self.foreground).as_ref() {
			running.thread.signal(signal);
		}
	}
}

impl Device for Terminal {
	fn settings(&self) -> libc::termios {
		*lock(&self.settings)
	}

	fn set_settings(&self, settings: &libc::termios, flush: bool) {
		let was_canonical = {
			let mut kept = lock(&self.settings);
			let was = kept.c_lflag & libc::ICANON != 0;
			*kept = *settings;
			was
		};
		let going_raw = was_canonical && settings.c_lflag & libc::ICANON == 0;
		if !flush && !going_raw {
			return;
		}
		let (pending, keys) = {
			let mut foreground = lock(&self.foreground);
			let Some(running) = foreground.as_mut() else {
				return;
			};
			let keys = running.keys.as_ref().and_then(|keys| keys.try_clone().ok());
			(running.line.take_pending(), keys)
		};
		// What was typed ahead of a program going raw, it reads now; a flush
		// drops it.
		if !flush
			&& !pending.is_empty()
			&& let Some(mut keys) = keys
		{
			let _ = keys.write_all(&pending);
		}
	}

	fn size(&self) -> (u16, u16) {
		self.size.get()
	}
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;
