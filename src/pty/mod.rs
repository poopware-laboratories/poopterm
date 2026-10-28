//! PTY sessions: shell spawning plus each session's reader and parser threads.
#[cfg(any(target_os = "macos", test))]
mod burst;
mod pump;
mod queue;
mod writer;

use std::{
	error::Error,
	io::Read,
	path::PathBuf,
	sync::{Arc, atomic::Ordering},
	thread,
};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use winit::event_loop::EventLoopProxy;

use crate::{
	config::Preferences,
	session_store::default_cwd,
	term::{SharedTerm, Term, TermGuard},
	terminfo::TermEnv,
	theme::Theme,
};

pub(crate) use queue::OutputQueue;
use writer::PtyWriter;

/// Bytes a session may buffer before its reader waits for the parser thread.
const OUTPUT_LIMIT: usize = 1 << 20;
const READ_SIZE: usize = 1 << 16;

#[derive(Debug)]
pub(crate) enum UserEvent {
	/// The parser thread applied new output to this session's terminal.
	Output(u64),
	/// The shell exited and all of its output has been parsed.
	Closed(u64),
	/// The desktop reported on the notification shown with this key.
	Notification(u64, crate::term::notifications::Event),
}

pub(crate) struct Session {
	pub(crate) id: u64,
	term: Arc<SharedTerm>,
	flags: Arc<pump::Flags>,
	/// Tab-relevant state the UI last saw, to detect changes.
	pub(crate) seen: Seen,
	pub(crate) custom_title: Option<String>,
	pub(crate) master: Box<dyn MasterPty + Send>,
	writer: PtyWriter,
	output: Arc<OutputQueue>,
	pub(crate) _child: Box<dyn Child + Send + Sync>,
	pub(crate) activity: bool,
}

/// Generations of terminal state shown outside the grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Seen {
	pub bell: bool,
	pub title: u64,
	pub cwd: u64,
	pub status: u64,
}

impl Seen {
	pub(crate) fn of(term: &Term) -> Self {
		Self {
			bell: term.bell,
			title: term.title_generation,
			cwd: term.cwd_generation,
			status: term.status.generation,
		}
	}
}

impl Drop for Session {
	fn drop(&mut self) {
		// Stop the parser thread and unblock the reader if the queue is full.
		self.output.close();
	}
}

impl Session {
	/// Sends input bytes to the shell.
	pub(crate) fn write(&self, bytes: &[u8]) {
		self.writer.write(bytes);
	}

	/// Locks the terminal. Keep the guard short-lived: the parser thread
	/// waits while it is held, and locking again on this thread panics.
	pub(crate) fn term(&self) -> TermGuard<'_> {
		self.term.lock()
	}

	/// Re-arms the output wake-up. Call before reading the terminal after
	/// [`UserEvent::Output`], so output parsed meanwhile sends a new one.
	pub(crate) fn acknowledge_output(&self) {
		self.flags.notified.store(false, Ordering::Release);
	}

	/// Whether output is arriving faster than it can be parsed.
	pub(crate) fn flooding(&self) -> bool {
		self.flags.flooding.load(Ordering::Acquire)
	}

	/// The tab title: a user-chosen name, else one a program gave the tab
	/// (ConEmu's OSC 9;3), else the title set by the shell (OSC 0/2).
	pub(crate) fn title(&self) -> String {
		match &self.custom_title {
			Some(title) => title.clone(),
			None => {
				let term = self.term();
				term.tab_title.clone().unwrap_or_else(|| term.title.clone())
			}
		}
	}
}

pub(crate) fn spawn_session(
	id: u64,
	cols: usize,
	rows: usize,
	theme: Theme,
	proxy: EventLoopProxy<UserEvent>,
	cwd: PathBuf,
	term_env: &TermEnv,
) -> Result<Session, Box<dyn Error>> {
	let cwd = if cwd.is_dir() { cwd } else { default_cwd() };
	let pty = native_pty_system();
	let pair = pty.openpty(PtySize {
		rows: rows as u16,
		cols: cols as u16,
		pixel_width: 0,
		pixel_height: 0,
	})?;
	let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
	let mut cmd = CommandBuilder::new(&shell);
	cmd.env("TERM", &term_env.term);
	for (name, value) in &term_env.extra {
		cmd.env(name, value);
	}
	cmd.env("COLORTERM", "truecolor");
	cmd.env("TERM_PROGRAM", "poopterm");
	cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
	cmd.args(crate::shell_integration::inject::shell_args(
		crate::shell_integration::inject::Shell::from_path(&shell),
		&term_env.extra,
	));
	cmd.cwd(&cwd);
	let child = pair.slave.spawn_command(cmd)?;
	drop(pair.slave);
	#[cfg(target_os = "linux")]
	if let Some(pid) = child.process_id() {
		crate::cgroup::place(pid);
	}

	let mut reader = pair.master.try_clone_reader()?;
	let writer = PtyWriter::spawn(pair.master.take_writer()?);
	let output = Arc::new(OutputQueue::new(OUTPUT_LIMIT));
	let mut term = Term::new(cols, rows, theme, cwd);
	term.scrollback_limit = Preferences::load().scrollback_lines;
	let term = Arc::new(SharedTerm::new(term));
	let flags = Arc::new(pump::Flags::default());

	let queue = output.clone();
	thread::Builder::new()
		.name(format!("pty-reader-{id}"))
		.spawn(move || {
			let mut buf = vec![0u8; READ_SIZE];
			loop {
				match reader.read(&mut buf) {
					Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
					Ok(0) | Err(_) => break,
					Ok(n) => {
						queue.push(&buf[..n]);
					}
				}
			}
			queue.close();
		})?;

	let (queue, shared, flag, replies) =
		(output.clone(), term.clone(), flags.clone(), writer.clone());
	thread::Builder::new()
		.name(format!("pty-parser-{id}"))
		.spawn(move || {
			let mut wake = || {
				let _ = proxy.send_event(UserEvent::Output(id));
			};
			pump::pump(
				&queue,
				&shared,
				&flag,
				|bytes| replies.write(bytes),
				&mut wake,
			);
			let _ = proxy.send_event(UserEvent::Closed(id));
		})?;

	Ok(Session {
		id,
		term,
		flags,
		seen: Seen::default(),
		custom_title: None,
		master: pair.master,
		writer,
		output,
		_child: child,
		activity: false,
	})
}
