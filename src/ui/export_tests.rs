//! Saving the scrollback: the text a terminal holds, history first, and
//! the dated names of the files it goes to.
use super::{
	broadcast::input_targets,
	export::{file_name, history_text},
};
use crate::{
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};

fn terminal(cols: usize, rows: usize, input: &str) -> Term {
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, input.as_bytes());
	term
}

#[test]
fn history_and_screen_come_out_oldest_first_without_padding() {
	let mut input = String::new();
	for i in 0..8 {
		input.push_str(&format!("line {i}   \r\n"));
	}
	let term = terminal(20, 3, &input);
	assert!(term.scrollback.len() >= 5, "some lines went to history");
	let expected: String = (0..8).map(|i| format!("line {i}\n")).collect();
	assert_eq!(
		history_text(&term),
		expected,
		"blank rows at the end left out"
	);
}

#[test]
fn colors_are_dropped_and_wide_characters_and_marks_kept() {
	let term = terminal(20, 2, "\x1b[1;31mred\x1b[0m 漢字 cafe\u{301}");
	assert_eq!(history_text(&term), "red 漢字 cafe\u{301}\n");
}

#[test]
fn blank_rows_inside_the_text_stay_and_an_empty_terminal_saves_nothing() {
	let term = terminal(10, 5, "a\r\n\r\n\r\nb");
	assert_eq!(history_text(&term), "a\n\n\nb\n");
	assert_eq!(history_text(&terminal(10, 3, "")), "");
	assert_eq!(
		history_text(&terminal(10, 3, "   \r\n  ")),
		"",
		"only blanks"
	);
}

#[test]
fn only_what_history_still_holds_is_saved() {
	let mut term = terminal(10, 2, "");
	term.scrollback_limit = 3;
	let mut parser = Parser::new();
	for i in 0..10 {
		parser.advance(&mut term, format!("n{i}\r\n").as_bytes());
	}
	let text = history_text(&term);
	assert!(!text.contains("n0") && text.contains("n9"), "{text:?}");
	assert_eq!(
		text.lines().count(),
		term.scrollback.len() + 1,
		"history and the last line"
	);
}

#[test]
fn file_names_carry_the_date_and_time_in_utc() {
	assert_eq!(file_name(0), "poopterm-scrollback-19700101-000000.txt");
	assert_eq!(file_name(86_399), "poopterm-scrollback-19700101-235959.txt");
	// 2026-11-22 18:05:01.
	assert_eq!(
		file_name(1_795_370_701),
		"poopterm-scrollback-20261122-180501.txt"
	);
	// Leap days, the 2000 one included (divisible by 400).
	assert_eq!(
		file_name(951_782_400),
		"poopterm-scrollback-20000229-000000.txt"
	);
	assert_eq!(
		file_name(1_709_164_800),
		"poopterm-scrollback-20240229-000000.txt"
	);
	// 2100 is no leap year: the day after 28 February is 1 March.
	assert_eq!(
		file_name(4_107_542_400),
		"poopterm-scrollback-21000301-000000.txt"
	);
	assert_eq!(
		file_name(946_684_799),
		"poopterm-scrollback-19991231-235959.txt"
	);
}

#[test]
fn names_sort_as_the_times_they_were_saved() {
	let times = [
		0u64,
		59,
		3600,
		86_400,
		2_678_400,
		31_536_000,
		1_795_370_701,
		4_107_542_400,
	];
	let names: Vec<String> = times.iter().map(|&t| file_name(t)).collect();
	let mut sorted = names.clone();
	sorted.sort();
	assert_eq!(names, sorted);
}

#[test]
fn input_goes_to_the_focused_pane_or_while_broadcasting_to_all() {
	assert_eq!(input_targets(2, &[1, 2, 3], false), [2]);
	assert_eq!(input_targets(2, &[1, 2, 3], true), [1, 2, 3]);
	assert_eq!(input_targets(7, &[7], true), [7], "one pane");
	// A focused session outside the tab's panes only gets its own input.
	assert_eq!(input_targets(9, &[1, 2], true), [9]);
	assert_eq!(input_targets(4, &[], true), [4]);
}
