//! DECDSR (`CSI ? Ps n`): the status reports about hardware Poopterm does
//! not have (printer, user-defined keys, locator, macros, sessions).
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
fn hardware_status_reports_answer_as_a_terminal_without_it() {
	let mut t = terminal();
	for (request, reply) in [
		("\x1b[?15n", "\x1b[?13n"),   // no printer
		("\x1b[?25n", "\x1b[?21n"),   // user-defined keys locked
		("\x1b[?53n", "\x1b[?50n"),   // no locator
		("\x1b[?55n", "\x1b[?50n"),   // the same, as xterm numbers it
		("\x1b[?56n", "\x1b[?57;0n"), // locator type unknown
		("\x1b[?62n", "\x1b[0*{"),    // DECMSR: no macro space
		("\x1b[?75n", "\x1b[?70n"),   // data integrity: no errors
		("\x1b[?85n", "\x1b[?83n"),   // not set up for multiple sessions
	] {
		assert_eq!(ask(&mut t, request), [reply], "{request:?}");
	}
}

#[test]
fn the_macro_checksum_echoes_the_request_id() {
	let mut t = terminal();
	assert_eq!(ask(&mut t, "\x1b[?63;123n"), ["\x1bP123!~0000\x1b\\"]);
	assert_eq!(ask(&mut t, "\x1b[?63n"), ["\x1bP0!~0000\x1b\\"]);
	assert_eq!(ask(&mut t, "\x1b[?63;65535n"), ["\x1bP65535!~0000\x1b\\"]);
}

#[test]
fn a_second_parameter_only_matters_to_the_checksum() {
	let mut t = terminal();
	assert_eq!(ask(&mut t, "\x1b[?15;99n"), ["\x1b[?13n"]);
	assert_eq!(ask(&mut t, "\x1b[?85;1n"), ["\x1b[?83n"]);
}

#[test]
fn ansi_status_requests_with_the_same_numbers_go_unanswered() {
	let mut t = terminal();
	for code in [15, 25, 53, 56, 62, 63, 75, 85] {
		assert_eq!(
			ask(&mut t, &format!("\x1b[{code}n")),
			Vec::<String>::new(),
			"{code}"
		);
	}
	// The ANSI ones that exist still answer.
	assert_eq!(ask(&mut t, "\x1b[5n"), ["\x1b[0n"]);
}

#[test]
fn unknown_dec_status_requests_go_unanswered() {
	let mut t = terminal();
	for code in [0, 1, 14, 16, 24, 54, 57, 64, 76, 84, 86, 9999] {
		assert_eq!(
			ask(&mut t, &format!("\x1b[?{code}n")),
			Vec::<String>::new(),
			"{code}"
		);
	}
	// The cursor report is unchanged.
	assert_eq!(ask(&mut t, "\x1b[2;3H\x1b[?6n"), ["\x1b[?2;3;1R"]);
}
