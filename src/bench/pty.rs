//! Benchmark stages through a real PTY: a program writes a workload into
//! the slave side, and Poopterm reads the master as a tab does.
//!
//! The ceiling stages only read and discard, so they show what the kernel's
//! terminal layer alone allows; the pipeline stages run the reader thread,
//! output queue and parser thread a tab runs. Cooked mode keeps the output
//! processing every program gets (OPOST, ONLCR turning `\n` into `\r\n`),
//! which costs the kernel time per line; raw mode turns it off, so the
//! terminal's own speed shows.

use std::{
	error::Error,
	fs::File,
	io::Read,
	path::PathBuf,
	process,
	sync::{
		Arc,
		atomic::{AtomicU64, Ordering},
	},
	thread,
	time::Instant,
};

use super::{
	corpus::Size,
	mark::{MARK, UntilMark},
};
use crate::{
	pty::{
		OUTPUT_LIMIT, OutputQueue,
		device::{self, WindowSize},
		pump, reader,
		spawn::{self, Command, Exit},
	},
	term::SharedTerm,
};

/// Whether the kernel processes output on its way to the master.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mode {
	/// As programs get it: OPOST and ONLCR on.
	Cooked,
	/// OPOST off: bytes pass unchanged.
	Raw,
}

/// The master's side of a PTY, as read.
type Output = Box<dyn Read + Send>;

type Outcome<T> = Result<T, Box<dyn Error>>;

