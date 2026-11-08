//! Driving the terminal `--benchmark --terminal` runs in, any emulator:
//! writing workloads to it and timing until it answers a Primary Device
//! Attributes query (`CSI c`). Terminals answer in order, only after
//! parsing everything written before the query, so buffering inside the
//! terminal cannot flatter the numbers.
//!
//! Settings go through `stty` on the terminal, as a shell user would set
//! them: echo and line editing off for reading replies, output processing
//! on (cooked) or off (raw) per measurement, and everything restored after.

use std::{
	error::Error,
	fs::File,
	io::Write,
	process::{Command, Stdio},
	sync::mpsc::{self, Receiver},
	thread,
	time::{Duration, Instant},
};

use super::{corpus::Size, pty::Mode, replies};
use crate::pty::reader;

type Outcome<T> = Result<T, Box<dyn Error>>;

/// Bytes per write, the same for every terminal.
const CHUNK: usize = 1 << 16;

/// How long a terminal may take to answer, a whole workload included.
const TIMEOUT: Duration = Duration::from_secs(120);

/// The `stty` setting for each [`Mode`], in its order.
const OUTPUT_PROCESSING: [&str; 2] = ["opost", "-opost"];

/// Ctrl-C, which arrives as a byte while the terminal is raw.
const INTERRUPT: u8 = 0x03;

/// Written when done: deletes kitty images (a screen clear keeps them),
/// resets colors and margins, and clears the screen.
pub(crate) const CLEAR: &[u8] = b"\x1b_Ga=d,q=2\x1b\\\x1b[0m\x1b[r\x1b[2J\x1b[H";

/// A terminal taken over for measuring, given back as it was when dropped.
pub(crate) struct Terminal {
	tty: File,
	replies: Receiver<Vec<u8>>,
	/// Replies read but not yet consumed.
	heard: Vec<u8>,
	/// `stty -g` from before, to restore.
	saved: String,
	pub(crate) timeout: Duration,
}

/// Runs `stty args` on `tty`, returning what it prints.
fn stty(tty: &File, args: &[&str]) -> Outcome<String> {
	let run = Command::new("stty")
		.args(args)
		.stdin(tty.try_clone()?)
		.stderr(Stdio::piped())
		.output()?;
	if !run.status.success() {
		let why = String::from_utf8_lossy(&run.stderr);
		return Err(format!("stty {}: {}", args.join(" "), why.trim()).into());
	}
	Ok(String::from_utf8(run.stdout)?)
}

impl Terminal {
	/// Takes over `tty`: saves its settings, turns echo and line editing
	/// off, and reads its replies on a thread of their own.
	pub(crate) fn open(tty: File) -> Outcome<Self> {
		let saved = stty(&tty, &["-g"])?.trim().to_owned();
		stty(&tty, &["raw", "-echo"])?;
		let (send, replies) = mpsc::channel();
		let input = tty.try_clone()?;
		thread::spawn(move || reader::drain(input, |bytes| drop(send.send(bytes.to_vec()))));
		let heard = Vec::new();
		Ok(Self {
			tty,
			replies,
			heard,
			saved,
			timeout: TIMEOUT,
		})
	}

	/// The terminal's size, as programs see it, within [`Size::bounded`].
	pub(crate) fn size(&self) -> Outcome<Size> {
		let size = stty(&self.tty, &["size"])?;
		// busybox prints nothing for a terminal of no size.
		let (rows, cols) = size.trim().split_once(' ').unwrap_or(("0", "0"));
		Ok(Size::bounded(cols.parse()?, rows.parse()?))
	}

