//! Alternate scroll (`?1007`): whether the wheel sends cursor keys to
//! programs on the alternate screen that do not track the mouse.
use crate::{
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
fn alternate_scroll_is_on_until_a_program_turns_it_off() {
	let mut t = terminal();
	assert!(t.alternate_scroll);
	send(&mut t, "\x1b[?1007$p\x1b[?1007l\x1b[?1007$p");
	assert!(!t.alternate_scroll);
	send(&mut t, "\x1b[?1007h");
	assert!(t.alternate_scroll);
	assert_eq!(replies(&mut t), ["\x1b[?1007;1$y", "\x1b[?1007;2$y"]);
}

#[test]
fn alternate_scroll_is_saved_restored_and_reset() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1007l\x1b[?1007s\x1b[?1007h\x1b[?1007r");
	assert!(!t.alternate_scroll, "restored to off");
	send(&mut t, "\x1bc");
	assert!(t.alternate_scroll, "a full reset turns it back on");
}

#[test]
fn switching_screens_keeps_alternate_scroll() {
	// It is a terminal setting, not part of either screen.
	let mut t = terminal();
	send(&mut t, "\x1b[?1007l\x1b[?1049h");
	assert!(!t.alternate_scroll);
	send(&mut t, "\x1b[?1049l");
	assert!(!t.alternate_scroll);
}
