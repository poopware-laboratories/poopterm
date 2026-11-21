//! Selecting with clicks: counting double and triple clicks, and the word
//! or row each selects.
use super::{
	click_select::{Clicks, selection},
	selection::{CellPoint, Selection},
	window_pointer::DOUBLE_CLICK,
};
use std::time::{Duration, Instant};

fn point(row: usize, col: usize) -> CellPoint {
	CellPoint::new(row, col)
}

#[test]
fn quick_presses_on_one_cell_count_up_to_three_and_start_over() {
	let (mut clicks, start) = (Clicks::default(), Instant::now());
	let at = |ms: u64| start + Duration::from_millis(ms);
	let counts: Vec<u8> = [0, 100, 200, 300, 400]
		.iter()
		.map(|&ms| clicks.press(at(ms), point(2, 5)))
		.collect();
	assert_eq!(counts, [1, 2, 3, 1, 2]);
}

#[test]
fn a_slow_press_or_another_cell_counts_as_a_first_click() {
	let (mut clicks, start) = (Clicks::default(), Instant::now());
	assert_eq!(clicks.press(start, point(0, 0)), 1);
	assert_eq!(
		clicks.press(start + DOUBLE_CLICK, point(0, 0)),
		2,
		"exactly at the limit"
	);
	let late = start + DOUBLE_CLICK * 2 + Duration::from_millis(1);
	assert_eq!(clicks.press(late, point(0, 0)), 1, "too slow");
	assert_eq!(clicks.press(late, point(0, 1)), 1, "another column");
	assert_eq!(clicks.press(late, point(1, 1)), 1, "another row");
	// A press with a clock that went backwards is no repeat.
	assert_eq!(clicks.press(start, point(1, 1)), 1);
}

/// What a press counted `count` times at column `col` of `text` selects,
/// as (first, last) columns.
fn picked(count: u8, text: &str, col: usize) -> Option<(usize, usize)> {
	let chars: Vec<char> = text.chars().collect();
	selection(count, &chars, point(7, col)).map(|s| {
		assert_eq!((s.start.row, s.end.row, s.block), (7, 7, false));
		(s.start.col, s.end.col)
	})
}

#[test]
fn a_double_click_takes_the_whole_word_around_it() {
	let line = "ls --color=auto src/app/mod.rs:42 ok";
	assert_eq!(picked(2, line, 0), Some((0, 1)), "ls");
	assert_eq!(
		picked(2, line, 6),
		Some((3, 14)),
		"an option with its value"
	);
	assert_eq!(picked(2, line, 20), Some((16, 32)), "a path with its line");
	assert_eq!(
		picked(2, line, 35),
		Some((34, 35)),
		"the last word, to the row's end"
	);
	assert_eq!(picked(2, "user@host:~/x$ ", 3), Some((0, 13)));
	assert_eq!(picked(2, "https://a.b/c?d=1#e", 9), Some((0, 18)), "a URL");
}

#[test]
fn spaces_select_their_run_and_other_characters_themselves() {
	assert_eq!(
		picked(2, "a    b", 2),
		Some((1, 4)),
		"the spaces between words"
	);
	assert_eq!(picked(2, "f(x, y)", 1), Some((1, 1)), "a bracket alone");
	assert_eq!(picked(2, "x));", 2), Some((2, 2)), "one of several");
	assert_eq!(
		picked(2, "a\0\0b", 1),
		Some((1, 2)),
		"empty cells count as spaces"
	);
}

#[test]
fn letters_of_any_script_and_wide_characters_are_words() {
	assert_eq!(picked(2, "voilà été", 2), Some((0, 4)));
	// A wide character fills two cells, both holding it.
	assert_eq!(picked(2, "漢漢字字 x", 1), Some((0, 3)));
	assert_eq!(picked(2, "é", 0), Some((0, 0)));
}

#[test]
fn a_triple_click_takes_the_row_and_single_clicks_nothing() {
	assert_eq!(picked(3, "first second", 8), Some((0, 11)));
	assert_eq!(
		picked(3, "    ", 0),
		None,
		"a blank row has nothing to take"
	);
	assert_eq!(
		picked(3, "  ab  \0\0", 0),
		Some((0, 3)),
		"up to the last character"
	);
	assert_eq!(picked(1, "word", 1), None);
	assert_eq!(picked(4, "word", 1), None);
}

#[test]
fn a_click_past_the_row_or_on_an_empty_row_is_clamped_or_nothing() {
	assert_eq!(
		picked(2, "abc def", 40),
		Some((4, 6)),
		"past the end: the last word"
	);
	assert_eq!(picked(2, "", 0), None);
	assert_eq!(picked(3, "", 0), None);
	let row: Vec<char> = "ab".chars().collect();
	let selected = selection(2, &row, point(3, 1)).unwrap();
	assert_eq!(
		selected,
		Selection {
			start: point(3, 0),
			end: point(3, 1),
			block: false
		}
	);
}

#[test]
fn a_wide_character_fills_both_its_cells_and_selects_whole() {
	use crate::{
		parser::Parser,
		term::Term,
		theme::{Theme, ThemeKind},
	};
	let mut term = Term::new(10, 1, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, "a漢字 b".as_bytes());
	let chars = super::click_select::chars(term.row_cells(0).unwrap());
	assert_eq!(chars[..6], ['a', '漢', '漢', '字', '字', ' ']);
	// A double click on the second cell of 字 takes "a漢字".
	let picked = selection(2, &chars, point(0, 4)).unwrap();
	assert_eq!((picked.start.col, picked.end.col), (0, 4));
	assert!(super::click_select::chars(&[]).is_empty());
}

#[test]
fn listings_mail_and_lists_split_where_a_reader_expects() {
	let ls = "-rw-r--r--  1 me  staff  1.2K Nov 21 10:00 notes.txt";
	assert_eq!(picked(2, ls, 4), Some((0, 9)), "permissions are one word");
	assert_eq!(picked(2, ls, 26), Some((25, 28)), "a size with its unit");
	assert_eq!(picked(2, ls, 40), Some((37, 41)), "a time");
	assert_eq!(picked(2, ls, 46), Some((43, 51)), "the file name");
	assert_eq!(
		picked(2, "mail me@example.org now", 8),
		Some((5, 18)),
		"an address"
	);
	// Commas and semicolons end words: lists split into their items.
	assert_eq!(picked(2, "a=1,b=2;c=3", 4), Some((4, 6)));
	assert_eq!(picked(2, "a=1,b=2;c=3", 3), Some((3, 3)), "the comma alone");
	assert_eq!(picked(2, "   indented", 1), Some((0, 2)), "leading spaces");
}