	/// Sends `query` and a Device Attributes query, waits for the answer to
	/// the latter, and returns everything heard before it.
	pub(crate) fn ask(&mut self, query: &[u8]) -> Outcome<Vec<u8>> {
		self.tty.write_all(&[query, b"\x1b[c"].concat())?;
		let deadline = Instant::now() + self.timeout;
		loop {
			if self.heard.contains(&INTERRUPT) {
				return Err("interrupted".into());
			}
			if let Some(reply) = replies::da1(&self.heard) {
				let rest = self.heard.split_off(reply.end);
				self.heard.truncate(reply.start);
				return Ok(std::mem::replace(&mut self.heard, rest));
			}
			let left = deadline.saturating_duration_since(Instant::now());
			let Ok(bytes) = self.replies.recv_timeout(left) else {
				return Err("the terminal did not answer a Device Attributes query (CSI c)".into());
			};
			self.heard.extend_from_slice(&bytes);
		}
	}

	/// MB/s of the terminal taking `data` with output processing as `mode`
	/// sets it, from the first byte written until it has parsed the last.
	pub(crate) fn rate(&mut self, data: &[u8], mode: Mode) -> Outcome<f64> {
		stty(&self.tty, &[OUTPUT_PROCESSING[mode as usize]])?;
		self.ask(b"")?;
		let start = Instant::now();
		for chunk in data.chunks(CHUNK) {
			self.tty.write_all(chunk)?;
		}
		self.ask(b"")?;
		Ok(data.len() as f64 / start.elapsed().as_secs_f64() / 1e6)
	}
}

