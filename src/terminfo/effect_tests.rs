//! Every capability the terminfo entry advertises holds in Poopterm: output
//! capabilities change the terminal, reply patterns match its replies, and
//! input ones match what it sends.
use std::{
	io::Write,
	os::unix::ffi::OsStrExt,
	process::{Command, Stdio},
};

use super::{
	NAME,
	tests::{TempDir, capabilities, compile_into, have},
};
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

/// The entry compiled into a temporary directory, or `None` without the
/// ncurses tools.
struct Entry(TempDir);

impl Entry {
	fn new(name: &str) -> Option<Self> {
		if !have("tic") || !have("tput") || !have("infocmp") {
			return None;
		}
		let dir = TempDir::new(name);
		assert!(compile_into(&dir.0));
		Some(Self(dir))
	}

	/// `cap` expanded with `args` by tput.
	fn tput(&self, cap: &str, args: &[&str]) -> Vec<u8> {
		let output = Command::new("tput")
			.env("TERMINFO", &self.0.0)
			.args(["-T", NAME, cap])
			.args(args)
			.output()
			.unwrap();
		assert!(output.status.success(), "tput {cap} {args:?}");
		output.stdout
	}
}

/// A 10x6 screen full of letters with the cursor at row 2, column 3.
fn terminal() -> Term {
	let mut term = Term::new(10, 6, Theme::from_kind(ThemeKind::Poop), "/".into());
	let rows = ["abcdefghij"; 6].join("\r\n");
	feed(&mut term, rows.as_bytes());
	feed(&mut term, b"\x1b[3;4H");
	term
}

fn feed(term: &mut Term, bytes: &[u8]) {
	Parser::new().advance(term, bytes);
}

/// Everything a capability may change, as text to compare.
fn state(t: &Term) -> String {
	let cursor = (t.cx, t.cy, t.wrap_pending, t.cursor_visible, t.cursor_shape);
	let style = (
		t.fg,
		t.bg,
		t.attrs,
		t.underline_color,
		t.bold,
		t.reverse,
		t.charsets,
		t.active_charset,
	);
	let region = (t.scroll_top, t.scroll_bottom, t.memory_locked, &t.tab_stops);
	let saved = (t.saved_cursor.is_some(), t.primary_screen.is_some());
	let modes = (
		t.bracketed_paste,
		t.mouse_tracking,
		t.mouse_encoding,
		t.application_cursor,
		t.application_keypad,
		t.auto_wrap,
		t.reverse_screen,
		t.insert_mode,
		t.focus_reporting,
		t.synchronized.is_some(),
		t.key_options,
		t.keyboard,
	);
	let colors = (t.colors, t.color_overrides);
	let host = (&t.title, &t.responses, t.bell, t.clipboard_writes.len());
	let grid: Vec<_> = (0..t.rows).map(|row| t.grid[row].to_vec()).collect();
	format!(
		"{cursor:?} {style:?} {region:?} {saved:?} {modes:?} {colors:?} {host:?} {} {grid:?}",
		t.scrollback.len()
	)
}

