//! Conformance programs on a real PTY, headless: the program's output goes
//! through the parser into a [`Term`], its replies go back, and a script
//! decides what to type whenever the output pauses.
//!
//! - esctest2: `ESCTEST=/path/to/esctest2/esctest cargo test --release --
//!   --ignored --nocapture esctest` (see `esctest_tests`).
//! - vttest: `cargo test --release -- --ignored --nocapture vttest_pages`
//!   walks every menu and writes each page it draws to `VTTEST_PAGES`
//!   (`/tmp/poopterm-vttest.txt` by default), to read against vttest's own
//!   description of each page. `VTTEST_SKIP` lists choices to leave, by menu path or label words (`keyboard` by default)
//!   out, comma-separated.
use crate::{
	parser::Parser,
	sys::{
		pty::{self as device, WindowSize},
		spawn::{self, Command},
	},
	term::Term,
	theme::{Theme, ThemeKind},
};
use std::{
	fs::File,
	io::{Read, Write},
	sync::mpsc,
	thread,
	time::Duration,
};

/// How long the output must pause before the script types.
const IDLE: Duration = Duration::from_millis(300);

/// Runs `command` on a `cols` × `rows` PTY. `feed` takes each read of
/// output and returns the replies to send; `idle` sees the screen whenever
/// the output pauses and returns what to type, or `None` to stop.
pub(super) fn drive(
	command: &Command,
	(cols, rows): (usize, usize),
	mut feed: impl FnMut(&mut Term, &[u8]) -> Vec<Vec<u8>>,
	mut idle: impl FnMut(&Term) -> Option<Vec<u8>>,
) -> Term {
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	let pair = device::open(WindowSize::of(cols, rows, (10, 20))).expect("a PTY");
	let mut input = File::from(pair.master.try_clone().expect("a second master"));
	let mut output = File::from(pair.master);
	let mut child = spawn::spawn(command, &pair.slave).expect("the program starts");
	drop(pair.slave);
	let (send, received) = mpsc::channel();
	thread::spawn(move || {
		let mut buffer = vec![0; 1 << 16];
		while let Ok(read @ 1..) = output.read(&mut buffer) {
			if send.send(buffer[..read].to_vec()).is_err() {
				break;
			}
		}
	});
	loop {
		match received.recv_timeout(IDLE) {
			Ok(bytes) => {
				for reply in feed(&mut term, &bytes) {
					input
						.write_all(&reply)
						.expect("the program reads its replies");
				}
			}
			Err(mpsc::RecvTimeoutError::Timeout) => {
				if child.try_wait().expect("the program's state").is_some() {
					break;
				}
				match idle(&term) {
					Some(keys) => input.write_all(&keys).expect("the program reads its input"),
					None => {
						let _ = child.hang_up();
						break;
					}
				}
			}
			Err(mpsc::RecvTimeoutError::Disconnected) => break,
		}
	}
	let _ = child.wait();
	term
}

/// The screen's rows as text, without trailing blanks.
pub(super) fn screen(term: &Term) -> Vec<String> {
	(0..term.rows)
		.map(|row| {
			term.grid[row]
				.iter()
				.map(|cell| cell.ch)
				.collect::<String>()
				.trim_end()
				.to_owned()
		})
		.collect()
}

/// A menu's choice prompt on `screen`: the highest choice it offers.
fn menu_choices(screen: &[String]) -> Option<u32> {
	let line = screen
		.iter()
		.find(|line| line.contains("Enter choice number (0 - "))?;
	let (_, rest) = line.split_once("(0 - ")?;
	rest.split(')').next()?.trim().parse().ok()
}

/// Walks vttest's menus depth first: every choice of every menu, Return on
/// every page, `0` to leave a menu once all its choices ran.
#[derive(Default)]
struct Walker {
	/// The open menus: their first line, the next choice and the last one.
	menus: Vec<(String, u32, u32)>,
	/// Choices to leave out: by menu path (`5`, `11.3.4`), or by words of
	/// their label (`keyboard`).
	skip: Vec<String>,
	/// Pauses in a row on a page that asks for nothing and has not changed
	/// since the last: a test that wants a key other than Return. A page
	/// still changing is still reading reports.
	waits: usize,
	/// Quit keys typed since a page last asked for Return.
	quits: usize,
	/// The screen at the last pause.
	last: Vec<String>,
}

/// Pauses on an unchanged page that asks for nothing before each quit key:
/// 3 seconds, past vttest's waits for reports.
const WAITS: usize = 10;

/// Quit keys a page may ignore before the walk stops.
const QUITS: usize = 30;

