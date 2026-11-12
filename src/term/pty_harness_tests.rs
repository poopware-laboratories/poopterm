//! Conformance programs on a real PTY, headless: the program's output goes
//! through the parser into a [`Term`], its replies go back, and a script
//! decides what to type whenever the output pauses.
//!
//! - esctest2: `ESCTEST=/path/to/esctest2/esctest cargo test --release --
//!   --ignored --nocapture esctest` (see `esctest_tests`).
//! - vttest: `cargo test --release -- --ignored --nocapture vttest_pages`
//!   walks every menu and writes each page it draws to `VTTEST_PAGES`
//!   (`/tmp/poopterm-vttest.txt` by default), to read against vttest's own
//!   description of each page. `VTTEST_SKIP` lists main-menu items to leave (5, the keyboard, by default)
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
	/// Main-menu choices to leave out.
	skip: Vec<u32>,
	/// Pauses in a row on a page that asks for nothing: a test still
	/// reading reports, or one that wants a key other than Return.
	waits: usize,
}

/// Pauses on a page that asks for nothing before typing the keys tests
/// quit on: 3 seconds, past vttest's waits for reports.
const WAITS: usize = 10;

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
			self.waits = if asks { 0 } else { self.waits + 1 };
			// A page no quit key leaves: stop, so the pages show where.
			if self.waits > WAITS + 30 {
				return None;
			}
			let keys: &[u8] = match self.waits.checked_sub(WAITS) {
				_ if asks => b"\r",
				None => b"",
				// Doubled: some tests quit on any key typed twice in a row.
				Some(n) => [&b"qq"[..], b"\x1b\x1b", b"\r\r"][n % 3],
			};
			return Some((keys.to_vec(), path));
		};
		self.waits = 0;
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
		let (_, next, last) = self.menus.last_mut()?;
		while top && self.skip.contains(next) {
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
		skip: vec![2],
		..Default::default()
	};
	let mut typed = |screen: &[String]| {
		walker
			.next(screen)
			.map(|(keys, _)| String::from_utf8(keys).unwrap())
	};
	assert_eq!(typed(&menu("Main", 3)).as_deref(), Some("1\r"));
	assert_eq!(typed(&page).as_deref(), Some("\r"));
	// Choice 2 is skipped; 3 opens a submenu with one choice.
	assert_eq!(typed(&menu("Main", 3)).as_deref(), Some("3\r"));
	assert_eq!(typed(&menu("Sub", 1)).as_deref(), Some("1\r"));
	assert_eq!(typed(&menu("Sub", 1)).as_deref(), Some("0\r"));
	// Back at the main menu with nothing left: stop.
	assert_eq!(typed(&menu("Main", 3)), None);
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
fn a_page_that_asks_for_nothing_for_long_gets_doubled_quit_keys_in_turn() {
	let mut walker = Walker::default();
	// Pages that change as keys echo still count as the same wait.
	let keys: Vec<Vec<u8>> = (0..WAITS + 4)
		.map(|n| {
			walker
				.next(&[format!("Press 'q' to quit. Echo {n}")])
				.unwrap()
				.0
		})
		.collect();
	assert!(keys[..WAITS - 1].iter().all(Vec::is_empty));
	let expected: [&[u8]; 5] = [b"qq", b"\x1b\x1b", b"\r\r", b"qq", b"\x1b\x1b"];
	assert_eq!(keys[WAITS - 1..], expected);
	// One that never leaves ends the walk.
	let mut stuck = Walker::default();
	let page = ["Ignores every key".to_owned()];
	assert_eq!(
		(0..WAITS + 30).filter_map(|_| stuck.next(&page)).count(),
		WAITS + 30
	);
	assert_eq!(stuck.next(&page), None);
	// A page that asks starts the count over.
	assert_eq!(walker.next(&["Push <RETURN>".to_owned()]).unwrap().0, b"\r");
	assert_eq!(walker.next(&["Quiet".to_owned()]).unwrap().0, b"");
}

#[test]
#[ignore = "needs vttest installed"]
fn vttest_pages() {
	let out = std::env::var("VTTEST_PAGES").unwrap_or_else(|_| "/tmp/poopterm-vttest.txt".into());
	// Menu 5 tests the keyboard: it waits for keys pressed by a person.
	let skip = std::env::var("VTTEST_SKIP").unwrap_or_else(|_| "5".into());
	let mut walker = Walker {
		skip: skip
			.split(',')
			.filter_map(|n| n.trim().parse().ok())
			.collect(),
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
