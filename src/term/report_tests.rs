//! Device reports: what DA1 claims, where the cursor reports itself, the
//! cursor blink mode, and the requests left unanswered on purpose.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(20, 6, Theme::from_kind(ThemeKind::Poop), "/".into())
}

/// The replies to `bytes`, in order.
fn ask(term: &mut Term, bytes: &str) -> Vec<String> {
	Parser::new().advance(term, bytes.as_bytes());
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn primary_device_attributes_claim_only_what_is_implemented() {
	// A VT525 with Sixel graphics (4), horizontal scrolling (21) and ANSI
	// color (22). Selective erase (6), rectangles (28) and the rest are not
	// claimed until they exist.
	let mut t = terminal();
	assert_eq!(
		ask(&mut t, "\x1b[c\x1b[0c"),
		["\x1b[?65;4;21;22c", "\x1b[?65;4;21;22c"]
	);
}

#[test]
fn device_attribute_requests_with_a_parameter_are_ignored() {
	let mut t = terminal();
	for request in ["\x1b[1c", "\x1b[65c", "\x1b[>1c", "\x1b[>41c", "\x1b[0;1c"] {
		assert_eq!(ask(&mut t, request), Vec::<String>::new(), "{request:?}");
	}
	// An explicit 0 is the same as none.
	assert_eq!(ask(&mut t, "\x1b[>0c").len(), 1);
}

#[test]
fn cursor_reports_count_from_the_margins_in_origin_mode() {
	let mut t = terminal();
	assert_eq!(
		ask(&mut t, "\x1b[3;7H\x1b[6n\x1b[?6n"),
		["\x1b[3;7R", "\x1b[?3;7;1R"]
	);
	// Rows 2-5 scroll; in origin mode row 1 is the region's first.
	let replies = ask(&mut t, "\x1b[2;5r\x1b[?6h\x1b[2;7H\x1b[6n\x1b[?6n");
	assert_eq!(replies, ["\x1b[2;7R", "\x1b[?2;7;1R"]);
	// Leaving origin mode reports the same cell from the screen's top.
	assert_eq!(ask(&mut t, "\x1b[?6l\x1b[3;7H\x1b[6n"), ["\x1b[3;7R"]);
}

#[test]
fn the_extended_cursor_report_stays_on_the_screen_after_full_width_output() {
	let mut t = terminal();
	// The last column, with a wrap pending: still column 20.
	let line = "x".repeat(20);
	assert_eq!(ask(&mut t, &format!("{line}\x1b[?6n")), ["\x1b[?1;20;1R"]);
}

#[test]
fn other_private_status_requests_are_not_answered_as_cursor_reports() {
	let mut t = terminal();
	assert_eq!(
		ask(&mut t, "\x1b[?5n\x1b[?7n\x1b[?99n"),
		Vec::<String>::new()
	);
}

#[test]
fn cursor_blink_is_a_mode_that_reports_saves_and_restores() {
	let mut t = terminal();
	assert_eq!(ask(&mut t, "\x1b[?12$p"), ["\x1b[?12;2$y"]);
	assert_eq!(ask(&mut t, "\x1b[?12h\x1b[?12$p"), ["\x1b[?12;1$y"]);
	assert_eq!(ask(&mut t, "\x1b[?12l\x1b[?12$p"), ["\x1b[?12;2$y"]);
	// XTSAVE, change, XTRESTORE.
	let replies = ask(&mut t, "\x1b[?12h\x1b[?12s\x1b[?12l\x1b[?12r\x1b[?12$p");
	assert_eq!(replies, ["\x1b[?12;1$y"]);
}

#[test]
fn decscusr_odd_styles_blink_and_even_ones_hold_still() {
	let mut t = terminal();
	for (style, blinks) in [(1, 1), (2, 2), (3, 1), (4, 2), (5, 1), (6, 2), (0, 1)] {
		let replies = ask(&mut t, &format!("\x1b[{style} q\x1b[?12$p"));
		assert_eq!(replies, [format!("\x1b[?12;{blinks}$y")], "style {style}");
	}
}

#[test]
fn decrqss_reports_the_cursor_style_with_its_blink() {
	let mut t = terminal();
	let style = |t: &mut Term| ask(t, "\x1bP$q q\x1b\\");
	assert_eq!(
		style(&mut t),
		["\x1bP1$r2 q\x1b\\"],
		"a steady block at start"
	);
	for (set, reported) in [
		("3", "3"),
		("4", "4"),
		("5", "5"),
		("6", "6"),
		("1", "1"),
		("0", "1"),
	] {
		ask(&mut t, &format!("\x1b[{set} q"));
		assert_eq!(
			style(&mut t),
			[format!("\x1bP1$r{reported} q\x1b\\")],
			"after {set}"
		);
	}
	// ?12 changes the blink and keeps the shape.
	ask(&mut t, "\x1b[4 q\x1b[?12h");
	assert_eq!(style(&mut t), ["\x1bP1$r3 q\x1b\\"]);
	ask(&mut t, "\x1b[?12l");
	assert_eq!(style(&mut t), ["\x1bP1$r4 q\x1b\\"]);
}

#[test]
fn a_soft_reset_keeps_the_cursor_style_and_a_full_reset_stops_the_blink() {
	let mut t = terminal();
	assert_eq!(ask(&mut t, "\x1b[?12h\x1b[!p\x1b[?12$p"), ["\x1b[?12;1$y"]);
	assert_eq!(ask(&mut t, "\x1bc\x1b[?12$p"), ["\x1b[?12;2$y"]);
}

#[test]
fn rectangle_checksums_are_never_answered() {
	// DECRQCRA would let any program read the screen back a checksum at a
	// time, so it is blocked, whatever its parameters.
	let mut t = terminal();
	let requests = [
		"\x1b[1;1;1;1;6;20*y",
		"\x1b[1*y",
		"\x1b[*y",
		"\x1b[65535;1;1;1;1;1*y",
	];
	for request in requests {
		assert_eq!(
			ask(&mut t, &format!("secret{request}")),
			Vec::<String>::new(),
			"{request:?}"
		);
	}
	// The terminal still answers what it should afterwards.
	assert_eq!(ask(&mut t, "\x1b[5n"), ["\x1b[0n"]);
}

#[test]
fn every_report_claims_the_same_terminal() {
	let mut t = terminal();
	let replies = ask(&mut t, "\x1b[c\x1b[>c\x1bP$q\"p\x1b\\");
	let level = |reply: &str, prefix: &str| -> u16 {
		let rest = reply
			.strip_prefix(prefix)
			.unwrap_or_else(|| panic!("{reply:?}"));
		rest.split([';', 'c']).next().unwrap().parse().unwrap()
	};
	// DA1 65 is a VT5xx, DA2 64 a VT520 family member, DECSCL 65 level 5.
	assert_eq!(level(&replies[0], "\x1b[?"), 65);
	assert_eq!(level(&replies[1], "\x1b[>"), 64);
	assert_eq!(level(&replies[2], "\x1bP1$r"), 65);
	// DA2's version is the crate version as MMmmpp, with no ROM cartridge.
	let fields: Vec<&str> = replies[1][3..replies[1].len() - 1].split(';').collect();
	let version: Vec<u32> = env!("CARGO_PKG_VERSION")
		.split(['.', '-'])
		.take(3)
		.map(|part| part.parse().unwrap())
		.collect();
	assert_eq!(
		fields[1],
		(version[0] * 10_000 + version[1] * 100 + version[2]).to_string()
	);
	assert_eq!(fields[2], "0");
}
