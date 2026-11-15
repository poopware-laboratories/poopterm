//! The cursor information report: DECRQPSR 1 (`CSI 1 $ w`) answered with
//! DECCIR, and DECRSPS 1 (`DCS 1 $ t … ST`) restoring from it.
use crate::{
	parser::Parser,
	term::{Term, attr::*},
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

/// The payload of the one DECCIR reply.
fn report(term: &mut Term) -> String {
	let replies = ask(term, "\x1b[1$w");
	assert_eq!(replies.len(), 1, "{replies:?}");
	let reply = &replies[0];
	let payload = reply
		.strip_prefix("\x1bP1$u")
		.and_then(|rest| rest.strip_suffix("\x1b\\"));
	payload.expect(reply).to_owned()
}

/// Restores from `report`; DECRSPS never answers.
fn restore(term: &mut Term, report: &str) {
	assert!(ask(term, &format!("\x1bP1$t{report}\x1b\\")).is_empty());
}

const HOME: &str = "1;1;1;@;@;@;0;2;@;BBBB";

#[test]
fn cursor_information_starts_at_home_with_nothing_set() {
	assert_eq!(report(&mut terminal()), HOME);
}

#[test]
fn cursor_information_reports_rendition_flags_and_character_sets() {
	let mut t = terminal();
	// Bold (1), underline (2), blink (4), reverse (8): '@' + 15 = 'O'.
	ask(&mut t, "\x1b[3;7H\x1b[1;4;5;7m");
	assert_eq!(report(&mut t), "3;7;1;O;@;@;0;2;@;BBBB");
	// G1 is DEC graphics and selected into GL with SO.
	ask(&mut t, "\x1b[0;4m\x1b)0\x0e");
	assert_eq!(report(&mut t), "3;7;1;B;@;@;1;2;@;B0BB");
}

#[test]
fn colors_and_other_styles_are_not_part_of_the_rendition() {
	let mut t = terminal();
	ask(&mut t, "\x1b[2;3;8;9;31;42m");
	assert_eq!(report(&mut t), "1;1;1;@;@;@;0;2;@;BBBB");
}

#[test]
fn cursor_information_counts_from_the_margins_in_origin_mode() {
	let mut t = terminal();
	// Origin mode (1) in the flags, the row from the top margin.
	ask(&mut t, "\x1b[2;5r\x1b[?6h\x1b[2;3H");
	assert_eq!(report(&mut t), "2;3;1;@;@;A;0;2;@;BBBB");
}

#[test]
fn cursor_information_reports_a_pending_wrap() {
	let mut t = terminal();
	ask(&mut t, &"x".repeat(20));
	// Wrap pending (8): '@' + 8 = 'H'.
	assert_eq!(report(&mut t), "1;20;1;@;@;H;0;2;@;BBBB");
}

#[test]
fn restoring_cursor_information_round_trips() {
	let mut t = terminal();
	ask(&mut t, "\x1b[2;5r\x1b[?6h\x1b[3;4H\x1b[1;7m\x1b)0\x0e");
	let saved = report(&mut t);
	ask(&mut t, "\x1b[r\x1b[?6l\x1b[H\x1b[m\x1b)B\x0f");
	assert_eq!(report(&mut t), HOME);
	ask(&mut t, "\x1b[2;5r");
	restore(&mut t, &saved);
	assert_eq!(report(&mut t), saved);
	assert!(t.bold && t.reverse && t.origin_mode);
	assert_eq!(
		(t.cy, t.cx),
		(3, 3),
		"row 3 of the region that starts at row 2"
	);
}

#[test]
fn restoring_replaces_the_rendition_and_keeps_the_colors() {
	let mut t = terminal();
	ask(&mut t, "\x1b[1;21;5;7;31m");
	let red = t.fg;
	// Only underline (2) is set: bold, double underline, blink and reverse go.
	restore(&mut t, "1;1;1;B;@;@;0;2;@;BBBB");
	assert!(!t.bold && !t.reverse);
	assert_eq!(t.attrs & (UNDERLINE | DOUBLE_UNDERLINE | BLINK), UNDERLINE);
	assert_eq!(t.fg, red, "DECCIR carries no colors");
	restore(&mut t, "1;1;1;D;@;@;0;2;@;BBBB");
	assert_eq!(t.attrs & (UNDERLINE | DOUBLE_UNDERLINE | BLINK), BLINK);
}

#[test]
fn restored_character_sets_draw_the_next_text() {
	let mut t = terminal();
	// G0 is DEC graphics: `q` draws a horizontal line.
	restore(&mut t, "1;1;1;@;@;@;0;2;@;0BBB");
	ask(&mut t, "q");
	assert_eq!(t.grid[0][0].ch, '─');
	// G1 is DEC graphics and in GL; G0 is ASCII again.
	restore(&mut t, "1;2;1;@;@;@;1;2;@;B0BB");
	ask(&mut t, "q\x0fq");
	assert_eq!((t.grid[0][1].ch, t.grid[0][2].ch), ('─', 'q'));
}

#[test]
fn sets_this_terminal_lacks_restore_as_ascii() {
	let mut t = terminal();
	ask(&mut t, "\x1b(0\x1b)0");
	// British (A) and DEC Supplemental (%5) are not DEC graphics.
	restore(&mut t, "1;1;1;@;@;@;0;2;@;A%5BB");
	assert_eq!(t.charsets, [false, false]);
	// GL 2 and 3 name G2 and G3, which do not exist: GL stays.
	ask(&mut t, "\x0e");
	restore(&mut t, "1;1;1;@;@;@;2;2;@;BBBB");
	assert_eq!(t.active_charset, 1);
}

#[test]
fn restoring_a_pending_wrap_wraps_the_next_character() {
	let mut t = terminal();
	restore(&mut t, "1;20;1;@;@;H;0;2;@;BBBB");
	ask(&mut t, "ab");
	assert_eq!(
		(t.grid[0][19].ch, t.grid[1][0].ch, t.grid[1][1].ch),
		(' ', 'a', 'b')
	);
}

#[test]
fn restoring_cursor_information_clamps_the_position() {
	let mut t = terminal();
	restore(&mut t, "99;0;1;@;@;@;0;2;@;BBBB");
	assert_eq!((t.cy, t.cx), (5, 0));
	// In origin mode, to the bottom margin.
	ask(&mut t, "\x1b[2;4r");
	restore(&mut t, "9;99;1;@;@;A;0;2;@;BBBB");
	assert_eq!((t.cy, t.cx), (3, 19));
}

#[test]
fn malformed_cursor_information_changes_nothing() {
	let mut t = terminal();
	ask(&mut t, "\x1b[2;3H");
	for bad in [
		"",
		"1;1",
		"1;1;1;@;@;@;0;2;@",
		"x;1;1;@;@;@;0;2;@;BBBB",
		"1;;1;@;@;@;0;2;@;BBBB",
		"1;1;1;AB;@;@;0;2;@;BBBB",
		"1;1;1;!;@;@;0;2;@;BBBB",
		"1;1;1;@;@;\x7f;0;2;@;BBBB",
		"1;1;1;@;@;@;x;2;@;BBBB",
		"1;1;1;@;@;@;0;2;@;BBB",
		"1;1;1;@;@;@;0;2;@;BBBB;extra",
	] {
		restore(&mut t, bad);
		assert_eq!(report(&mut t), "2;3;1;@;@;@;0;2;@;BBBB", "{bad:?}");
	}
}

#[test]
fn every_rendition_round_trips() {
	let mut t = terminal();
	for bits in 0..16 {
		let sgr: Vec<&str> = [(1, "1"), (2, "4"), (4, "5"), (8, "7")]
			.iter()
			.filter(|&&(bit, _)| bits & bit != 0)
			.map(|&(_, code)| code)
			.collect();
		ask(&mut t, &format!("\x1b[0;{}m", sgr.join(";")));
		let saved = report(&mut t);
		assert_eq!(
			saved.split(';').nth(3),
			Some(&*char::from(0x40 + bits).to_string())
		);
		ask(&mut t, "\x1b[m");
		restore(&mut t, &saved);
		assert_eq!(report(&mut t), saved, "SGR {sgr:?}");
	}
}

#[test]
fn every_character_set_choice_round_trips() {
	let mut t = terminal();
	for (g0, g1, gl) in [
		("B", "B", 0),
		("0", "B", 0),
		("B", "0", 1),
		("0", "0", 1),
		("0", "B", 1),
	] {
		let shift = if gl == 1 { "\x0e" } else { "\x0f" };
		ask(&mut t, &format!("\x1b({g0}\x1b){g1}{shift}"));
		let saved = report(&mut t);
		assert_eq!(saved, format!("1;1;1;@;@;@;{gl};2;@;{g0}{g1}BB"));
		ask(&mut t, "\x1b(B\x1b)B\x0f");
		restore(&mut t, &saved);
		assert_eq!(report(&mut t), saved);
	}
}

#[test]
fn restoring_leaves_the_saved_cursor_alone() {
	let mut t = terminal();
	// DECSC at row 2, column 2 in bold; DECCIR moves elsewhere without it.
	ask(&mut t, "\x1b[2;2H\x1b[1m\x1b7");
	restore(&mut t, "5;5;1;@;@;@;0;2;@;BBBB");
	assert_eq!((t.cy, t.cx, t.bold), (4, 4, false));
	ask(&mut t, "\x1b8");
	assert_eq!((t.cy, t.cx, t.bold), (1, 1, true));
}

#[test]
fn restoring_without_the_origin_flag_leaves_origin_mode() {
	let mut t = terminal();
	ask(&mut t, "\x1b[3;5r\x1b[?6h");
	restore(&mut t, "2;4;1;@;@;@;0;2;@;BBBB");
	// The position is then counted from the top of the screen.
	assert!(!t.origin_mode);
	assert_eq!((t.cy, t.cx), (1, 3));
}

#[test]
fn flag_bits_this_terminal_lacks_are_ignored() {
	let mut t = terminal();
	// Srend 'P' (16) and Sflag 'F' (2, single shift 2; 4, single shift 3).
	restore(&mut t, "2;2;1;P;@;F;0;2;@;BBBB");
	assert_eq!(report(&mut t), "2;2;1;@;@;@;0;2;@;BBBB");
	// The known bits beside them still apply.
	restore(&mut t, "2;2;1;Q;@;G;0;2;@;BBBB");
	assert!(t.bold && t.origin_mode);
}
