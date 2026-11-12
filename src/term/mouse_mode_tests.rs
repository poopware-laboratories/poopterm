//! Mouse modes: X10 compatibility tracking (`?9`) and the coordinate
//! encodings `?1005`, `?1006` and `?1015`, which replace one another.
use crate::{
	input::mouse::MouseEncoding,
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(20, 5, Theme::from_kind(ThemeKind::Poop), "/".into())
}

fn send(term: &mut Term, bytes: &str) {
	Parser::new().advance(term, bytes.as_bytes());
}

fn replies(term: &mut Term) -> Vec<String> {
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn x10_compatibility_tracking_turns_on_and_off() {
	let mut t = terminal();
	send(&mut t, "\x1b[?9h");
	assert_eq!(t.mouse_tracking, 9);
	send(&mut t, "\x1b[?9$p\x1b[?1000$p");
	assert_eq!(replies(&mut t), ["\x1b[?9;1$y", "\x1b[?1000;2$y"]);
	send(&mut t, "\x1b[?9l");
	assert_eq!(t.mouse_tracking, 0);
	send(&mut t, "\x1b[?9$p");
	assert_eq!(replies(&mut t), ["\x1b[?9;2$y"]);
}

#[test]
fn x10_compatibility_and_normal_tracking_replace_each_other() {
	let mut t = terminal();
	send(&mut t, "\x1b[?9h\x1b[?1000h");
	assert_eq!(t.mouse_tracking, 1000);
	send(&mut t, "\x1b[?9h");
	assert_eq!(t.mouse_tracking, 9);
}

#[test]
fn each_encoding_turns_on_and_off() {
	for (mode, encoding) in [
		(1005, MouseEncoding::Utf8),
		(1006, MouseEncoding::Sgr),
		(1015, MouseEncoding::Urxvt),
		(1016, MouseEncoding::SgrPixels),
	] {
		let mut t = terminal();
		send(&mut t, &format!("\x1b[?{mode}h"));
		assert_eq!(t.mouse_encoding, encoding, "?{mode}");
		send(&mut t, &format!("\x1b[?{mode}l"));
		assert_eq!(t.mouse_encoding, MouseEncoding::X10, "?{mode}");
	}
}

#[test]
fn the_last_encoding_set_wins() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1006h\x1b[?1015h");
	assert_eq!(t.mouse_encoding, MouseEncoding::Urxvt);
	send(&mut t, "\x1b[?1015;1005h");
	assert_eq!(t.mouse_encoding, MouseEncoding::Utf8);
}

#[test]
fn resetting_an_encoding_not_in_use_keeps_the_current_one() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1015h\x1b[?1006l\x1b[?1005l");
	assert_eq!(t.mouse_encoding, MouseEncoding::Urxvt);
}

#[test]
fn only_the_encoding_in_use_reports_as_set() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b[?1006h\x1b[?1015h\x1b[?1005$p\x1b[?1006$p\x1b[?1015$p",
	);
	assert_eq!(
		replies(&mut t),
		["\x1b[?1005;2$y", "\x1b[?1006;2$y", "\x1b[?1015;1$y"]
	);
}

#[test]
fn xtrestore_brings_back_the_saved_encoding() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1015h\x1b[?1005;1006;1015s\x1b[?1006h");
	assert_eq!(t.mouse_encoding, MouseEncoding::Sgr);
	send(&mut t, "\x1b[?1005;1006;1015r");
	assert_eq!(t.mouse_encoding, MouseEncoding::Urxvt);
}

#[test]
fn a_full_reset_returns_to_the_default_encoding() {
	let mut t = terminal();
	send(&mut t, "\x1b[?9h\x1b[?1015h\x1bc");
	assert_eq!(
		(t.mouse_tracking, t.mouse_encoding),
		(0, MouseEncoding::X10)
	);
}

#[test]
fn an_encoding_does_not_turn_tracking_on() {
	// The encoding only says how reports look; tracking says whether to send.
	let mut t = terminal();
	send(&mut t, "\x1b[?1005h\x1b[?1015h");
	assert_eq!(t.mouse_tracking, 0);
}

#[test]
fn sgr_pixels_and_sgr_replace_each_other() {
	// Programs often set ?1006 then ?1016, falling back if the second is
	// unknown; the last one set is used.
	let mut t = terminal();
	send(&mut t, "\x1b[?1006h\x1b[?1016h");
	assert_eq!(t.mouse_encoding, MouseEncoding::SgrPixels);
	send(&mut t, "\x1b[?1006$p\x1b[?1016$p");
	assert_eq!(replies(&mut t), ["\x1b[?1006;2$y", "\x1b[?1016;1$y"]);
	send(&mut t, "\x1b[?1016l");
	assert_eq!(t.mouse_encoding, MouseEncoding::X10);
}