impl Walker {
	/// What to type on `screen`, and the menu path it was typed at.
	fn next(&mut self, screen: &[String]) -> Option<(Vec<u8>, String)> {
		let path = self
			.menus
			.iter()
			.map(|(_, next, _)| (next - 1).to_string())
			.collect::<Vec<_>>()
			.join(".");
		let Some(last) = menu_choices(screen) else {
			// Return only where a page asks for it: typed while a test
			// still reads its reports, it lands among them. A page that
			// asks for nothing for long gets the keys tests quit on.
			let asks = screen
				.iter()
				.any(|line| line.to_ascii_uppercase().contains("RETURN"));
			let unchanged = screen == self.last;
			self.last = screen.to_vec();
			if asks {
				(self.waits, self.quits) = (0, 0);
				return Some((b"\r".to_vec(), path));
			}
			self.waits = if unchanged { self.waits + 1 } else { 0 };
			if self.waits < WAITS {
				return Some((Vec::new(), path));
			}
			// A page no quit key leaves: stop, so the pages show where.
			(self.waits, self.quits) = (0, self.quits + 1);
			if self.quits > QUITS {
				return None;
			}
			// Doubled: some tests quit on any key typed twice in a row.
			let keys = [&b"qq"[..], b"\x1b\x1b", b"\r\r"][(self.quits - 1) % 3];
			return Some((keys.to_vec(), path));
		};
		(self.waits, self.quits) = (0, 0);
		let title = screen
			.iter()
			.find(|line| !line.is_empty())
			.cloned()
			.unwrap_or_default();
		if self.menus.last().is_none_or(|(open, ..)| *open != title) {
			if let Some(depth) = self.menus.iter().position(|(open, ..)| *open == title) {
				self.menus.truncate(depth + 1);
			} else {
				self.menus.push((title, 1, last));
			}
		}
		let top = self.menus.len() == 1;
		// Where this menu sits: the choices that led to it.
		let depth = self.menus.len() - 1;
		let parents: Vec<String> = self.menus[..depth]
			.iter()
			.map(|(_, next, _)| (next - 1).to_string())
			.collect();
		let (_, next, last) = self.menus.last_mut()?;
		let skipped = |choice: u32| {
			let path = parents
				.iter()
				.cloned()
				.chain([choice.to_string()])
				.collect::<Vec<_>>()
				.join(".");
			// The choice's line on the menu, such as ` 5. Test of keyboard`.
			let label = screen
				.iter()
				.map(|line| line.trim_start())
				.find(|line| {
					line
						.strip_prefix(&choice.to_string())
						.is_some_and(|rest| rest.starts_with(['.', '*']))
				})
				.unwrap_or("")
				.to_lowercase();
			self.skip.iter().any(|skip| {
				let by_path = skip.bytes().all(|b| b.is_ascii_digit() || b == b'.');
				if by_path {
					*skip == path
				} else {
					label.contains(&skip.to_lowercase())
				}
			})
		};
		while skipped(*next) {
			*next += 1;
		}
		if *next > *last {
			if top {
				return None;
			}
			self.menus.pop();
			return Some((b"0\r".to_vec(), path));
		}
		let choice = *next;
		*next += 1;
		Some((format!("{choice}\r").into_bytes(), path))
	}
}

#[test]
fn menu_prompts_give_their_last_choice() {
	let screen = |line: &str| vec!["VT100 test program".to_owned(), line.to_owned()];
	assert_eq!(
		menu_choices(&screen("Enter choice number (0 - 12): ")),
		Some(12)
	);
	assert_eq!(
		menu_choices(&screen("          Enter choice number (0 - 7): ")),
		Some(7)
	);
	assert_eq!(menu_choices(&screen("Push <RETURN>")), None);
}

#[test]
fn the_walker_runs_every_choice_then_leaves_the_menu() {
	let menu = |title: &str, last: u32| {
		vec![
			title.to_owned(),
			format!("Enter choice number (0 - {last}): "),
		]
	};
	let page = vec!["A page".to_owned(), "Push <RETURN>".to_owned()];
	let mut walker = Walker {
		skip: vec!["2".into(), "3.2".into()],
		..Default::default()
	};
	let mut typed = |screen: &[String]| {
		walker
			.next(screen)
			.map(|(keys, _)| String::from_utf8(keys).unwrap())
	};
	assert_eq!(typed(&menu("Main", 3)).as_deref(), Some("1\r"));
	assert_eq!(typed(&page).as_deref(), Some("\r"));
	// Choice 2 is skipped; 3 opens a submenu.
	assert_eq!(typed(&menu("Main", 3)).as_deref(), Some("3\r"));
	assert_eq!(typed(&menu("Sub", 3)).as_deref(), Some("1\r"));
	// 3.2 is skipped too, by its path.
	assert_eq!(typed(&menu("Sub", 3)).as_deref(), Some("3\r"));
	assert_eq!(typed(&menu("Sub", 3)).as_deref(), Some("0\r"));
	// Back at the main menu with nothing left: stop.
	assert_eq!(typed(&menu("Main", 3)), None);
}