/// Runs `argv` on a new PTY of `size` in `mode`, handing its output to
/// `read`, and returns the seconds from spawning until `read` is done.
/// The program runs under `sh`, which turns output processing off with
/// `stty` in raw mode (on the slave itself, as any program would), so both
/// modes pay for the same start-up. After the program, `sh` notes its exit
/// status in a file, prints the [`MARK`] the output ends at and sleeps
/// until killed once the output is read: some kernels drop a PTY's unread
/// output when its program exits. A failed program is an error rather than
/// a fast, empty run.
fn timed(argv: &[&str], size: Size, mode: Mode, read: impl FnOnce(Output)) -> Outcome<f64> {
	let start = Instant::now();
	let pair = device::open(WindowSize::of(size.cols, size.rows, (0, 0)))?;
	let end = std::str::from_utf8(MARK)?;
	let script = format!(r#"s=$1; shift; "$@"; echo $? >"$s"; printf %s '{end}'; exec sleep 3600"#);
	let script = match mode {
		Mode::Cooked => script,
		Mode::Raw => format!("stty -opost || exit; {script}"),
	};
	let record = Payload::new(b"")?;
	let command = argv.iter().fold(
		Command::new("sh")
			.arg("-c")
			.arg(&script)
			.arg("sh")
			.arg(record.cat()[1]),
		|command, arg| command.arg(arg),
	);
	// Cloned first, so a failure leaves no child unwaited.
	let output = File::from(pair.master.try_clone()?);
	let mut child = spawn::spawn(&command, &pair.slave)?;
	drop(pair.slave);
	read(Box::new(UntilMark::new(output)));
	let seconds = start.elapsed().as_secs_f64();
	// `sh` sleeps past the mark; if it ended before, its own status counts.
	let _ = child.hang_up();
	let ended = match child.wait()? {
		Exit::Code(code) => code,
		Exit::Signal(signal) => 128 + signal,
	};
	let noted = std::fs::read_to_string(&record.0)?.trim().parse();
	let code = noted.unwrap_or(ended);
	if code != 0 {
		return Err(format!("{} exited with {code}", argv.join(" ")).into());
	}
	Ok(seconds)
}

/// Runs `argv` on a PTY and hands each read of its output to `sink`.
pub(crate) fn collect(
	argv: &[&str],
	size: Size,
	mode: Mode,
	sink: impl FnMut(&[u8]),
) -> Outcome<f64> {
	timed(argv, size, mode, |output| reader::drain(output, sink))
}

/// Runs `argv` on a PTY through the reader thread, output queue and parser
/// thread a tab runs, into `term`, dropping its replies.
pub(crate) fn pipeline(argv: &[&str], size: Size, mode: Mode, term: &SharedTerm) -> Outcome<f64> {
	timed(argv, size, mode, |output| {
		let queue = Arc::new(OutputQueue::new(OUTPUT_LIMIT));
		let theirs = queue.clone();
		let reading = thread::spawn(move || reader::forward(output, &theirs));
		pump::pump(&queue, term, &pump::Flags::default(), |_| (), || ());
		reading.join().expect("the PTY reader never panics");
	})
}

/// MB/s of `cat` writing `data` into a PTY of `size` in `mode`, through a
/// tab's pipeline into a primed terminal when `parse`, else read and dropped.
/// Writing the file and priming the terminal are not timed.
pub(crate) fn rate(data: &[u8], size: Size, mode: Mode, parse: bool) -> Outcome<f64> {
	let payload = Payload::new(data)?;
	let seconds = if parse {
		let term = SharedTerm::new(super::primed(size));
		pipeline(&payload.cat(), size, mode, &term)?
	} else {
		collect(&payload.cat(), size, mode, |_| ())?
	};
	Ok(data.len() as f64 / seconds / 1e6)
}

/// The calling thread's number, for file names: tests count their own.
fn thread_number() -> String {
	format!("{:?}", thread::current().id()).replace(|c: char| !c.is_ascii_digit(), "")
}

/// `data` in a file of its own until dropped, for `cat` to write.
pub(crate) struct Payload(PathBuf);

impl Payload {
	pub(crate) fn new(data: &[u8]) -> std::io::Result<Self> {
		static NEXT: AtomicU64 = AtomicU64::new(0);
		let next = NEXT.fetch_add(1, Ordering::Relaxed);
		let name = format!(
			"poopterm-bench-{}-{}-{next}",
			process::id(),
			thread_number()
		);
		let path = std::env::temp_dir().join(name);
		std::fs::write(&path, data)?;
		Ok(Self(path))
	}

	/// The command that writes it to its terminal.
	pub(crate) fn cat(&self) -> [&str; 2] {
		["cat", self.0.to_str().expect("temporary paths are UTF-8")]
	}
}

impl Drop for Payload {
	fn drop(&mut self) {
		let _ = std::fs::remove_file(&self.0);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		bench::{
			corpus::{self, WORKLOADS},
			primed,
		},
		parser::Parser,
		term::Term,
	};

	const SMALL: Size = Size { cols: 40, rows: 12 };

	/// Held by every test that writes payloads, so counting this process's
	/// temporary files sees no other test's.
	fn files() -> std::sync::MutexGuard<'static, ()> {
		static FILES: std::sync::Mutex<()> = std::sync::Mutex::new(());
		FILES
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	fn output(argv: &[&str], size: Size, mode: Mode) -> Vec<u8> {
		let mut all = Vec::new();
		collect(argv, size, mode, |bytes| all.extend_from_slice(bytes)).unwrap();
		all
	}

	fn printf(text: &str) -> Vec<u8> {
		output(&["printf", text], SMALL, Mode::Cooked)
	}

	#[test]
	fn cooked_output_turns_each_newline_into_a_carriage_return_and_newline() {
		assert_eq!(printf("a\\nb\\n"), b"a\r\nb\r\n");
	}

	#[test]
	fn output_ending_in_what_could_begin_the_mark_arrives_whole() {
		let almost = std::str::from_utf8(&MARK[..MARK.len() - 1]).unwrap();
		let argv = ["printf", "%s", almost];
		assert_eq!(output(&argv, SMALL, Mode::Raw), almost.as_bytes());
	}

	#[test]
	fn raw_output_passes_bytes_unchanged() {
		let raw = output(&["printf", "a\\nb\\n\\r\\t\\033[0m"], SMALL, Mode::Raw);
		assert_eq!(raw, b"a\nb\n\r\t\x1b[0m");
	}

	#[test]
	fn arguments_reach_the_program_as_given() {
		// Through `sh -c`, yet neither split at spaces nor expanded.
		let argv = ["printf", "%s|", "a b", "$HOME", "\"q\"", "*"];
		assert_eq!(output(&argv, SMALL, Mode::Raw), b"a b|$HOME|\"q\"|*|");
	}

	#[test]
	fn output_longer_than_several_reads_arrives_whole() {
		let _files = files();
		let data = vec![b'x'; 3 * reader::READ_SIZE + 5];
		let payload = Payload::new(&data).unwrap();
		let mut sizes = Vec::new();
		collect(&payload.cat(), SMALL, Mode::Raw, |bytes| {
			sizes.push(bytes.len())
		})
		.unwrap();
		assert!(sizes.len() > 1, "{sizes:?}");
		assert_eq!(sizes.iter().sum::<usize>(), data.len());
	}

	#[test]
	fn the_program_sees_the_size_asked() {
		let size = Size { cols: 93, rows: 27 };
		assert_eq!(output(&["stty", "size"], size, Mode::Cooked), b"27 93\r\n");
	}

	#[test]
	fn raw_mode_leaves_input_settings_alone() {
		// Only output processing is turned off; the program still gets a
		// canonical, echoing terminal like any other.
		let settings = String::from_utf8(output(&["stty", "-a"], SMALL, Mode::Raw)).unwrap();
		let flags: Vec<&str> = settings.split_whitespace().collect();
		for flag in ["-opost", "icanon", "echo"] {
			assert!(flags.contains(&flag), "{flag}: {settings}");
		}
	}

	#[test]
	fn the_smallest_and_largest_sizes_reach_the_program() {
		for (cols, rows) in [(2, 2), (1000, 2), (2, 1000)] {
			let seen = output(&["stty", "size"], Size { cols, rows }, Mode::Raw);
			assert_eq!(seen, format!("{rows} {cols}\n").as_bytes());
		}
	}

	#[test]
	fn a_program_killed_by_a_signal_is_an_error() {
		let killed = collect(&["sh", "-c", "kill -KILL $$"], SMALL, Mode::Cooked, |_| ());
		// The wrapping shell notes 128 + 9, as shells report a signal.
		assert!(killed.unwrap_err().to_string().ends_with("exited with 137"));
	}

	/// The error a run of `script` under `sh -c` ends with.
	fn failure(script: &str) -> String {
		let run = collect(&["sh", "-c", script], SMALL, Mode::Cooked, |_| ());
		run.expect_err(script).to_string()
	}

	#[test]
	fn a_wrapper_killed_before_it_notes_the_status_reports_its_own_signal() {
		// The program kills the shell around it, which never writes the
		// status file, so the shell's own end counts: 128 + SIGTERM.
		let error = failure("kill -TERM $PPID");
		assert!(error.ends_with("exited with 143"), "{error}");
	}

	#[test]
	fn a_size_beyond_what_the_kernel_holds_saturates_rather_than_wraps() {
		// 70000 columns would wrap to 4464 in 16 bits.
		let seen = output(
			&["stty", "size"],
			Size {
				cols: 70_000,
				rows: 3,
			},
			Mode::Raw,
		);
		assert_eq!(seen, b"3 65535\n");
	}

	#[test]
	fn settings_a_program_changed_do_not_carry_into_the_next_run() {
		// Each run gets a new PTY with the usual settings.
		output(&["stty", "-echo", "cols", "7"], SMALL, Mode::Raw);
		let next = String::from_utf8(output(&["stty", "-a"], SMALL, Mode::Raw)).unwrap();
		let flags: Vec<&str> = next.split_whitespace().collect();
		assert!(
			flags.contains(&"echo") && !flags.contains(&"-echo"),
			"{next}"
		);
		assert_eq!(output(&["stty", "size"], SMALL, Mode::Raw), b"12 40\n");
	}

	#[test]
	fn standard_error_is_part_of_the_output() {
		let seen = output(
			&["sh", "-c", "printf out; printf err >&2"],
			SMALL,
			Mode::Raw,
		);
		assert_eq!(seen, b"outerr");
	}

	#[test]
	fn a_job_left_running_does_not_stretch_the_run() {
		// It holds the terminal past its shell's end; the run ends at the
		// mark, and closing the PTY hangs the job up.
		let seconds = collect(&["sh", "-c", "sleep 5 &"], SMALL, Mode::Raw, |_| ()).unwrap();
		assert!(seconds < 3.0, "the run took {seconds} s");
	}

	#[test]
	fn a_programs_own_code_above_128_is_reported_as_it_is() {
		let error = failure("exit 200");
		assert!(error.ends_with("exited with 200"), "{error}");
	}

	#[test]
	fn the_program_runs_with_the_pty_as_its_terminal() {
		let script = ["sh", "-c", "test -t 0 && test -t 1 && test -t 2 && tty"];
		let seen = String::from_utf8(output(&script, SMALL, Mode::Raw)).unwrap();
		assert!(seen.starts_with("/dev/"), "{seen:?}");
	}

	#[test]
	fn no_wrapping_shell_is_left_behind_once_a_run_ends() {
		// The program names the shell around it, which sleeps past the mark
		// until it is hung up and reaped.
		let seen = output(&["sh", "-c", "echo $PPID"], SMALL, Mode::Raw);
		let wrapper: libc::pid_t = String::from_utf8(seen).unwrap().trim().parse().unwrap();
		// SAFETY: signal 0 sends nothing; a reaped pid no longer exists.
		let alive = unsafe { libc::kill(wrapper, 0) } == 0;
		assert!(!alive, "the shell {wrapper} outlived its run");
	}

	#[test]
	fn the_time_covers_the_whole_run() {
		let seconds = collect(&["sleep", "0.2"], SMALL, Mode::Cooked, |_| ()).unwrap();
		assert!((0.2..60.0).contains(&seconds), "{seconds}");
	}

	#[test]
	fn a_missing_program_is_an_error_not_a_hang() {
		let error = collect(
			&["/nonexistent/poopterm-bench"],
			SMALL,
			Mode::Cooked,
			|_| (),
		);
		assert!(error.is_err());
	}

	#[test]
	fn a_payload_is_written_whole_and_removed_when_dropped() {
		let _files = files();
		let data: Vec<u8> = (0..=255).cycle().take(300_000).collect();
		let payload = Payload::new(&data).unwrap();
		let path = PathBuf::from(payload.cat()[1]);
		assert_eq!(std::fs::read(&path).unwrap(), data);
		assert_eq!(output(&payload.cat(), SMALL, Mode::Raw), data);
		drop(payload);
		assert!(!path.exists());
	}

	#[test]
	fn two_payloads_never_share_a_file() {
		let _files = files();
		let (first, second) = (Payload::new(b"1").unwrap(), Payload::new(b"2").unwrap());
		assert_ne!(first.cat(), second.cat());
		assert_eq!(std::fs::read(second.cat()[1]).unwrap(), b"2");
	}

	/// The visible rows and cursor of `term`.
	fn screen(term: &Term) -> (Vec<String>, usize, usize) {
		let rows = (0..term.rows).map(|y| term.grid[y].iter().map(|cell| cell.ch).collect());
		(rows.collect(), term.cx, term.cy)
	}

	#[test]
	fn raw_output_through_the_pipeline_leaves_the_screen_parsing_it_leaves() {
		let _files = files();
		for workload in WORKLOADS
			.iter()
			.filter(|w| ["log", "tui", "emoji", "cursor"].contains(&w.name))
		{
			let data = workload.build(16 << 10, SMALL);
			let payload = Payload::new(&data).unwrap();
			let shared = SharedTerm::new(primed(SMALL));
			pipeline(&payload.cat(), SMALL, Mode::Raw, &shared).unwrap();
			let mut expected = primed(SMALL);
			Parser::new().advance(&mut expected, &data);
			assert_eq!(
				screen(&shared.lock()),
				screen(&expected),
				"{}",
				workload.name
			);
		}
	}

	#[test]
	fn a_failing_program_is_an_error_naming_it() {
		let error = collect(&["sh", "-c", "exit 3"], SMALL, Mode::Raw, |_| ()).unwrap_err();
		assert_eq!(error.to_string(), "sh -c exit 3 exited with 3");
		let term = SharedTerm::new(primed(SMALL));
		let error = pipeline(&["false"], SMALL, Mode::Cooked, &term).unwrap_err();
		assert_eq!(error.to_string(), "false exited with 1");
	}

	#[test]
	fn cooked_output_through_the_pipeline_leaves_the_screen_its_carriage_returns_leave() {
		let _files = files();
		// Bare newlines, which only output processing turns into new lines.
		let data = b"one\ntwo\n  three\n".repeat(50);
		let payload = Payload::new(&data).unwrap();
		let shared = SharedTerm::new(primed(SMALL));
		pipeline(&payload.cat(), SMALL, Mode::Cooked, &shared).unwrap();
		let mut cooked = Vec::new();
		for &byte in &data {
			match byte {
				b'\n' => cooked.extend_from_slice(b"\r\n"),
				_ => cooked.push(byte),
			}
		}
		let mut expected = primed(SMALL);
		Parser::new().advance(&mut expected, &cooked);
		assert_eq!(screen(&shared.lock()), screen(&expected));
	}

	#[test]
	fn output_beyond_the_queue_limit_waits_for_the_parser_and_all_arrives() {
		// Three queues' worth: the reader thread must block on a full queue
		// rather than drop output, as a tab's does.
		let _files = files();
		let data = corpus::find("log").unwrap().build(3 * OUTPUT_LIMIT, SMALL);
		let payload = Payload::new(&data).unwrap();
		let shared = SharedTerm::new(primed(SMALL));
		pipeline(&payload.cat(), SMALL, Mode::Raw, &shared).unwrap();
		let mut expected = primed(SMALL);
		Parser::new().advance(&mut expected, &data);
		assert_eq!(screen(&shared.lock()), screen(&expected));
	}

	#[test]
	fn nothing_written_is_a_rate_of_zero() {
		let _files = files();
		assert_eq!(rate(b"", SMALL, Mode::Cooked, false).unwrap(), 0.0);
	}

	#[test]
	fn the_pipeline_shows_what_the_program_sees_and_waits_for_it() {
		let term = SharedTerm::new(primed(SMALL));
		let argv = ["sh", "-c", "sleep 0.2; stty size"];
		let seconds = pipeline(&argv, SMALL, Mode::Cooked, &term).unwrap();
		assert!((0.2..60.0).contains(&seconds), "{seconds}");
		let (rows, ..) = screen(&term.lock());
		assert!(rows.iter().any(|row| row.starts_with("12 40 ")), "{rows:?}");
	}

	#[test]
	fn rates_count_the_bytes_written_not_the_bytes_read() {
		let _files = files();
		// Cooked output grows (`\n` → `\r\n`); the rate is still of the
		// workload, so cooked and raw compare like for like.
		let data = b"line\n".repeat(20_000);
		for parse in [false, true] {
			for mode in [Mode::Cooked, Mode::Raw] {
				let rate = rate(&data, SMALL, mode, parse).unwrap();
				let seconds = data.len() as f64 / rate / 1e6;
				assert!((0.0..10.0).contains(&seconds), "{mode:?} {parse}: {rate}");
			}
		}
	}

	#[test]
	fn a_rate_leaves_no_file_behind() {
		let _files = files();
		let before = temporary_files();
		rate(b"x", SMALL, Mode::Raw, true).unwrap();
		assert_eq!(temporary_files(), before);
	}

	#[test]
	fn a_failed_run_leaves_no_file_behind() {
		let before = temporary_files();
		assert!(collect(&["false"], SMALL, Mode::Raw, |_| ()).is_err());
		assert_eq!(temporary_files(), before);
	}

	#[test]
	fn files_are_named_for_the_thread_that_made_them() {
		let name = |payload: &Payload| payload.0.file_name().unwrap().to_owned();
		let here = name(&Payload::new(b"x").unwrap());
		let there = thread::spawn(move || name(&Payload::new(b"y").unwrap()));
		let there = there.join().unwrap();
		let prefix = format!("poopterm-bench-{}-{}-", process::id(), thread_number());
		assert!(here.to_string_lossy().starts_with(&prefix), "{here:?}");
		assert!(!there.to_string_lossy().starts_with(&prefix), "{there:?}");
	}

	fn temporary_files() -> usize {
		let prefix = format!("poopterm-bench-{}-{}-", process::id(), thread_number());
		let entries = std::fs::read_dir(std::env::temp_dir()).unwrap();
		entries
			.filter(|entry| {
				entry
					.as_ref()
					.unwrap()
					.file_name()
					.to_string_lossy()
					.starts_with(&prefix)
			})
			.count()
	}

	#[test]
	fn the_pipeline_parses_everything_before_it_returns() {
		let shared = SharedTerm::new(primed(SMALL));
		pipeline(&["printf", "\\033[2J\\033[Hend"], SMALL, Mode::Raw, &shared).unwrap();
		let term = shared.lock();
		assert_eq!(screen(&term).0[0].trim_end(), "end");
		assert_eq!((term.cx, term.cy), (3, 0));
	}

	#[test]
	fn replies_to_queries_are_dropped_not_written_back() {
		// A DA1 query would make a tab answer on the PTY; here nobody reads
		// the answer, and the run still ends.
		let shared = SharedTerm::new(primed(SMALL));
		pipeline(&["printf", "\\033[c\\033[6n"], SMALL, Mode::Raw, &shared).unwrap();
		assert!(shared.lock().responses.is_empty());
	}
}
