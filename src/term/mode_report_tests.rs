//! DECRQM answers for modes Poopterm only keeps or never allows: ANSI and
//! DEC modes that are permanently reset, and DEC modes stored so programs
//! read back what they set.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal() -> Term {
	Term::new(20, 5, Theme::from_kind(ThemeKind::Poop), "/".into())
}

/// The replies to `bytes`, in order.
fn ask(term: &mut Term, bytes: &str) -> Vec<String> {
	Parser::new().advance(term, bytes.as_bytes());
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

/// GATM, SRTM, VEM, HEM, PUM, FEAM, FETM, MATM, TTM, SATM, TSM and EBM.
const PERMANENT_ANSI: [u16; 12] = [1, 5, 7, 10, 11, 13, 14, 15, 16, 17, 18, 19];

/// DECSCLM, DECPFF, DECPEX, DECHEBM and DECNRCM.
const STORED_DEC: [u16; 5] = [4, 18, 19, 35, 42];

#[test]
fn ansi_modes_without_an_effect_are_permanently_reset() {
	let mut t = terminal();
	for mode in PERMANENT_ANSI {
		assert_eq!(
			ask(&mut t, &format!("\x1b[{mode}$p")),
			[format!("\x1b[{mode};4$y")]
		);
		// Setting one changes nothing.
		assert_eq!(
			ask(&mut t, &format!("\x1b[{mode}h\x1b[{mode}$p")),
			[format!("\x1b[{mode};4$y")]
		);
	}
}

#[test]
fn ansi_modes_that_work_still_report_their_state() {
	let mut t = terminal();
	assert_eq!(ask(&mut t, "\x1b[4h\x1b[4$p"), ["\x1b[4;1$y"]);
	assert_eq!(ask(&mut t, "\x1b[4l\x1b[4$p"), ["\x1b[4;2$y"]);
	assert_eq!(ask(&mut t, "\x1b[20$p"), ["\x1b[20;2$y"]);
	// Modes that were never defined stay unrecognized.
	for mode in [0, 3, 6, 8, 9, 21, 9999] {
		assert_eq!(
			ask(&mut t, &format!("\x1b[{mode}$p")),
			[format!("\x1b[{mode};0$y")]
		);
	}
}

#[test]
fn horizontal_cursor_coupling_is_permanently_reset() {
	let mut t = terminal();
	assert_eq!(ask(&mut t, "\x1b[?60$p"), ["\x1b[?60;4$y"]);
	assert_eq!(ask(&mut t, "\x1b[?60h\x1b[?60$p"), ["\x1b[?60;4$y"]);
}

#[test]
fn stored_dec_modes_start_reset_and_follow_decset_and_decrst() {
	let mut t = terminal();
	for mode in STORED_DEC {
		assert_eq!(
			ask(&mut t, &format!("\x1b[?{mode}$p")),
			[format!("\x1b[?{mode};2$y")]
		);
		assert_eq!(
			ask(&mut t, &format!("\x1b[?{mode}h\x1b[?{mode}$p")),
			[format!("\x1b[?{mode};1$y")]
		);
	}
	// Each mode is its own flag: resetting one leaves the others set.
	assert_eq!(
		ask(
			&mut t,
			"\x1b[?18l\x1b[?4$p\x1b[?18$p\x1b[?19$p\x1b[?35$p\x1b[?42$p"
		),
		[
			"\x1b[?4;1$y",
			"\x1b[?18;2$y",
			"\x1b[?19;1$y",
			"\x1b[?35;1$y",
			"\x1b[?42;1$y"
		]
	);
}

#[test]
fn one_query_answers_a_mix_of_mode_kinds() {
	let mut t = terminal();
	assert_eq!(
		ask(&mut t, "\x1b[?35h\x1b[?35;60;7;9999$p"),
		[
			"\x1b[?35;1$y",
			"\x1b[?60;4$y",
			"\x1b[?7;1$y",
			"\x1b[?9999;0$y"
		]
	);
}

#[test]
fn xtsave_and_xtrestore_keep_stored_modes() {
	let mut t = terminal();
	ask(&mut t, "\x1b[?4;42h\x1b[?4;42;18s\x1b[?4;42l\x1b[?18h");
	assert_eq!(
		ask(&mut t, "\x1b[?4;42;18r\x1b[?4$p\x1b[?42$p\x1b[?18$p"),
		["\x1b[?4;1$y", "\x1b[?42;1$y", "\x1b[?18;2$y"]
	);
}

#[test]
fn a_full_reset_clears_stored_modes() {
	let mut t = terminal();
	ask(&mut t, "\x1b[?4;18;19;35;42h");
	ask(&mut t, "\x1bc");
	for mode in STORED_DEC {
		assert_eq!(
			ask(&mut t, &format!("\x1b[?{mode}$p")),
			[format!("\x1b[?{mode};2$y")]
		);
	}
}

#[test]
fn permanent_modes_belong_to_one_namespace_each() {
	let mut t = terminal();
	// ?1 is DECCKM, which works; ANSI 60 was never defined.
	assert_eq!(
		ask(&mut t, "\x1b[?1$p\x1b[60$p"),
		["\x1b[?1;2$y", "\x1b[60;0$y"]
	);
	// ?18 is a stored DEC mode while ANSI 18 (TSM) is permanently reset.
	assert_eq!(
		ask(&mut t, "\x1b[?18h\x1b[18$p\x1b[?18$p"),
		["\x1b[18;4$y", "\x1b[?18;1$y"]
	);
}

#[test]
fn decarm_stays_a_keyboard_mode() {
	let mut t = terminal();
	assert_eq!(ask(&mut t, "\x1b[?8l\x1b[?8$p"), ["\x1b[?8;2$y"]);
	assert!(!t.keyboard.auto_repeat, "key repeats are dropped");
}

#[test]
fn a_soft_reset_keeps_stored_modes() {
	let mut t = terminal();
	assert_eq!(ask(&mut t, "\x1b[?42h\x1b[!p\x1b[?42$p"), ["\x1b[?42;1$y"]);
}

#[test]
fn restoring_a_permanently_reset_mode_leaves_it_reset() {
	let mut t = terminal();
	assert_eq!(
		ask(&mut t, "\x1b[?60h\x1b[?60s\x1b[?60r\x1b[?60$p"),
		["\x1b[?60;4$y"]
	);
}