/// Output capabilities: arguments, what to send first so the capability
/// has something to change, and what to send after it.
const EFFECTS: &[(&str, &[&str], &str, &str)] = &[
	("bel", &[], "", ""),
	("blink", &[], "", ""),
	("bold", &[], "", ""),
	("dim", &[], "", ""),
	("invis", &[], "", ""),
	("rev", &[], "", ""),
	("sitm", &[], "", ""),
	("smso", &[], "", ""),
	("smul", &[], "", ""),
	("smxx", &[], "", ""),
	("Smol", &[], "", ""),
	("Smulx", &["3"], "", ""),
	("rmso", &[], "\x1b[7m", ""),
	("rmul", &[], "\x1b[4m", ""),
	("ritm", &[], "\x1b[3m", ""),
	("rmxx", &[], "\x1b[9m", ""),
	("Rmol", &[], "\x1b[53m", ""),
	("sgr0", &[], "\x1b[1m", ""),
	(
		"sgr",
		&["1", "0", "0", "0", "0", "0", "0", "0", "0"],
		"",
		"",
	),
	("setaf", &["1"], "", ""),
	("setab", &["1"], "", ""),
	("setrgbf", &["1", "2", "3"], "", ""),
	("setal", &["66051"], "", ""),
	("Setulc", &["66051"], "", ""),
	("setrgbb", &["1", "2", "3"], "", ""),
	("op", &[], "\x1b[31;42m", ""),
	("smacs", &[], "", "q"),
	("rmacs", &[], "\x1b(0", ""),
	("cbt", &[], "", ""),
	("ht", &[], "", ""),
	("hts", &[], "", ""),
	("tbc", &[], "", ""),
	("cr", &[], "", ""),
	("nel", &[], "", ""),
	("ind", &[], "", ""),
	("indn", &["2"], "", ""),
	("ri", &[], "", ""),
	("rin", &["2"], "", ""),
	("home", &[], "", ""),
	("cup", &["0", "0"], "", ""),
	("hpa", &["0"], "", ""),
	("vpa", &["0"], "", ""),
	("cub", &["2"], "", ""),
	("cub1", &[], "", ""),
	("cud", &["2"], "", ""),
	("cud1", &[], "", ""),
	("cuf", &["2"], "", ""),
	("cuf1", &[], "", ""),
	("cuu", &["2"], "", ""),
	("cuu1", &[], "", ""),
	("csr", &["1", "3"], "", ""),
	("sc", &[], "", ""),
	("rc", &[], "\x1b7\x1b[H", ""),
	("civis", &[], "", ""),
	("cnorm", &[], "\x1b[?25l", ""),
	("Ss", &["6"], "", ""),
	("Se", &[], "\x1b[6 q", ""),
	("Cs", &["#ff0000"], "", ""),
	("Cr", &[], "\x1b]12;#ff0000\x1b\\", ""),
	("clear", &[], "", ""),
	("ed", &[], "", ""),
	("el", &[], "", ""),
	("el1", &[], "", ""),
	("ech", &["2"], "", ""),
	("dch", &["2"], "", ""),
	("dch1", &[], "", ""),
	("ich", &["2"], "", ""),
	("ich1", &[], "", ""),
	("dl", &["2"], "", ""),
	("dl1", &[], "", ""),
	("il", &["2"], "", ""),
	("il1", &[], "", ""),
	("rep", &["65", "3"], "", ""),
	("E3", &[], "\x1b[6H\n\n", ""),
	("smir", &[], "", ""),
	("rmir", &[], "\x1b[4h", ""),
	("smam", &[], "\x1b[?7l", ""),
	("rmam", &[], "", ""),
	("smcup", &[], "", ""),
	("rmcup", &[], "\x1b[?1049h", ""),
	("smkx", &[], "", ""),
	("rmkx", &[], "\x1b[?1h\x1b=", ""),
	("smm", &[], "", ""),
	("rmm", &[], "\x1b[?1036l\x1b[?1034h", ""),
	("meml", &[], "", ""),
	("memu", &[], "\x1bl", ""),
	("BE", &[], "", ""),
	("BD", &[], "\x1b[?2004h", ""),
	("fe", &[], "", ""),
	("fd", &[], "\x1b[?1004h", ""),
	("XM", &["1"], "", ""),
	("XM", &["0"], "\x1b[?1006;1000h", ""),
	("Sync", &["1"], "", ""),
	("Ms", &["c", "eA=="], "", ""),
	("initc", &["1", "1000", "0", "0"], "", ""),
	("oc", &[], "\x1b]4;1;rgb:ff/00/00\x1b\\", ""),
	("is2", &[], "\x1b[4h", ""),
	("rs1", &[], "\x1b[4h", ""),
	("rs2", &[], "\x1b[4h", ""),
	// Status line and titles: the text between them becomes the title.
	("tsl", &[], "", "t\x07"),
	("TS", &[], "", "t\x07"),
	("fsl", &[], "\x1b]2;t", ""),
	("dsl", &[], "\x1b]2;t\x07", ""),
	// Queries change nothing but the replies waiting to be sent.
	("u7", &[], "", ""),
	("u9", &[], "", ""),
	("RV", &[], "", ""),
	("XR", &[], "", ""),
];

/// Patterns for the replies to queries, checked against those replies.
const REPLIES: &[&str] = &["u6", "u8"];
/// What Poopterm sends (pasted text's brackets, mouse reports), the
/// character set mapping line drawing uses, and the visual bell.
const OTHERS: &[&str] = &["PS", "PE", "xm", "acsc", "flash"];

#[test]
fn every_output_capability_changes_the_terminal() {
	let Some(entry) = Entry::new("effects") else {
		return;
	};
	for &(cap, args, before, after) in EFFECTS {
		let mut term = terminal();
		feed(&mut term, before.as_bytes());
		let unchanged = state(&term);
		feed(&mut term, &entry.tput(cap, args));
		feed(&mut term, after.as_bytes());
		assert_ne!(state(&term), unchanged, "{cap} changes nothing");
	}
}

#[test]
fn every_advertised_string_capability_is_checked() {
	let Some(entry) = Entry::new("coverage") else {
		return;
	};
	let mut advertised: Vec<_> = capabilities(&entry.0.0)
		.into_keys()
		.filter(|cap| !cap.starts_with('k'))
		.collect();
	advertised.sort();
	let mut checked: Vec<_> = EFFECTS.iter().map(|&(cap, ..)| cap).collect();
	checked.extend(REPLIES.iter().chain(OTHERS));
	checked.sort();
	checked.dedup();
	assert_eq!(advertised, checked);
}

/// Whether `text` is matched in full by extended regular expression `pattern`.
fn matches(pattern: &[u8], text: &[u8]) -> bool {
	let mut grep = Command::new("grep")
		.arg("-Eqx")
		.arg(std::ffi::OsStr::from_bytes(pattern))
		.stdin(Stdio::piped())
		.spawn()
		.unwrap();
	let mut stdin = grep.stdin.take().unwrap();
	stdin.write_all(text).unwrap();
	stdin.write_all(b"\n").unwrap();
	drop(stdin);
	grep.wait().unwrap().success()
}

