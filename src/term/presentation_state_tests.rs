//! Presentation state reports: DECRQPSR (`CSI 2 $ w`) answered with the tab
//! stops (DECTABSR), and DECRSPS (`DCS 2 $ t … ST`) restoring them.
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize) -> Term {
	Term::new(cols, 6, Theme::from_kind(ThemeKind::Poop), "/".into())
}

/// The replies to `bytes`, in order.
fn ask(term: &mut Term, bytes: &str) -> Vec<String> {
	Parser::new().advance(term, bytes.as_bytes());
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

/// The payload of the one DECTABSR reply.
fn tab_report(term: &mut Term) -> String {
	let replies = ask(term, "\x1b[2$w");
	assert_eq!(replies.len(), 1, "{replies:?}");
	let reply = &replies[0];
	let payload = reply
		.strip_prefix("\x1bP2$u")
		.and_then(|rest| rest.strip_suffix("\x1b\\"));
	payload.expect(reply).to_owned()
}

/// Restores the tab stops from `report`; DECRSPS never answers.
fn restore(term: &mut Term, report: &str) {
	assert!(ask(term, &format!("\x1bP2$t{report}\x1b\\")).is_empty());
}

#[test]
fn tab_stop_reports_list_the_stops_from_column_one() {
	let mut t = terminal(20);
	assert_eq!(tab_report(&mut t), "9/17");
	// HTS at column 4, TBC at column 9.
	ask(&mut t, "\x1b[1;4H\x1bH\x1b[1;9H\x1b[g");
	assert_eq!(tab_report(&mut t), "4/17");
	ask(&mut t, "\x1b[3g");
	assert_eq!(tab_report(&mut t), "");
}

#[test]
fn a_wide_screen_reports_every_stop() {
	let mut t = terminal(80);
	assert_eq!(tab_report(&mut t), "9/17/25/33/41/49/57/65/73");
	// HTS in the last column.
	ask(&mut t, "\x1b[1;80H\x1bH");
	assert_eq!(tab_report(&mut t), "9/17/25/33/41/49/57/65/73/80");
}

#[test]
fn restoring_tab_stops_replaces_them_all() {
	let mut t = terminal(20);
	restore(&mut t, "3/5/40");
	// Column 40 is off the screen.
	assert_eq!(tab_report(&mut t), "3/5");
	ask(&mut t, "\r\tA\tB");
	assert_eq!((t.grid[0][2].ch, t.grid[0][4].ch), ('A', 'B'));
	// An empty list clears every stop; junk entries are skipped.
	restore(&mut t, "");
	assert_eq!(tab_report(&mut t), "");
	restore(&mut t, "2/x/0/7");
	assert_eq!(tab_report(&mut t), "2/7");
}

#[test]
fn restored_stops_may_come_in_any_order_and_repeat() {
	let mut t = terminal(20);
	restore(&mut t, "17/3/3/20//1");
	assert_eq!(tab_report(&mut t), "1/3/17/20");
}

#[test]
fn a_report_restores_what_it_reported() {
	let mut t = terminal(40);
	ask(
		&mut t,
		"\x1b[3g\x1b[1;6H\x1bH\x1b[1;11H\x1bH\x1b[1;33H\x1bH",
	);
	let saved = tab_report(&mut t);
	assert_eq!(saved, "6/11/33");
	ask(&mut t, "\x1b[3g\x1b[1;2H\x1bH");
	restore(&mut t, &saved);
	assert_eq!(tab_report(&mut t), saved);
}

#[test]
fn restored_stops_drive_every_tab_movement() {
	let mut t = terminal(20);
	restore(&mut t, "4/9/15");
	// HT to column 4, CHT 2 to column 15, CBT 1 back to column 9.
	ask(&mut t, "\r\t");
	assert_eq!(t.cx, 3);
	ask(&mut t, "\x1b[2I");
	assert_eq!(t.cx, 14);
	ask(&mut t, "\x1b[Z");
	assert_eq!(t.cx, 8);
	// With no stop after column 15, HT goes to the last column.
	ask(&mut t, "\x1b[1;16H\t");
	assert_eq!(t.cx, 19);
}

#[test]
fn restored_stops_survive_a_soft_reset_but_not_a_full_one() {
	let mut t = terminal(20);
	restore(&mut t, "5");
	ask(&mut t, "\x1b[!p");
	assert_eq!(tab_report(&mut t), "5", "DECSTR leaves the tab stops");
	ask(&mut t, "\x1bc");
	assert_eq!(
		tab_report(&mut t),
		"9/17",
		"RIS restores every eighth column"
	);
}

#[test]
fn restored_stops_are_shared_with_the_alternate_screen() {
	let mut t = terminal(20);
	ask(&mut t, "\x1b[?1049h");
	restore(&mut t, "6/12");
	ask(&mut t, "\x1b[?1049l");
	assert_eq!(tab_report(&mut t), "6/12");
}

#[test]
fn resizing_keeps_restored_stops_on_the_screen() {
	let mut t = terminal(20);
	restore(&mut t, "3/18");
	// Narrower: column 18 goes; wider again: the new columns get the
	// default stops, the old ones keep theirs.
	t.resize(12, 6);
	assert_eq!(tab_report(&mut t), "3");
	t.resize(30, 6);
	assert_eq!(tab_report(&mut t), "3/17/25");
}

#[test]
fn an_oversized_restore_changes_nothing() {
	let mut t = terminal(20);
	let report = "1/".repeat(super::dcs::MAX_REQUEST / 2 + 1);
	restore(&mut t, &report);
	assert_eq!(tab_report(&mut t), "9/17");
	// Within the limit, the same entries are applied.
	restore(&mut t, &"1/".repeat(super::dcs::MAX_REQUEST / 2));
	assert_eq!(tab_report(&mut t), "1");
}

#[test]
fn other_presentation_reports_are_not_answered() {
	let mut t = terminal(20);
	for request in ["\x1b[$w", "\x1b[0$w", "\x1b[3$w", "\x1b[?2$w", "\x1b[?1$w"] {
		assert!(ask(&mut t, request).is_empty(), "{request:?}");
	}
	// DECRSPS with another number, none, or more than one restores nothing,
	// and a tab report given as cursor information is malformed.
	for request in [
		"\x1bP3$t1/2\x1b\\",
		"\x1bP$t1/2\x1b\\",
		"\x1bP2;2$t1/2\x1b\\",
		"\x1bP1$t1/2\x1b\\",
	] {
		assert!(ask(&mut t, request).is_empty(), "{request:?}");
		assert_eq!(tab_report(&mut t), "9/17", "{request:?}");
	}
}

#[test]
fn a_restore_split_across_reads_applies_when_the_string_ends() {
	let mut t = terminal(20);
	let mut parser = Parser::new();
	for chunk in ["\x1bP2", "$t4/1", "2"] {
		parser.advance(&mut t, chunk.as_bytes());
		assert_eq!((t.tab_stops[3], t.tab_stops[8]), (false, true), "{chunk:?}");
	}
	// The ESC of ST ends the string; the `\` after it changes nothing.
	parser.advance(&mut t, b"\x1b");
	assert_eq!(tab_report(&mut t), "4/12");
	parser.advance(&mut t, b"\\");
	assert_eq!(tab_report(&mut t), "4/12");
}
