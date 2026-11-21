//! Selecting with clicks, end to end and for any input: what a double or
//! triple click on a real terminal copies, words that are as long as they
//! can be and no longer, and press counting against a plain reference.
use super::{
	click_select::{Clicks, chars, selection},
	copy::selection_text,
	selection::{CellPoint, Selection},
	window_pointer::DOUBLE_CLICK,
};
use crate::{
	bench::corpus::Rng,
	parser::Parser,
	term::Term,
	theme::{Theme, ThemeKind},
};
use std::time::{Duration, Instant};

fn terminal(cols: usize, rows: usize, input: &str) -> Term {
	let mut term = Term::new(cols, rows, Theme::from_kind(ThemeKind::Poop), "/".into());
	Parser::new().advance(&mut term, input.as_bytes());
	term
}

/// What a press counted `count` times on row `row`, column `col` of `term`
/// copies, as the app does it: the model's selection in lines, then text.
fn copied(term: &Term, count: u8, (row, col): (usize, usize)) -> Option<String> {
	let point = CellPoint::new(row, col);
	let row_chars = chars(term.row_cells(row)?);
	let selected = selection(count, &row_chars, point)?;
	selection_text(term, selected.in_lines(term.history_base))
}

#[test]
fn a_double_click_on_a_terminal_copies_the_word_under_it() {
	let term = terminal(
		40,
		3,
		"$ cat src/app/mod.rs:42 | less\r\nerror: --color=auto",
	);
	assert_eq!(
		copied(&term, 2, (0, 10)).as_deref(),
		Some("src/app/mod.rs:42")
	);
	assert_eq!(copied(&term, 2, (0, 0)).as_deref(), Some("$"));
	assert_eq!(copied(&term, 2, (1, 12)).as_deref(), Some("--color=auto"));
	assert_eq!(
		copied(&term, 2, (1, 4)).as_deref(),
		Some("error:"),
		"a colon joins the word"
	);
}

#[test]
fn a_triple_click_copies_the_row_without_its_trailing_blanks() {
	let term = terminal(20, 2, "  hello world\r\n");
	assert_eq!(copied(&term, 3, (0, 15)).as_deref(), Some("  hello world"));
	assert_eq!(
		copied(&term, 3, (1, 0)),
		None,
		"an empty row selects nothing"
	);
}

#[test]
fn combining_marks_and_wide_characters_are_copied_with_their_word() {
	let term = terminal(20, 1, "cafe\u{301} 漢字x !");
	assert_eq!(copied(&term, 2, (0, 1)).as_deref(), Some("cafe\u{301}"));
	assert_eq!(
		copied(&term, 2, (0, 7)).as_deref(),
		Some("漢字x"),
		"clicked on 字's second cell"
	);
}

#[test]
fn words_in_scrollback_are_copied_after_history_was_evicted() {
	let mut term = terminal(20, 2, "");
	term.scrollback_limit = 3;
	let mut parser = Parser::new();
	for i in 0..10 {
		parser.advance(&mut term, format!("line-{i} tail\r\n").as_bytes());
	}
	assert!(term.history_base > 0, "older lines were evicted");
	// Row 0 is the oldest kept line.
	let oldest = copied(&term, 2, (0, 2)).unwrap();
	assert!(oldest.starts_with("line-") && oldest.len() == 6, "{oldest}");
	assert_eq!(copied(&term, 3, (0, 0)), Some(format!("{oldest} tail")));
}

#[test]
fn rows_and_lines_convert_both_ways() {
	let selection = Selection {
		start: CellPoint::new(2, 3),
		end: CellPoint::new(0, 9),
		block: true,
	};
	for base in [0, 1, 500] {
		let lines = selection.in_lines(base);
		assert_eq!(
			(lines.start.row, lines.end.row, lines.block),
			(2 + base, base, true)
		);
		assert_eq!((lines.start.col, lines.end.col), (3, 9), "columns stay");
		assert_eq!(
			lines.in_rows(base),
			Some(
				Selection {
					start: CellPoint::new(0, 9),
					end: CellPoint::new(2, 3),
					block: true
				}
				.ordered_selection()
			)
		);
	}
}

trait Ordered {
	fn ordered_selection(self) -> Selection;
}

impl Ordered for Selection {
	fn ordered_selection(self) -> Selection {
		let (start, end) = self.ordered();
		Selection {
			start,
			end,
			block: self.block,
		}
	}
}

/// The class a double click groups by, written out again.
fn kind(ch: char) -> u8 {
	if ch.is_alphanumeric() || "-_./~:@?&=%+#$".contains(ch) {
		0
	} else if ch.is_whitespace() || ch == '\0' {
		1
	} else {
		2
	}
}

#[test]
fn a_double_click_takes_the_longest_run_of_one_kind_around_the_cell() {
	let soup: Vec<char> = "ab9_-./ :@\t()[]{},;!\"'é漢\0".chars().collect();
	let mut rng = Rng::new(61);
	for _ in 0..3000 {
		let row: Vec<char> = (0..1 + rng.below(30)).map(|_| *rng.pick(&soup)).collect();
		let col = rng.below(row.len());
		let picked = selection(2, &row, CellPoint::new(4, col)).unwrap();
		let (first, last) = (picked.start.col, picked.end.col);
		assert!(first <= col && col <= last, "{row:?} at {col}");
		let k = kind(row[col]);
		if k == 2 {
			assert_eq!((first, last), (col, col), "other characters alone: {row:?}");
			continue;
		}
		assert!(
			row[first..=last].iter().all(|&c| kind(c) == k),
			"{row:?} at {col}"
		);
		assert!(
			first == 0 || kind(row[first - 1]) != k,
			"could start earlier: {row:?} at {col}"
		);
		assert!(
			last + 1 == row.len() || kind(row[last + 1]) != k,
			"could end later: {row:?} at {col}"
		);
	}
}

#[test]
fn press_counts_follow_a_plain_reference_for_any_sequence() {
	let mut rng = Rng::new(62);
	let start = Instant::now();
	for _ in 0..200 {
		let (mut clicks, mut at) = (Clicks::default(), start);
		let mut expected: Option<(Instant, CellPoint, u8)> = None;
		for _ in 0..30 {
			at += Duration::from_millis(rng.below(2) as u64 * rng.below(900) as u64);
			let point = CellPoint::new(rng.below(2), rng.below(2));
			let count = match expected {
				Some((when, there, n)) if there == point && at - when <= DOUBLE_CLICK && n < 3 => n + 1,
				_ => 1,
			};
			assert_eq!(clicks.press(at, point), count);
			expected = Some((at, point, count));
		}
	}
}
