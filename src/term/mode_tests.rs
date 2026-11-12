//! Terminal modes: setting, resetting and reporting them.
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

fn row(term: &Term, y: usize) -> String {
	let text: String = term.grid[y].iter().map(|c| c.ch).collect();
	text.trim_end().to_owned()
}

fn replies(term: &mut Term) -> Vec<String> {
	std::mem::take(&mut term.responses)
		.into_iter()
		.map(|reply| String::from_utf8(reply).unwrap())
		.collect()
}

#[test]
fn deckpam_and_deckpnm_switch_the_keypad() {
	let mut t = terminal();
	assert!(!t.application_keypad, "the keypad starts numeric");
	send(&mut t, "\x1b=");
	assert!(t.application_keypad);
	send(&mut t, "\x1b>");
	assert!(!t.application_keypad);
}

#[test]
fn decnkm_is_the_same_keypad_mode() {
	let mut t = terminal();
	send(&mut t, "\x1b[?66h");
	assert!(t.application_keypad);
	send(&mut t, "\x1b[?66$p");
	send(&mut t, "\x1b>\x1b[?66$p");
	assert_eq!(replies(&mut t), ["\x1b[?66;1$y", "\x1b[?66;2$y"]);
	send(&mut t, "\x1b=\x1b[?66l");
	assert!(!t.application_keypad);
}

#[test]
fn soft_and_hard_resets_return_the_keypad_to_numeric() {
	let mut t = terminal();
	send(&mut t, "\x1b=\x1b[!p");
	assert!(!t.application_keypad, "DECSTR");
	send(&mut t, "\x1b=\x1bc");
	assert!(!t.application_keypad, "RIS");
}

#[test]
fn keypad_mode_survives_cursor_save_and_screen_switches() {
	let mut t = terminal();
	send(&mut t, "\x1b7\x1b=\x1b8");
	assert!(t.application_keypad);
	send(&mut t, "\x1b[?1049h");
	assert!(t.application_keypad);
	send(&mut t, "\x1b[?1049l");
	assert!(t.application_keypad);
}

#[test]
fn keypad_escapes_with_intermediates_are_other_sequences() {
	let mut t = terminal();
	send(&mut t, "\x1b(=\x1b#=");
	assert!(!t.application_keypad);
}

#[test]
fn keypad_mode_does_not_move_the_cursor_or_cancel_wrap() {
	let mut t = terminal();
	send(&mut t, "\x1b[1;20HX\x1b=");
	assert_eq!((t.cx, t.cy), (19, 0));
	assert!(t.wrap_pending);
}

#[test]
fn decnkm_combines_with_other_modes_in_one_sequence() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1;66h");
	assert!(t.application_cursor && t.application_keypad);
	send(&mut t, "\x1b[?66;1l");
	assert!(!t.application_cursor && !t.application_keypad);
}

