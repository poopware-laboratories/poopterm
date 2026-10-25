//! Scrollback search.
use crate::{
	term::Term,
	ui::selection::{CellPoint, Selection},
};

pub(crate) fn find_matches(term: &Term, query: &str) -> Vec<Selection> {
	let query = query.to_ascii_lowercase();
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
				.map(|(col, cell)| (cell.ch.to_ascii_lowercase(), col))
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
		let found = find_matches(&term, "hello");
		assert_eq!(found.len(), 2);
		assert_eq!(found[0].start.col, 3);
		assert_eq!(found[0].end.col, 7);
		assert!(find_matches(&term, &"x".repeat(100)).is_empty());
		assert!(find_matches(&term, "").is_empty());
	}
}
