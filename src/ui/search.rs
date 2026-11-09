//! Scrollback search.
use crate::{
	term::Term,
	ui::selection::{CellPoint, Selection},
};

/// Where `query` occurs in the scrollback and screen; letters match in any
/// case unless `case_sensitive`.
pub(crate) fn find_matches(term: &Term, query: &str, case_sensitive: bool) -> Vec<Selection> {
	let fold = |ch: char| {
		if case_sensitive {
			ch
		} else {
			ch.to_ascii_lowercase()
		}
	};
	let query: String = query.chars().map(fold).collect();
	let mut matches = Vec::new();
	if !query.is_empty() {
		let needle: Vec<char> = query.chars().collect();
		let total_rows = term.scrollback.len() + term.rows;
		for row in 0..total_rows {
			let Some(cells) = term.row_cells(row) else {
				continue;
			};
			let chars: Vec<(char, usize)> = cells
				.iter()
				.enumerate()
				.filter(|(_, cell)| cell.width != 0)
				.map(|(col, cell)| (fold(cell.ch), col))
				.collect();
			if needle.len() > chars.len() {
				continue;
			}
			for start in 0..=chars.len() - needle.len() {
				if chars[start..start + needle.len()]
					.iter()
					.map(|(ch, _)| *ch)
					.eq(needle.iter().copied())
				{
					if matches.len() >= 10_000 {
						return matches;
					}
					matches.push(Selection {
						start: CellPoint {
							row,
							col: chars[start].1,
						},
						end: CellPoint {
							row,
							col: chars[start + needle.len() - 1].1,
						},
						block: false,
					});
				}
			}
		}
	}
	matches
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		parser::Parser,
		theme::{Theme, ThemeKind},
	};

	#[test]
	fn search_handles_wide_cells_and_long_queries() {
		let mut term = Term::new(20, 2, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
		Parser::new().advance(&mut term, "猫 hello HELLO".as_bytes());
		let found = find_matches(&term, "hello", false);
		assert_eq!(found.len(), 2);
		assert_eq!(found[0].start.col, 3);
		assert_eq!(found[0].end.col, 7);
		assert!(find_matches(&term, &"x".repeat(100), false).is_empty());
		assert!(find_matches(&term, "", false).is_empty());
	}

	fn term_with(text: &str) -> Term {
		let mut term = Term::new(40, 2, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
		Parser::new().advance(&mut term, text.as_bytes());
		term
	}

	#[test]
	fn matching_ignores_case_unless_asked_not_to() {
		let term = term_with("Hello hello HELLO");
		assert_eq!(find_matches(&term, "hello", false).len(), 3);
		assert_eq!(find_matches(&term, "HELLO", false).len(), 3);
		let exact = |query| find_matches(&term, query, true);
		assert_eq!(exact("hello").len(), 1);
		assert_eq!(exact("hello")[0].start.col, 6);
		assert_eq!(exact("Hello").len(), 1);
		assert_eq!(exact("HELLO")[0].start.col, 12);
		assert!(exact("hELLO").is_empty());
	}

	#[test]
	fn an_exact_query_still_finds_its_own_case_across_a_wide_cell() {
		let term = term_with("猫 Abc abc");
		assert_eq!(find_matches(&term, "Abc", true)[0].start.col, 3);
		assert_eq!(find_matches(&term, "abc", true)[0].start.col, 7);
		assert_eq!(find_matches(&term, "abc", false).len(), 2);
	}

	#[test]
	fn an_empty_exact_query_finds_nothing() {
		assert!(find_matches(&term_with("abc"), "", true).is_empty());
	}

	#[test]
	fn overlapping_occurrences_are_each_found() {
		let term = term_with("aaaa");
		let cols: Vec<_> = find_matches(&term, "aa", false)
			.iter()
			.map(|m| (m.start.col, m.end.col))
			.collect();
		assert_eq!(cols, [(0, 1), (1, 2), (2, 3)]);
	}

	#[test]
	fn a_match_spans_exactly_the_queried_characters() {
		let term = term_with("say Hello!");
		for case_sensitive in [false, true] {
			let found = find_matches(&term, "Hello", case_sensitive);
			assert_eq!(found.len(), 1, "{case_sensitive}");
			assert_eq!((found[0].start.col, found[0].end.col), (4, 8));
			assert_eq!(found[0].start.row, found[0].end.row);
			assert!(!found[0].block);
		}
	}

	#[test]
	fn matches_come_back_top_to_bottom_then_left_to_right() {
		let term = term_with("ab\r\nab ab");
		let points: Vec<_> = find_matches(&term, "ab", false)
			.iter()
			.map(|m| (m.start.row, m.start.col))
			.collect();
		let mut sorted = points.clone();
		sorted.sort_unstable();
		assert_eq!(points.len(), 3);
		assert_eq!(points, sorted);
	}

	#[test]
	fn a_query_longer_than_the_line_finds_nothing_either_way() {
		let term = term_with("abc");
		for case_sensitive in [false, true] {
			assert!(find_matches(&term, "abcd", case_sensitive).is_empty());
		}
	}

	#[test]
	fn non_ascii_letters_match_as_themselves_in_either_mode() {
		// Folding is for ASCII letters only: É is not é.
		let term = term_with("Éé");
		assert_eq!(find_matches(&term, "É", false).len(), 1);
		assert_eq!(find_matches(&term, "é", false).len(), 1);
		assert_eq!(find_matches(&term, "É", true).len(), 1);
	}

	#[test]
	fn the_cap_holds_with_case_sensitive_matching_too() {
		let mut term = Term::new(200, 100, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
		let line = "x".repeat(200);
		Parser::new().advance(&mut term, format!("{line}\r\n").repeat(99).as_bytes());
		assert_eq!(find_matches(&term, "x", false).len(), 10_000);
		assert_eq!(find_matches(&term, "x", true).len(), 10_000);
	}

	#[test]
	fn scrollback_rows_are_searched_too() {
		let mut term = Term::new(10, 2, Theme::from_kind(ThemeKind::Poop), "/tmp".into());
		Parser::new().advance(&mut term, b"needle\r\n1\r\n2\r\n3\r\n");
		let found = find_matches(&term, "needle", true);
		assert_eq!(found.len(), 1);
		assert!(found[0].start.row < term.scrollback.len());
	}
}