#[test]
fn xtrestore_brings_back_modes_saved_by_xtsave() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1h\x1b[?7l\x1b[?1;7s");
	send(&mut t, "\x1b[?1l\x1b[?7h");
	assert!(!t.application_cursor && t.auto_wrap);
	send(&mut t, "\x1b[?1;7r");
	assert!(t.application_cursor, "?1 was set when saved");
	assert!(!t.auto_wrap, "?7 was reset when saved");
}

#[test]
fn xtrestore_of_modes_never_saved_changes_nothing() {
	let mut t = terminal();
	send(&mut t, "\x1b[?2004h\x1b[?2004r");
	assert!(t.bracketed_paste);
	send(&mut t, "\x1b[?25s\x1b[?25l\x1b[?2004;25r");
	assert!(t.bracketed_paste, "?2004 was not saved");
	assert!(t.cursor_visible, "?25 was");
}

#[test]
fn xtsave_keeps_only_the_latest_value_of_each_mode() {
	let mut t = terminal();
	send(
		&mut t,
		"\x1b[?1004h\x1b[?1004s\x1b[?1004l\x1b[?1004s\x1b[?1004h",
	);
	send(&mut t, "\x1b[?1004r");
	assert!(!t.focus_reporting);
	assert_eq!(t.saved_modes.len(), 1);
}

#[test]
fn xtsave_ignores_modes_poopterm_does_not_have() {
	let mut t = terminal();
	send(&mut t, "\x1b[?9999;31337;1s");
	assert_eq!(t.saved_modes.len(), 1, "only ?1 is known");
	send(&mut t, "\x1b[?9999r");
	assert!(!t.application_cursor);
}

#[test]
fn xtsave_without_parameters_saves_nothing() {
	let mut t = terminal();
	send(&mut t, "\x1b[?s\x1b[?r");
	assert!(t.saved_modes.is_empty());
	assert!(t.auto_wrap && t.cursor_visible);
}

#[test]
fn xtrestore_brings_back_the_mouse_tracking_mode() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1002h\x1b[?1000;1002;1003s\x1b[?1003h");
	assert_eq!(t.mouse_tracking, 1003);
	send(&mut t, "\x1b[?1000;1002;1003r");
	assert_eq!(t.mouse_tracking, 1002);
}

#[test]
fn csi_s_without_question_mark_still_saves_the_cursor() {
	let mut t = terminal();
	send(&mut t, "\x1b[3;4H\x1b[s\x1b[1;1H\x1b[u");
	assert_eq!((t.cx, t.cy), (3, 2));
	assert!(t.saved_modes.is_empty());
}

#[test]
fn newline_mode_makes_line_feeds_return_the_carriage() {
	let mut t = terminal();
	send(&mut t, "\x1b[20hab\ncd\x0bef\x0cg");
	assert_eq!(row(&t, 0), "ab");
	assert_eq!(row(&t, 1), "cd");
	assert_eq!(row(&t, 2), "ef");
	assert_eq!(row(&t, 3), "g");
	assert!(t.newline_mode);
}

#[test]
fn index_ignores_newline_mode() {
	let mut t = terminal();
	send(&mut t, "\x1b[20hab\x1bDc");
	assert_eq!(row(&t, 1), "  c");
}

#[test]
fn resetting_newline_mode_restores_plain_line_feeds() {
	let mut t = terminal();
	send(&mut t, "\x1b[20h\x1b[20lab\nc");
	assert_eq!(row(&t, 1), "  c");
}

#[test]
fn ansi_modes_combine_in_one_sequence() {
	let mut t = terminal();
	send(&mut t, "\x1b[4;20h");
	assert!(t.insert_mode && t.newline_mode);
	send(&mut t, "\x1b[20;4l");
	assert!(!t.insert_mode && !t.newline_mode);
}

#[test]
fn keyboard_action_and_send_receive_modes_are_kept() {
	let mut t = terminal();
	assert!(!t.keyboard_locked && !t.local_echo);
	send(&mut t, "\x1b[2h\x1b[12l");
	assert!(t.keyboard_locked, "KAM locks the keyboard");
	assert!(t.local_echo, "resetting SRM echoes input locally");
	send(&mut t, "\x1b[2l\x1b[12h");
	assert!(!t.keyboard_locked && !t.local_echo);
}

#[test]
fn decrqm_reports_ansi_modes() {
	let mut t = terminal();
	send(&mut t, "\x1b[4h\x1b[4$p\x1b[20$p\x1b[2$p\x1b[12$p\x1b[99$p");
	assert_eq!(
		replies(&mut t),
		[
			"\x1b[4;1$y",
			"\x1b[20;2$y",
			"\x1b[2;2$y",
			"\x1b[12;1$y",
			"\x1b[99;0$y"
		]
	);
}

#[test]
fn decrqm_for_ansi_modes_answers_each_request_once() {
	let mut t = terminal();
	send(&mut t, "\x1b[$p\x1b[20h\x1b[20$p");
	assert_eq!(replies(&mut t), ["\x1b[0;0$y", "\x1b[20;1$y"]);
}

#[test]
fn decstr_unlocks_the_keyboard_and_leaves_newline_mode() {
	let mut t = terminal();
	send(&mut t, "\x1b[2;4;20h\x1b[!p");
	assert!(!t.keyboard_locked && !t.insert_mode);
	assert!(t.newline_mode);
	send(&mut t, "\x1bc");
	assert!(!t.newline_mode, "RIS resets everything");
}

#[test]
fn local_echo_shows_typed_input() {
	let mut t = terminal();
	t.echo_input(b"hi\r\nyo");
	assert_eq!(row(&t, 0), "hi");
	assert_eq!(row(&t, 1), "yo");
	assert!(t.responses.is_empty());
}

#[test]
fn xtrestore_applies_side_effects_of_the_restored_mode() {
	let mut t = terminal();
	send(&mut t, "\x1b[2;4r\x1b[?6s\x1b[?6h\x1b[3;5H");
	assert_eq!((t.cx, t.cy), (4, 3));
	send(&mut t, "\x1b[?6r");
	assert!(!t.origin_mode);
	assert_eq!((t.cx, t.cy), (0, 0), "resetting DECOM homes the cursor");
}

#[test]
fn xtrestore_can_leave_the_alternate_screen() {
	let mut t = terminal();
	send(&mut t, "shell\x1b[?1049s\x1b[?1049hfull screen");
	assert!(t.primary_screen.is_some());
	send(&mut t, "\x1b[?1049r");
	assert!(t.primary_screen.is_none());
	assert_eq!(row(&t, 0), "shell");
}

#[test]
fn saved_modes_can_be_restored_more_than_once() {
	let mut t = terminal();
	send(&mut t, "\x1b[?2004;66h\x1b[?2004;66s");
	for _ in 0..2 {
		send(&mut t, "\x1b[?2004;66l\x1b[?66;2004r");
		assert!(t.bracketed_paste && t.application_keypad);
	}
}

#[test]
fn ris_forgets_saved_modes() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1h\x1b[?1s\x1bc\x1b[?1r");
	assert!(t.saved_modes.is_empty());
	assert!(!t.application_cursor);
}

#[test]
fn saved_modes_stay_bounded_by_the_known_modes() {
	let mut t = terminal();
	let all = "1;5;6;7;9;25;47;66;80;1000;1002;1003;1004;1005;1006;1007;1015;1016;1047;1049;2004";
	for _ in 0..3 {
		send(&mut t, &format!("\x1b[?{all};3;31337s"));
	}
	assert_eq!(t.saved_modes.len(), all.split(';').count());
}

#[test]
fn newline_mode_scrolls_at_the_bottom_margin_and_returns() {
	let mut t = terminal();
	send(&mut t, "\x1b[20h\x1b[1;2r\x1b[2;5Hxy\n");
	assert_eq!((t.cx, t.cy), (0, 1));
	assert_eq!(row(&t, 0), "    xy", "the region scrolled up");
	assert_eq!(row(&t, 1), "");
}

#[test]
fn newline_mode_line_feed_cancels_a_pending_wrap() {
	let mut t = terminal();
	send(&mut t, "\x1b[20h\x1b[1;20HZ\nq");
	assert_eq!(row(&t, 1), "q");
	assert!(t.grid[0][19].ch == 'Z');
}

#[test]
fn newline_mode_leaves_carriage_return_and_nel_alone() {
	let mut t = terminal();
	send(&mut t, "\x1b[20habc\rX\x1bEY");
	assert_eq!(row(&t, 0), "Xbc");
	assert_eq!(row(&t, 1), "Y");
	assert_eq!((t.cx, t.cy), (1, 1));
}

#[test]
fn private_mode_numbers_do_not_set_ansi_modes() {
	let mut t = terminal();
	send(&mut t, "\x1b[?20h\x1b[?4h\x1b[?2h\x1b[?12l");
	assert!(!t.newline_mode && !t.insert_mode);
	assert!(!t.keyboard_locked && !t.local_echo);
}

#[test]
fn decrqm_keeps_private_and_ansi_mode_numbers_apart() {
	let mut t = terminal();
	send(&mut t, "\x1b[20h\x1b[?20$p\x1b[20$p\x1b[?1h\x1b[1$p");
	assert_eq!(
		replies(&mut t),
		["\x1b[?20;0$y", "\x1b[20;1$y", "\x1b[1;0$y"]
	);
}

#[test]
fn decrqm_reports_kam_and_srm_after_changes() {
	let mut t = terminal();
	send(&mut t, "\x1b[2h\x1b[12l\x1b[2$p\x1b[12$p");
	assert_eq!(replies(&mut t), ["\x1b[2;1$y", "\x1b[12;2$y"]);
}

#[test]
fn local_echo_runs_echoed_escape_sequences() {
	let mut t = terminal();
	t.echo_input(b"abc\x1b[D\x1b[DX");
	assert_eq!(row(&t, 0), "aXc", "cursor keys move the cursor locally");
	t.echo_input(b"\x1b[31mr");
	assert_eq!(row(&t, 0), "aXr");
	assert_eq!(t.grid[0][2].fg, t.indexed_color(1));
}

#[test]
fn local_echo_of_enter_in_newline_mode_starts_a_new_line() {
	let mut t = terminal();
	send(&mut t, "\x1b[20h");
	t.echo_input(b"ab\r\ncd");
	assert_eq!(row(&t, 0), "ab");
	assert_eq!(row(&t, 1), "cd");
}

#[test]
fn local_echo_is_independent_of_the_programs_parser_state() {
	let mut t = terminal();
	// The program's output stopped inside a sequence; the echo still prints.
	send(&mut t, "\x1b[3");
	t.echo_input(b"k");
	assert_eq!(row(&t, 0), "k");
}

#[test]
fn decstr_keeps_modes_saved_by_xtsave() {
	let mut t = terminal();
	send(&mut t, "\x1b[?1h\x1b[?1s\x1b[!p");
	assert!(!t.application_cursor);
	send(&mut t, "\x1b[?1r");
	assert!(t.application_cursor);
}

#[test]
fn insert_mode_set_through_sm_still_shifts_text() {
	let mut t = terminal();
	send(&mut t, "abc\r\x1b[3;4;99hX");
	assert_eq!(row(&t, 0), "Xabc", "unknown modes 3 and 99 are skipped");
	send(&mut t, "\x1b[4lY");
	assert_eq!(row(&t, 0), "XYbc");
}

#[test]
fn a_locked_keyboard_does_not_stop_output_or_replies() {
	let mut t = terminal();
	send(&mut t, "\x1b[2hout\x1b[5n");
	assert_eq!(row(&t, 0), "out");
	assert_eq!(replies(&mut t), ["\x1b[0n"]);
}
