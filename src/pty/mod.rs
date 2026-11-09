//! PTY sessions: shell spawning plus each session's reader and parser threads.
#[cfg(any(target_os = "macos", test))]
mod burst;
pub(crate) mod device;
mod exits;
pub(crate) mod pump;
mod queue;
pub(crate) mod reader;
pub(crate) mod reap;
pub(crate) mod spawn;
pub(crate) mod termios;
mod writer;

use std::{
	error::Error,
	fs::File,
	os::fd::OwnedFd,
	path::{Path, PathBuf},
	sync::{Arc, atomic::Ordering},
	thread,
};

use winit::event_loop::EventLoopProxy;

use device::WindowSize;
use spawn::{Child, Command};

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
pub(crate) const OUTPUT_LIMIT: usize = 1 << 20;

#[derive(Debug)]
pub(crate) enum UserEvent {
	/// The parser thread applied new output to this session's terminal.
	Output(u64),
	/// The shell exited and all of its output has been parsed.
	Closed(u64),
	/// The desktop reported on the notification shown with this key.
	Notification(u64, crate::term::notifications::Event),
	/// The desktop said which window buttons it wants.
	#[cfg(any(target_os = "linux", target_os = "freebsd"))]
	ButtonLayout(crate::ui::window_controls::ButtonLayout),
	/// The app menu's About item was chosen.
	#[cfg(target_os = "macos")]
	About,
}

pub(crate) struct Session {
	pub(crate) id: u64,
	term: Arc<SharedTerm>,
	flags: Arc<pump::Flags>,
	/// Tab-relevant state the UI last saw, to detect changes.
	pub(crate) seen: Seen,
	pub(crate) custom_title: Option<String>,
	master: OwnedFd,
	writer: PtyWriter,
	output: Arc<OutputQueue>,
	/// The shell, until the session ends and hands it to the reaper.
	child: Option<Child>,
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
		// Hang up as a closing terminal does, and reap the shell once it
		// is gone, without waiting for it here.
		if let Some(child) = self.child.take() {
			let _ = child.hang_up();
			reap::reap_later(child);
		}
	}
}

impl Session {
	/// The shell's process id.
	pub(crate) fn pid(&self) -> Option<u32> {
		self.child.as_ref().map(Child::id)
	}

	/// Tells the PTY, and so the shell, its new size in cells.
	pub(crate) fn resize(&self, cols: usize, rows: usize) {
		let _ = device::set_size(&self.master, WindowSize::of(cols, rows, (0, 0)));
	}

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
	let pair = device::open(WindowSize::of(cols, rows, (0, 0)))?;
	let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
	let child = spawn::spawn(&shell_command(&shell, term_env, &cwd), &pair.slave)?;
	// The shell holds the slave now; ours would keep the PTY open after it
	// exits, and the reader would never see the end.
	drop(pair.slave);
	#[cfg(target_os = "linux")]
	crate::cgroup::place(child.id());

	let reader = File::from(pair.master.try_clone()?);
	let writer = PtyWriter::spawn(Box::new(File::from(pair.master.try_clone()?)));
	let output = Arc::new(OutputQueue::new(OUTPUT_LIMIT));
	let mut term = Term::new(cols, rows, theme, cwd);
	term.scrollback_limit = Preferences::load().scrollback_lines;
	let term = Arc::new(SharedTerm::new(term));
	let flags = Arc::new(pump::Flags::default());

	let queue = output.clone();
	thread::Builder::new()
		.name(format!("pty-reader-{id}"))
		.spawn(move || reader::forward(reader, &queue))?;

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
		child: Some(child),
		activity: false,
	})
}

/// How a tab starts `shell` in `cwd`: with its integration arguments, and
/// the terminal's own variables over the environment we were given.
fn shell_command(shell: &str, term_env: &TermEnv, cwd: &Path) -> Command {
	let integration = crate::shell_integration::inject::Shell::from_path(shell);
	let args = crate::shell_integration::inject::shell_args(integration, &term_env.extra);
	let mut command = Command::new(shell).env("TERM", &term_env.term);
	for (name, value) in &term_env.extra {
		command = command.env(name, value);
	}
	for arg in args {
		command = command.arg(arg);
	}
	command
		.env("COLORTERM", "truecolor")
		.env("TERM_PROGRAM", "poopterm")
		.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"))
		.cwd(cwd)
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
