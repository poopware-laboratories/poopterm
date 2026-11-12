//! Requests sent as device control strings: DECRQSS (`DCS $ q`) and
//! XTGETTCAP (`DCS + q`).
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(80, 24, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

/// The reply to DECRQSS for `setting`.
fn status(term: &mut Term, setting: &str) -> String {
	send(term, &format!("\x1bP$q{setting}\x1b\\"));
	let replies = std::mem::take(&mut term.responses);
	assert_eq!(replies.len(), 1, "{setting}");
	String::from_utf8(replies[0].clone()).unwrap()
}

#[test]
fn graphic_rendition_is_reported() {
	let mut t = terminal();
	assert_eq!(status(&mut t, "m"), "\x1bP1$r0m\x1b\\");
	send(&mut t, "\x1b[1;2;3;4;5;7;8;9;53m");
	assert_eq!(status(&mut t, "m"), "\x1bP1$r0;1;2;3;4;5;7;8;9;53m\x1b\\");
	send(&mut t, "\x1b[0;4:2m");
	assert_eq!(status(&mut t, "m"), "\x1bP1$r0;21m\x1b\\");
	// Palette colors by number, others as RGB.
	send(&mut t, "\x1b[0;31;48:2::1:2:3m");
	assert_eq!(status(&mut t, "m"), "\x1bP1$r0;31;48:2::1:2:3m\x1b\\");
	send(&mut t, "\x1b[0;94m");
	assert_eq!(status(&mut t, "m"), "\x1bP1$r0;94m\x1b\\");
	send(&mut t, "\x1b[0;38;5;208m");
	assert_eq!(status(&mut t, "m"), "\x1bP1$r0;38:5:208m\x1b\\");
	send(&mut t, "\x1b[0;38;2;10;20;30m");
	assert_eq!(status(&mut t, "m"), "\x1bP1$r0;38:2::10:20:30m\x1b\\");
}

#[test]
fn other_settings_are_reported() {
	let mut t = terminal();
	assert_eq!(status(&mut t, "r"), "\x1bP1$r1;24r\x1b\\");
	send(&mut t, "\x1b[5;10r");
	assert_eq!(status(&mut t, "r"), "\x1bP1$r5;10r\x1b\\");
	assert_eq!(status(&mut t, " q"), "\x1bP1$r2 q\x1b\\");
	// Blinking styles are reported as set, not as their steady twins.
	send(&mut t, "\x1b[5 q");
	assert_eq!(status(&mut t, " q"), "\x1bP1$r5 q\x1b\\");
	send(&mut t, "\x1b[4 q");
	assert_eq!(status(&mut t, " q"), "\x1bP1$r4 q\x1b\\");
	assert_eq!(status(&mut t, "\"p"), "\x1bP1$r62;1\"p\x1b\\");
	assert_eq!(status(&mut t, "\"q"), "\x1bP1$r0\"q\x1b\\");
	assert_eq!(status(&mut t, "t"), "\x1bP1$r24t\x1b\\");
	assert_eq!(status(&mut t, "$|"), "\x1bP1$r80$|\x1b\\");
	assert_eq!(status(&mut t, "*|"), "\x1bP1$r24*|\x1b\\");
}

#[test]
fn unknown_or_oversized_requests_are_invalid() {
	let mut t = terminal();
	for setting in ["s", "", "x", "mm", "$}", &"m".repeat(5000)] {
		assert_eq!(status(&mut t, setting), "\x1bP0$r\x1b\\", "{setting}");
	}
}

#[test]
fn replies_are_not_held_back_by_synchronized_output() {
	let mut t = terminal();
	send(&mut t, "\x1b[?2026h\x1bP$qr\x1b\\");
	assert_eq!(t.responses, [b"\x1bP1$r1;24r\x1b\\".to_vec()]);
}

#[test]
fn other_device_control_strings_are_ignored() {
	let mut t = terminal();
	send(&mut t, "\x1bP$tm\x1b\\\x1bP!|0\x1b\\\x1bP1$qm\x1b\\");
	assert!(t.responses.is_empty());
}

/// Every reply to XTGETTCAP for the hex-encoded `names`.
fn capabilities(term: &mut Term, names: &str) -> Vec<String> {
	send(term, &format!("\x1bP+q{names}\x1b\\"));
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn capabilities_are_reported_in_hex() {
	let mut t = terminal();
	// TN: xterm-poopterm.
	assert_eq!(
		capabilities(&mut t, "544e"),
		["\x1bP1+r544E=787465726D2D706F6F707465726D\x1b\\"]
	);
	// kcuu1 and Co together; flags have no value.
	assert_eq!(
		capabilities(&mut t, "6B63757531;436f"),
		["\x1bP1+r6B63757531=1B4F41;436F=323536\x1b\\"]
	);
	assert_eq!(capabilities(&mut t, "616d"), ["\x1bP1+r616D\x1b\\"]);
}

#[test]
fn an_unknown_capability_ends_the_list() {
	let mut t = terminal();
	assert_eq!(capabilities(&mut t, "6e6f7065"), ["\x1bP0+r\x1b\\"]);
	assert_eq!(
		capabilities(&mut t, "436f;6e6f7065;544e"),
		["\x1bP1+r436F=323536\x1b\\", "\x1bP0+r\x1b\\"]
	);
	for bad in ["", "zz", "436", "436f;", ";436f", &"41".repeat(3000)] {
		assert_eq!(
			capabilities(&mut t, bad).last().unwrap(),
			"\x1bP0+r\x1b\\",
			"{bad}"
		);
	}
}
