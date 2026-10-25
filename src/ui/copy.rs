//! The text a selection copies.
use crate::{term::Term, ui::selection::Selection};

/// The text under `selection`, given in lines (see [`Selection::in_rows`]):
/// one line per row, without the spacers of wide characters, and with
/// trailing blanks dropped from every line that is not the selection's last.
/// A block drops them on its last line too, since its right edge is a column
/// rather than a point in the text. `None` once all of it was evicted.
pub(crate) fn selection_text(term: &Term, selection: Selection) -> Option<String> {
	let selection = selection.in_rows(term.history_base)?;
	let (start, end) = selection.ordered();
	let mut text = String::new();
	for row in start.row..=end.row {
		let Some(cells) = term.row_cells(row) else {
			break;
		};
		if let Some((from, to)) = selection.span(row, cells.len()) {
			for cell in cells[from..=to].iter().filter(|cell| cell.width != 0) {
				text.push(cell.ch);
				text.extend(term.marks(cell));
			}
		}
		if row != end.row || selection.block {
			text.truncate(text.trim_end_matches(' ').len());
		}
		if row != end.row {
			text.push('\n');
		}
	}
	Some(text)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		parser::Parser,
		theme::{Theme, ThemeKind},
		ui::selection::CellPoint,
	};

	fn term(input: &str) -> Term {
		let mut term = Term::new(12, 4, Theme::from_kind(ThemeKind::Poop), "/".into());
		Parser::new().advance(&mut term, input.as_bytes());
		term
	}

	fn select(start: (usize, usize), end: (usize, usize), block: bool) -> Selection {
		let point = |(row, col)| CellPoint { row, col };
		Selection {
			start: point(start),
			end: point(end),
			block,
		}
	}

	#[test]
	fn a_linear_selection_copies_the_text_between_its_ends() {
		let term = term("first line\r\nsecond\r\nthird row");
		let text = selection_text(&term, select((0, 6), (2, 4), false));
		assert_eq!(text.as_deref(), Some("line\nsecond\nthird"));
	}

	#[test]
	fn a_linear_selection_keeps_blanks_it_ends_on() {
		let term = term("ab  \r\ncd");
		assert_eq!(
			selection_text(&term, select((0, 0), (0, 3), false)).as_deref(),
			Some("ab  ")
		);
	}

	#[test]
	fn a_block_copies_the_same_columns_of_every_row() {
		let term = term("abcdefgh\r\nijklmnop\r\nqrstuvwx");
		let text = selection_text(&term, select((0, 2), (2, 4), true));
		assert_eq!(text.as_deref(), Some("cde\nklm\nstu"));
		// Dragged from the bottom-right corner: the same text.
		let back = selection_text(&term, select((2, 4), (0, 2), true));
		assert_eq!(back, text);
	}

	#[test]
	fn a_block_drops_trailing_blanks_on_every_line_and_keeps_short_lines() {
		let term = term("abcdef\r\nab\r\nabcd");
		let text = selection_text(&term, select((0, 1), (2, 5), true));
		assert_eq!(text.as_deref(), Some("bcdef\nb\nbcd"));
	}

	#[test]
	fn a_block_right_of_the_text_copies_empty_lines() {
		let term = term("ab\r\ncd");
		let text = selection_text(&term, select((0, 6), (1, 9), true));
		assert_eq!(text.as_deref(), Some("\n"));
	}

	#[test]
	fn wide_characters_are_copied_once_and_marks_with_their_base() {
		let term = term("猫x e\u{301}y");
		let text = selection_text(&term, select((0, 0), (0, 5), false));
		assert_eq!(text.as_deref(), Some("猫x e\u{301}y"));
		// A block starting on the spacer half leaves the wide character out.
		let text = selection_text(&term, select((0, 1), (0, 2), true));
		assert_eq!(text.as_deref(), Some("x"));
	}

	#[test]
	fn selections_reach_into_history() {
		let mut term = term("old one\r\nold two\r\n\r\n\r\n\r\nnow");
		assert_eq!(term.scrollback.len(), 2);
		let text = selection_text(&term, select((1, 0), (5, 2), true));
		assert_eq!(text.as_deref(), Some("old\n\n\n\nnow"));
		term.scrollback_limit = 2;
		let lines = term.history_base + 1;
		let text = selection_text(&term, select((lines, 0), (lines, 6), false));
		assert_eq!(text.as_deref(), Some("old two"));
	}

	#[test]
	fn an_evicted_selection_copies_nothing() {
		let mut term = term("a\r\nb\r\nc\r\nd\r\ne");
		term.set_scrollback_limit(0);
		assert_eq!(term.history_base, 1);
		assert!(selection_text(&term, select((0, 0), (0, 1), false)).is_none());
		assert_eq!(
			selection_text(&term, select((1, 0), (1, 0), false)).as_deref(),
			Some("b")
		);
	}
}
