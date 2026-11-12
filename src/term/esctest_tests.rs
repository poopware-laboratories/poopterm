//! Runs esctest2, the xterm conformance suite, against the parser and
//! terminal through a real PTY, one test per esctest process so a failure
//! that leaves replies unread cannot spoil the tests after it:
//!
//! ```sh
//! ESCTEST=/path/to/esctest2/esctest ESCTEST_NAMES=names.txt \
//!     cargo test --release -- --ignored --nocapture esctest
//! ```
//!
//! `ESCTEST_NAMES` is a file naming the tests (`Class.test_name`, or the
//! `Run test: …` lines of an earlier esctest log); without it, one process
//! runs `ESCTEST_INCLUDE` (a regex, everything by default). Logs go to
//! `ESCTEST_LOG` (a directory, `/tmp/poopterm-esctest` by default), and the
//! failing tests are listed at the end.
//!
//! esctest reads cells back with DECRQCRA checksums, which Poopterm never
//! answers (they would let any program read the screen). This harness
//! answers them itself, from the grid, so the suite can still check what
//! the terminal drew; nothing here changes what the terminal answers.
use super::pty_harness_tests::{drive, screen};
use crate::{
	parser::Parser,
	sys::spawn::Command,
	term::Term,
	theme::{Theme, ThemeKind},
};

/// What the harness found in a program's output.
#[derive(Debug, PartialEq)]
enum Piece<'a> {
	/// Output for the terminal.
	Output(&'a [u8]),
	/// A DECRQCRA request's parameters, for the harness to answer.
	Checksum(Vec<u16>),
}

/// Splits `bytes` at DECRQCRA requests (`CSI Ps ; … * y`). A request cut
/// off at the end stays in the returned tail, to complete with the next read.
fn split(bytes: &[u8]) -> (Vec<Piece<'_>>, &[u8]) {
	let mut pieces = Vec::new();
	let (mut start, mut at) = (0, 0);
	while let Some(found) = bytes[at..].windows(2).position(|w| w == b"\x1b[") {
		let csi = at + found;
		let params = csi + 2;
		let end = params
			+ bytes[params..]
				.iter()
				.take_while(|b| matches!(b, b'0'..=b'9' | b';'))
				.count();
		match &bytes[end..] {
			[b'*', b'y', ..] => {
				if csi > start {
					pieces.push(Piece::Output(&bytes[start..csi]));
				}
				let numbers = std::str::from_utf8(&bytes[params..end]).unwrap_or("");
				let values = numbers.split(';').map(|n| n.parse().unwrap_or(0)).collect();
				pieces.push(Piece::Checksum(values));
				(start, at) = (end + 2, end + 2);
			}
			[] | [b'*'] => {
				if csi > start {
					pieces.push(Piece::Output(&bytes[start..csi]));
				}
				return (pieces, &bytes[csi..]);
			}
			_ => at = params,
		}
	}
	// A lone ESC at the end may start a request.
	let keep = usize::from(bytes.last() == Some(&0x1b) && bytes.len() > start);
	if bytes.len() - keep > start {
		pieces.push(Piece::Output(&bytes[start..bytes.len() - keep]));
	}
	(pieces, &bytes[bytes.len() - keep..])
}

/// The DECCKSR reply to a DECRQCRA request: xterm's checksum of the
/// rectangle (`Pid ; Pp ; top ; left ; bottom ; right`, 1-based, defaults
/// the whole page), the negated 16-bit sum of the characters.
fn checksum_reply(term: &Term, params: &[u16]) -> String {
	let get = |i: usize, default: usize| match params.get(i).copied().unwrap_or(0) {
		0 => default,
		n => usize::from(n),
	};
	let origin = if term.origin_mode { term.scroll_top } else { 0 };
	let top = (get(2, 1) - 1 + origin).min(term.rows - 1);
	let left = (get(3, 1) - 1).min(term.cols - 1);
	let bottom = (get(4, term.rows) - 1 + origin).min(term.rows - 1);
	let right = (get(5, term.cols) - 1).min(term.cols - 1);
	let mut sum = 0u32;
	for y in top..=bottom {
		for cell in &term.grid[y][left..=right] {
			sum = sum.wrapping_add(cell.ch as u32);
		}
	}
	let checksum = 0x1_0000u32.wrapping_sub(sum) & 0xffff;
	format!("\x1bP{}!~{checksum:04X}\x1b\\", get(0, 0))
}

#[test]
fn requests_are_split_from_the_output_around_them() {
	let (pieces, tail) = split(b"ab\x1b[1;0;2;3;2;3*ycd\x1b[0m");
	let expected = [
		Piece::Output(b"ab"),
		Piece::Checksum(vec![1, 0, 2, 3, 2, 3]),
		Piece::Output(b"cd\x1b[0m"),
	];
	assert_eq!(pieces, expected);
	assert_eq!(tail, b"");
}

#[test]
fn a_request_cut_off_by_a_read_waits_for_the_rest() {
	for cut in [&b"x\x1b"[..], b"x\x1b[", b"x\x1b[12;0", b"x\x1b[12;0*"] {
		let (pieces, tail) = split(cut);
		assert_eq!(pieces, [Piece::Output(b"x")], "{cut:?}");
		assert_eq!(tail, &cut[1..], "{cut:?}");
	}
}