#[test]
fn choices_are_skipped_by_words_of_their_label() {
	let screen: Vec<String> = [
		"Menu 3: Tests",
		"          1. Test of keyboard",
		"          2* Test Keyboard usage (DECKBUM)",
		"          3. Test of screen",
		"Enter choice number (0 - 3): ",
	]
	.map(str::to_owned)
	.to_vec();
	let mut walker = Walker {
		skip: vec!["Keyboard".into()],
		..Default::default()
	};
	// 1 and 2 name the keyboard, in either case and after `.` or `*`.
	assert_eq!(walker.next(&screen).unwrap().0, b"3\r");
	assert_eq!(walker.next(&screen), None);
}

#[test]
fn return_is_typed_only_where_a_page_asks_for_it() {
	let mut walker = Walker::default();
	let mut typed = |line: &str| walker.next(&[line.to_owned()]).unwrap().0;
	assert_eq!(typed("Push <RETURN>"), b"\r");
	assert_eq!(typed("Push <return> to continue"), b"\r");
	// Still reading reports: nothing, so no key lands among them.
	assert_eq!(typed("1: DECCKM  <27> [ ? 1 ; 2 $ y"), b"");
	assert_eq!(typed("Push <RETURN>"), b"\r");
}

#[test]
fn a_page_that_stays_unchanged_gets_doubled_quit_keys_in_turn() {
	let mut walker = Walker::default();
	let page = ["Press 'q' to quit.".to_owned()];
	let keys: Vec<Vec<u8>> = (0..3 * WAITS + 1)
		.map(|_| walker.next(&page).unwrap().0)
		.collect();
	// The first pause only notes the page; each quit key needs WAITS
	// unchanged pauses after the last.
	let typed: Vec<(usize, &[u8])> = keys
		.iter()
		.enumerate()
		.filter(|(_, key)| !key.is_empty())
		.map(|(at, key)| (at, &key[..]))
		.collect();
	let expected: [(usize, &[u8]); 3] = [
		(WAITS, b"qq"),
		(2 * WAITS, b"\x1b\x1b"),
		(3 * WAITS, b"\r\r"),
	];
	assert_eq!(typed, expected);
	// A page that asks starts the count over.
	assert_eq!(walker.next(&["Push <RETURN>".to_owned()]).unwrap().0, b"\r");
	assert_eq!(walker.next(&["Quiet".to_owned()]).unwrap().0, b"");
}

#[test]
fn a_page_still_printing_reports_gets_no_keys() {
	let mut walker = Walker::default();
	// Each pause shows one more report: never a key among them.
	for n in 0..5 * WAITS {
		let page: Vec<String> = (0..=n)
			.map(|line| format!("{line}: <27> [ ? {line} ; 0 $ y"))
			.collect();
		assert_eq!(walker.next(&page).unwrap().0, b"", "after {n} reports");
	}
}

#[test]
fn a_page_that_ignores_every_quit_key_ends_the_walk() {
	let mut walker = Walker::default();
	let page = ["Ignores every key".to_owned()];
	let mut quits = 0;
	for _ in 0..=(QUITS + 1) * WAITS {
		match walker.next(&page) {
			Some((key, _)) => quits += usize::from(!key.is_empty()),
			None => break,
		}
	}
	assert_eq!(quits, QUITS, "then the walk ends");
}

#[test]
#[ignore = "needs vttest installed"]
fn vttest_pages() {
	let out = std::env::var("VTTEST_PAGES").unwrap_or_else(|_| "/tmp/poopterm-vttest.txt".into());
	// Keyboard tests wait for keys a person presses, and no quit key the
	// walker types leaves them; they appear in several menus.
	let skip = std::env::var("VTTEST_SKIP").unwrap_or_else(|_| "keyboard".into());
	let mut walker = Walker {
		skip: skip.split(',').map(|path| path.trim().to_owned()).collect(),
		..Default::default()
	};
	// Written page by page, so a walk that stalls still shows where.
	let mut pages = std::io::BufWriter::new(File::create(&out).expect("the pages file"));
	let max: usize = std::env::var("VTTEST_STEPS")
		.ok()
		.and_then(|n| n.parse().ok())
		.unwrap_or(600);
	let mut steps = 0;
	let command = Command::new("vttest")
		.arg("24x80.80")
		.env("TERM", "xterm-256color");
	let mut parser = Parser::new();
	drive(
		&command,
		(80, 24),
		|term, bytes| {
			parser.advance(term, bytes);
			std::mem::take(&mut term.responses)
		},
		|term| {
			steps += 1;
			if steps > max {
				return None;
			}
			let screen = screen(term);
			let (keys, path) = walker.next(&screen)?;
			let typed = String::from_utf8_lossy(&keys);
			writeln!(pages, "==== {steps} at {path} (typing {typed:?})").expect("the pages file");
			for line in &screen {
				writeln!(pages, "{line}").expect("the pages file");
			}
			pages.flush().expect("the pages file");
			Some(keys)
		},
	);
	println!("vttest pages: {out} ({steps} steps)");
}