#[test]
fn replies_match_the_advertised_patterns() {
	let Some(entry) = Entry::new("replies") else {
		return;
	};
	let reply = |query: &str| {
		let mut term = terminal();
		feed(&mut term, &entry.tput(query, &[]));
		assert_eq!(term.responses.len(), 1, "{query}");
		term.responses.remove(0)
	};
	// The cursor report has the form u6 gives for tput's 0-based position,
	// with the row first like cup.
	let mut term = terminal();
	feed(&mut term, b"\x1b[3;3H");
	feed(&mut term, &entry.tput("u7", &[]));
	assert_eq!(term.responses, [entry.tput("u6", &["2", "2"])]);
	let mut cup = entry.tput("cup", &["2", "3"]);
	*cup.last_mut().unwrap() = b'R';
	assert_eq!(reply("u7"), cup);
	assert!(matches(b"\x1b\\[\\?[;0-9]+c", &reply("u9")));
	assert_eq!(entry.tput("u8", &[]), b"\x1b[?%[;0123456789]c");
}

#[test]
fn line_drawing_and_paste_brackets_match_what_poopterm_does() {
	let Some(entry) = Entry::new("others") else {
		return;
	};
	// Each pair maps a character to the one printed under smacs; every one
	// draws something else than the ASCII character itself.
	let acsc = entry.tput("acsc", &[]);
	assert!(!acsc.is_empty());
	for pair in acsc.chunks(2) {
		let mut term = terminal();
		feed(&mut term, &entry.tput("smacs", &[]));
		feed(&mut term, &pair[1..]);
		assert_ne!(term.grid[2][3].ch, pair[1] as char, "{:?}", pair[1] as char);
	}
	// The visual bell turns the screen to reverse video, then back.
	let flash = entry.tput("flash", &[]);
	let back = flash.iter().rposition(|&b| b == 0x1b).unwrap();
	let mut term = terminal();
	feed(&mut term, &flash[..back]);
	assert!(term.reverse_screen);
	feed(&mut term, &flash[back..]);
	assert!(!term.reverse_screen);
	// xm describes SGR mouse reports from 0-based y and x, button, pressed.
	use crate::input::mouse::{
		MouseAction, MouseButtonCode, MouseEncoding, MouseReport, encode_mouse,
	};
	for (kind, pressed) in [(MouseAction::Press, "1"), (MouseAction::Release, "0")] {
		let report = MouseReport {
			button: MouseButtonCode::Right,
			kind,
			col: 4,
			row: 3,
			pixel: (25, 33),
			mods: Default::default(),
		};
		let xm = entry.tput("xm", &["2", "3", "2", pressed]);
		assert_eq!(encode_mouse(report, MouseEncoding::Sgr, 1000), Some(xm));
	}
	let pasted = crate::input::paste::paste_bytes("x", true);
	let mut expected = entry.tput("PS", &[]);
	expected.push(b'x');
	expected.extend(entry.tput("PE", &[]));
	assert_eq!(pasted, expected);
}

#[test]
fn boolean_capabilities_hold() {
	let Some(entry) = Entry::new("booleans") else {
		return;
	};
	let caps = String::from_utf8(
		Command::new("infocmp")
			.args(["-x", "-1", "-A"])
			.arg(&entry.0.0)
			.arg(NAME)
			.output()
			.unwrap()
			.stdout,
	)
	.unwrap();
	let flag = |name: &str| caps.lines().any(|line| line.trim() == format!("{name},"));
	// bce: erasing fills with the current background.
	assert!(flag("bce"));
	let mut term = terminal();
	feed(&mut term, &entry.tput("setab", &["1"]));
	feed(&mut term, &entry.tput("el", &[]));
	assert_eq!(term.grid[2][9].bg, term.bg);
	assert_ne!(term.bg, terminal().bg);
	// am and xenl: the last column holds the cursor until the next character.
	assert!(flag("am") && flag("xenl"));
	let mut term = terminal();
	feed(&mut term, b"\x1b[3;10Hx");
	assert_eq!((term.cx, term.cy, term.wrap_pending), (9, 2, true));
	feed(&mut term, b"y");
	assert_eq!((term.cx, term.cy), (1, 3));
	// OTbs: backspace moves left.
	assert!(flag("OTbs"));
	let mut term = terminal();
	feed(&mut term, b"\x08");
	assert_eq!(term.cx, 2);
}

#[test]
fn initc_scales_thousandths_to_hex_bytes() {
	let Some(entry) = Entry::new("initc") else {
		return;
	};
	// x * 255 / 1000, rounded down: 999 must not round up to ff.
	let set = entry.tput("initc", &["1", "500", "999", "1000"]);
	assert_eq!(set, b"\x1b]4;1;rgb:7F/FE/FF\x1b\\");
	assert_eq!(
		entry.tput("initc", &["255", "0", "4", "3"]),
		b"\x1b]4;255;rgb:00/01/00\x1b\\"
	);
}