#[test]
fn other_sequences_with_star_or_y_are_output() {
	let bytes = b"\x1b[2*x\x1b[5y\x1b[?1*y";
	assert_eq!(split(bytes).0, [Piece::Output(bytes)]);
}

#[test]
fn a_checksum_counts_one_cell_or_the_whole_page() {
	let mut term = Term::new(4, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, b"AB");
	// 'B' alone: 0x10000 - 0x42.
	assert_eq!(
		checksum_reply(&term, &[7, 0, 1, 2, 1, 2]),
		"\x1bP7!~FFBE\x1b\\"
	);
	// The page: A, B and six blanks.
	let sum = 0x41 + 0x42 + 6 * 0x20;
	let whole = format!("\x1bP3!~{:04X}\x1b\\", 0x1_0000 - sum);
	assert_eq!(checksum_reply(&term, &[3]), whole);
}

/// Runs esctest on `include` (a regex of test names), logging to `log`:
/// the names of the tests that passed and failed, and what was on screen
/// when esctest stopped.
fn run(dir: &str, include: &str, log: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
	let command = Command::new("python3")
		.arg("-E")
		.arg("-s")
		.arg(format!("{dir}/esctest.py"))
		.arg("--expected-terminal=xterm")
		.arg("--no-print-logs")
		.arg(format!("--include={include}"))
		.arg(format!("--logfile={log}"))
		.arg("--timeout=0.5")
		.env("TERM", "xterm-256color")
		.cwd(dir);
	let (mut parser, mut pending) = (Parser::new(), Vec::new());
	let term = drive(
		&command,
		(80, 24),
		|term, bytes| {
			pending.extend_from_slice(bytes);
			let (pieces, tail) = split(&pending);
			let mut replies = Vec::new();
			for piece in pieces {
				match piece {
					Piece::Output(bytes) => {
						parser.advance(term, bytes);
						replies.append(&mut term.responses);
					}
					Piece::Checksum(params) => replies.push(checksum_reply(term, &params).into_bytes()),
				}
			}
			pending = tail.to_vec();
			replies
		},
		// esctest types nothing itself; a long pause is it waiting on a
		// report it never got, which its own timeout ends.
		|_| Some(Vec::new()),
	);
	let report = std::fs::read_to_string(log).unwrap_or_default();
	let (mut passed, mut failed, mut current) = (Vec::new(), Vec::new(), None);
	for line in report.lines() {
		if let Some(name) = line.strip_prefix("Run test: ") {
			current = Some(name.trim().to_owned());
		} else if line.starts_with("Passed.") || line.starts_with("Fails as expected") {
			// A known xterm limitation esctest expects to fail counts as passed.
			passed.extend(current.take());
		} else if line.starts_with("*** TEST ") && line.contains("FAILED") {
			failed.extend(current.take());
		}
	}
	(passed, failed, screen(&term))
}

/// The test names in `text`: one per line, bare or as `Run test: Name`.
fn names(text: &str) -> Vec<String> {
	let mut names: Vec<String> = text
		.lines()
		.map(|line| line.trim().trim_start_matches("Run test: ").trim())
		.filter(|name| {
			!name.contains(' ')
				&& name
					.split_once('.')
					.is_some_and(|(class, test)| !class.is_empty() && test.starts_with("test_"))
		})
		.map(str::to_owned)
		.collect();
	names.dedup();
	names
}

#[test]
fn test_names_come_from_lists_and_logs() {
	let text =
		"Run test: CUPTests.test_CUP_Basic\nPassed.\n\nBSTests.test_BS_Basic\n*** 5 tests passed ***\n";
	assert_eq!(
		names(text),
		["CUPTests.test_CUP_Basic", "BSTests.test_BS_Basic"]
	);
}

#[test]
#[ignore = "needs an esctest2 checkout in ESCTEST"]
fn esctest() {
	let Ok(dir) = std::env::var("ESCTEST") else {
		eprintln!("set ESCTEST to esctest2's esctest directory");
		return;
	};
	let logs = std::env::var("ESCTEST_LOG").unwrap_or_else(|_| "/tmp/poopterm-esctest".into());
	std::fs::create_dir_all(&logs).expect("the log directory");
	let listed = std::env::var("ESCTEST_NAMES")
		.ok()
		.map(|file| names(&std::fs::read_to_string(file).expect("the names file")));
	let (mut passed, mut failed) = (Vec::new(), Vec::new());
	match listed {
		Some(names) => {
			for name in names {
				let log = format!("{logs}/{name}.log");
				let (ok, bad, shown) = run(&dir, &format!("^{}$", name.replace('.', "\\.")), &log);
				if ok.is_empty() && bad.is_empty() {
					// It never ran: what esctest said instead.
					eprintln!("{name} did not run:\n{}", shown.join("\n").trim());
					failed.push(name);
				} else {
					passed.extend(ok);
					failed.extend(bad);
				}
			}
		}
		None => {
			let include = std::env::var("ESCTEST_INCLUDE").unwrap_or_else(|_| ".*".into());
			let (ok, bad, _) = run(&dir, &include, &format!("{logs}/all.log"));
			(passed, failed) = (ok, bad);
		}
	}
	println!("{} passed, {} failed", passed.len(), failed.len());
	for name in &failed {
		println!("FAILED {name}");
	}
	println!("logs: {logs}");
}