impl Drop for Terminal {
	fn drop(&mut self) {
		let _ = self.tty.write_all(CLEAR);
		let _ = stty(&self.tty, &[&self.saved]);
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use crate::{
		parser::Parser,
		term::Term,
		theme::{Theme, ThemeKind},
	};
	use portable_pty::{MasterPty, PtySize, native_pty_system};
	use std::{
		io::Read,
		sync::{Arc, Mutex},
	};

	/// How the emulator on the other side answers queries.
	#[derive(Clone, Copy, PartialEq)]
	pub(crate) enum Answers {
		/// Each reply in one write.
		Whole,
		/// One byte per write, so replies arrive split.
		Bytewise,
		/// Never.
		Never,
		/// Each reply whole, a quarter of a second late.
		Slow,
		/// All but the XTVERSION reply.
		Anonymous,
	}

	/// How late [`Answers::Slow`] answers.
	const LATE: Duration = Duration::from_millis(250);

	/// What the emulator writes to the master: its replies and typed keys.
	type Keyboard = Arc<Mutex<Box<dyn Write + Send>>>;

	/// The far side of a PTY running Poopterm's own terminal as the
	/// emulator under test: everything it was sent, and the master.
	pub(crate) struct Emulator {
		seen: Arc<Mutex<Vec<u8>>>,
		keyboard: Keyboard,
		master: Box<dyn MasterPty + Send>,
	}

	impl Emulator {
		pub(crate) fn seen(&self) -> Vec<u8> {
			self.seen.lock().unwrap().clone()
		}

		/// Resizes the terminal, as dragging its window would.
		fn resize(&self, cols: u16, rows: u16) {
			let size = PtySize {
				rows,
				cols,
				..PtySize::default()
			};
			self.master.resize(size).unwrap();
		}

		/// Types `bytes` as if from the keyboard.
		pub(crate) fn type_in(&self, bytes: &[u8]) {
			self.keyboard.lock().unwrap().write_all(bytes).unwrap();
		}
	}

	/// A terminal of `cols`×`rows` answering as `answers` says, and the
	/// slave side for `Terminal` to drive.
	pub(crate) fn emulator(cols: u16, rows: u16, answers: Answers) -> (File, Emulator) {
		let size = PtySize {
			rows,
			cols,
			pixel_width: 0,
			pixel_height: 0,
		};
		let pair = native_pty_system().openpty(size).unwrap();
		let path = pair.master.tty_name().expect("the slave has a path");
		let tty = File::options().read(true).write(true).open(path).unwrap();
		drop(pair.slave);
		let seen = Arc::new(Mutex::new(Vec::new()));
		let mut input = pair.master.try_clone_reader().unwrap();
		let keyboard: Keyboard = Arc::new(Mutex::new(pair.master.take_writer().unwrap()));
		let (theirs, output) = (seen.clone(), keyboard.clone());
		thread::spawn(move || {
			let theme = Theme::from_kind(ThemeKind::Poop);
			let mut term = Term::new(cols as usize, rows as usize, theme, "/".into());
			let (mut parser, mut buf) = (Parser::new(), vec![0; 1 << 16]);
			while let Ok(n @ 1..) = input.read(&mut buf) {
				theirs.lock().unwrap().extend_from_slice(&buf[..n]);
				parser.advance(&mut term, &buf[..n]);
				let mut output = output.lock().unwrap();
				for reply in term.responses.drain(..) {
					match answers {
						Answers::Whole => output.write_all(&reply).unwrap(),
						Answers::Slow => {
							thread::sleep(LATE);
							output.write_all(&reply).unwrap();
						}
						Answers::Anonymous if reply.starts_with(b"\x1bP>|") => {}
						Answers::Anonymous => output.write_all(&reply).unwrap(),
						Answers::Bytewise => reply.iter().for_each(|byte| {
							output.write_all(&[*byte]).unwrap();
							output.flush().unwrap();
						}),
						Answers::Never => {}
					}
				}
			}
		});
		(
			tty,
			Emulator {
				seen,
				keyboard,
				master: pair.master,
			},
		)
	}

	pub(crate) fn settings(tty: &File) -> String {
		stty(tty, &["-g"]).unwrap()
	}

	#[test]
	fn asking_returns_what_the_terminal_said_before_the_attributes() {
		let (tty, _emulator) = emulator(80, 24, Answers::Whole);
		let mut terminal = Terminal::open(tty).unwrap();
		let heard = terminal.ask(b"\x1b[>q").unwrap();
		let name = format!("poopterm {}", env!("CARGO_PKG_VERSION"));
		assert_eq!(replies::xtversion(&heard), Some(name));
		// The attributes reply itself is consumed, so the next ask waits anew.
		assert_eq!(terminal.ask(b"").unwrap(), b"");
	}

	#[test]
	fn replies_split_across_reads_are_put_together() {
		let (tty, _emulator) = emulator(80, 24, Answers::Bytewise);
		let mut terminal = Terminal::open(tty).unwrap();
		let heard = terminal.ask(b"\x1b[>q").unwrap();
		assert!(replies::xtversion(&heard).is_some(), "{heard:?}");
	}

	#[test]
	fn a_rate_ends_only_once_the_terminal_has_parsed_everything() {
		let (tty, emulator) = emulator(80, 24, Answers::Whole);
		let mut terminal = Terminal::open(tty).unwrap();
		let data: Vec<u8> = b"0123456789abcdef".repeat(3 * CHUNK / 16 + 1);
		let rate = terminal.rate(&data, Mode::Raw).unwrap();
		assert!(rate > 0.0 && rate.is_finite(), "{rate}");
		// One query before the workload and one after it, nothing more.
		assert_eq!(emulator.seen(), [&b"\x1b[c"[..], &data, b"\x1b[c"].concat());
	}

	#[test]
	fn cooked_output_reaches_the_terminal_processed_and_raw_output_unchanged() {
		let (tty, emulator) = emulator(80, 24, Answers::Whole);
		let mut terminal = Terminal::open(tty).unwrap();
		terminal.rate(b"a\nb\n", Mode::Cooked).unwrap();
		terminal.rate(b"c\nd\n", Mode::Raw).unwrap();
		let expected = b"\x1b[ca\r\nb\r\n\x1b[c\x1b[cc\nd\n\x1b[c";
		assert_eq!(emulator.seen(), expected);
	}

	#[test]
	fn the_size_is_the_terminals() {
		let (tty, _emulator) = emulator(93, 27, Answers::Whole);
		let terminal = Terminal::open(tty).unwrap();
		assert_eq!(terminal.size().unwrap(), Size { cols: 93, rows: 27 });
	}

	#[test]
	fn the_terminal_is_given_back_as_it_was_and_cleared() {
		let (tty, emulator) = emulator(80, 24, Answers::Whole);
		let before = settings(&tty);
		let terminal = Terminal::open(tty.try_clone().unwrap()).unwrap();
		let during = stty(&tty, &["-a"]).unwrap();
		let flags: Vec<&str> = during.split_whitespace().collect();
		for flag in ["-echo", "-icanon", "-isig"] {
			assert!(flags.contains(&flag), "{flag}: {during}");
		}
		drop(terminal);
		assert_eq!(settings(&tty), before);
		assert!(emulator.seen().ends_with(CLEAR));
	}

	#[test]
	fn a_terminal_that_never_answers_is_an_error_not_a_hang() {
		let (tty, _emulator) = emulator(80, 24, Answers::Never);
		let mut terminal = Terminal::open(tty).unwrap();
		terminal.timeout = Duration::from_millis(200);
		let start = Instant::now();
		let error = terminal.ask(b"").unwrap_err().to_string();
		assert!(error.contains("did not answer"), "{error}");
		assert!(start.elapsed() < Duration::from_secs(30));
	}

	#[test]
	fn ctrl_c_stops_the_run() {
		let (tty, emulator) = emulator(80, 24, Answers::Never);
		let mut terminal = Terminal::open(tty).unwrap();
		emulator.type_in(&[INTERRUPT]);
		assert_eq!(terminal.ask(b"").unwrap_err().to_string(), "interrupted");
	}

	#[test]
	fn what_follows_an_answer_waits_for_the_next_ask() {
		let (tty, emulator) = emulator(80, 24, Answers::Never);
		let mut terminal = Terminal::open(tty).unwrap();
		emulator.type_in(b"one\x1b[?1ctwo\x1b[?1c");
		assert_eq!(terminal.ask(b"").unwrap(), b"one");
		assert_eq!(terminal.ask(b"").unwrap(), b"two");
	}

	#[test]
	fn each_rate_leaves_output_processing_as_its_mode_set_it() {
		let (tty, _emulator) = emulator(80, 24, Answers::Whole);
		let watch = tty.try_clone().unwrap();
		let mut terminal = Terminal::open(tty).unwrap();
		let opost = || {
			let all = stty(&watch, &["-a"]).unwrap();
			all
				.split_whitespace()
				.find(|flag| flag.ends_with("opost"))
				.map(str::to_owned)
		};
		terminal.rate(b"x", Mode::Raw).unwrap();
		assert_eq!(opost().as_deref(), Some("-opost"));
		terminal.rate(b"x", Mode::Cooked).unwrap();
		assert_eq!(opost().as_deref(), Some("opost"));
	}

	#[test]
	fn nothing_to_write_is_a_rate_of_zero_after_both_queries() {
		let (tty, emulator) = emulator(80, 24, Answers::Whole);
		let mut terminal = Terminal::open(tty).unwrap();
		assert_eq!(terminal.rate(b"", Mode::Raw).unwrap(), 0.0);
		assert_eq!(emulator.seen(), b"\x1b[c\x1b[c");
	}

	#[test]
	fn a_failed_run_still_gives_the_terminal_back() {
		let (tty, emulator) = emulator(80, 24, Answers::Never);
		let before = settings(&tty);
		let mut terminal = Terminal::open(tty.try_clone().unwrap()).unwrap();
		terminal.timeout = Duration::from_millis(50);
		assert!(terminal.rate(b"lost", Mode::Cooked).is_err());
		drop(terminal);
		assert_eq!(settings(&tty), before);
		assert!(emulator.seen().ends_with(CLEAR));
	}

	#[test]
	fn a_rate_with_replies_split_byte_by_byte_waits_for_each_whole() {
		let (tty, emulator) = emulator(80, 24, Answers::Bytewise);
		let mut terminal = Terminal::open(tty).unwrap();
		for _ in 0..3 {
			terminal.rate(b"data", Mode::Raw).unwrap();
		}
		assert_eq!(emulator.seen(), b"\x1b[cdata\x1b[c".repeat(3));
		// Nothing is left over from the split replies.
		assert_eq!(terminal.ask(b"").unwrap(), b"");
	}

	#[test]
	fn a_setting_stty_refuses_is_an_error_naming_it() {
		let (tty, _emulator) = emulator(80, 24, Answers::Whole);
		let error = stty(&tty, &["no-such-setting"]).unwrap_err().to_string();
		assert!(error.starts_with("stty no-such-setting: "), "{error}");
	}

	#[test]
	fn a_path_that_is_no_terminal_is_refused() {
		let file = File::open("/dev/null").unwrap();
		assert!(Terminal::open(file).is_err());
	}

	#[test]
	fn the_clock_runs_until_the_terminal_answers() {
		let (tty, _emulator) = emulator(80, 24, Answers::Slow);
		let mut terminal = Terminal::open(tty).unwrap();
		let data = vec![b'x'; 1000];
		// Even instant parsing takes as long as the late answer.
		let fastest = data.len() as f64 / LATE.as_secs_f64() / 1e6;
		let rate = terminal.rate(&data, Mode::Raw).unwrap();
		assert!(rate > 0.0 && rate <= fastest, "{rate} > {fastest}");
	}

	#[test]
	fn a_terminal_without_a_name_still_answers_the_attributes() {
		let (tty, _emulator) = emulator(80, 24, Answers::Anonymous);
		let mut terminal = Terminal::open(tty).unwrap();
		assert_eq!(terminal.ask(b"\x1b[>q").unwrap(), b"");
	}

	#[test]
	fn ctrl_c_wins_over_an_answer_heard_with_it() {
		let (tty, emulator) = emulator(80, 24, Answers::Never);
		let mut terminal = Terminal::open(tty).unwrap();
		emulator.type_in(b"\x03\x1b[?1c");
		assert_eq!(terminal.ask(b"").unwrap_err().to_string(), "interrupted");
	}

	#[test]
	fn keys_typed_while_waiting_do_not_put_the_deadline_off() {
		let (tty, emulator) = emulator(80, 24, Answers::Never);
		let mut terminal = Terminal::open(tty).unwrap();
		terminal.timeout = Duration::from_millis(200);
		let typist = thread::spawn(move || {
			for _ in 0..100 {
				emulator.type_in(b"x");
				thread::sleep(Duration::from_millis(20));
			}
		});
		let start = Instant::now();
		assert!(terminal.ask(b"").is_err());
		assert!(
			start.elapsed() < Duration::from_secs(1),
			"{:?}",
			start.elapsed()
		);
		typist.join().unwrap();
	}

	#[test]
	fn a_secondary_attributes_reply_is_heard_not_taken_for_the_answer() {
		let (tty, _emulator) = emulator(80, 24, Answers::Whole);
		let mut terminal = Terminal::open(tty).unwrap();
		let heard = terminal.ask(b"\x1b[>c").unwrap();
		assert!(
			heard.starts_with(b"\x1b[>") && heard.ends_with(b"c"),
			"{heard:?}"
		);
		assert_eq!(terminal.ask(b"").unwrap(), b"");
	}

	#[test]
	fn a_terminal_without_a_size_gets_the_smallest_grid() {
		let (tty, _emulator) = emulator(0, 0, Answers::Whole);
		let terminal = Terminal::open(tty).unwrap();
		assert_eq!(terminal.size().unwrap(), Size { cols: 2, rows: 2 });
	}

	#[test]
	fn the_size_is_read_when_asked_not_when_opened() {
		let (tty, emulator) = emulator(80, 24, Answers::Whole);
		let terminal = Terminal::open(tty).unwrap();
		emulator.resize(132, 50);
		assert_eq!(
			terminal.size().unwrap(),
			Size {
				cols: 132,
				rows: 50
			}
		);
	}
}
