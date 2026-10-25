//! Emoji that span two cells: a flag of two regional indicators, and an
//! emoji followed by a skin tone. Programs print each part with its own
//! width, so the grid keeps them in separate cells; fonts draw them as one
//! glyph, which the first cell draws across both.
use crate::term::Cell;

fn is_indicator(c: char) -> bool {
	('\u{1f1e6}'..='\u{1f1ff}').contains(&c)
}

fn is_tone(c: char) -> bool {
	('\u{1f3fb}'..='\u{1f3ff}').contains(&c)
}

/// Whether any cell of `cells` could take part in a join: indicators and
/// skin tones lie in one range, checked in a single pass.
pub(crate) fn may_join(cells: &[Cell]) -> bool {
	cells
		.iter()
		.any(|cell| ('\u{1f1e6}'..='\u{1f3ff}').contains(&cell.ch))
}

/// The column of the cell after `cell` at `col`, skipping wide spacers.
fn next(cells: &[Cell], cell: &Cell, col: usize) -> Option<usize> {
	let next = col + usize::from(cell.width);
	(next < cells.len()).then_some(next)
}

/// The column of the cell before the one at `col`, skipping wide spacers.
fn previous(cells: &[Cell], col: usize) -> Option<usize> {
	let previous = col.checked_sub(1)?;
	if cells[previous].width == 0 {
		previous.checked_sub(1)
	} else {
		Some(previous)
	}
}

/// Whether the regional indicator at `col` starts a pair: indicators pair
/// up from the start of their run.
fn starts_pair(cells: &[Cell], mut col: usize) -> bool {
	let mut before = 0;
	while let Some(previous) = previous(cells, col)
		&& is_indicator(cells[previous].ch)
	{
		before += 1;
		col = previous;
	}
	before % 2 == 0
}

/// The column of the cell that the cell at `col` may join with into one
/// glyph: the second indicator of a pair, or a skin tone after an emoji.
/// Whether they join is up to the font.
pub(crate) fn partner(cells: &[Cell], col: usize) -> Option<usize> {
	let first = cells.get(col)?;
	if first.width == 0 {
		return None;
	}
	let at = next(cells, first, col)?;
	let second = cells[at].ch;
	let pair = is_indicator(first.ch) && is_indicator(second) && starts_pair(cells, col);
	let toned = is_tone(second) && !is_tone(first.ch) && !is_indicator(first.ch);
	(pair || toned).then_some(at)
}

/// The column of the cell whose partner is the cell at `col`, if any.
pub(crate) fn joined_to(cells: &[Cell], col: usize) -> Option<usize> {
	let ch = cells.get(col)?.ch;
	// Most cells are neither, and need no look back.
	if !is_indicator(ch) && !is_tone(ch) {
		return None;
	}
	let first = previous(cells, col)?;
	(partner(cells, first) == Some(col)).then_some(first)
}

/// How a cell takes part in a glyph drawn across two cells.
#[derive(Debug, PartialEq)]
pub(crate) enum Join {
	Alone,
	/// The first cell, which draws the glyph of `chars` over `columns`.
	First {
		chars: Vec<char>,
		columns: usize,
	},
	/// The second cell, whose glyph the first one draws.
	Second,
}

/// How the cell at `col` joins a partner. `parts` appends a cell's
/// characters (its own and its marks), or returns false for a cell that
/// cannot join; `joins` says whether the font joins the characters.
pub(crate) fn join(
	cells: &[Cell],
	col: usize,
	mut parts: impl FnMut(&Cell, &mut Vec<char>) -> bool,
	joins: impl FnOnce(&[char]) -> bool,
) -> Join {
	let (first, second) = match (partner(cells, col), joined_to(cells, col)) {
		(Some(second), _) => (col, second),
		(None, Some(first)) => (first, col),
		(None, None) => return Join::Alone,
	};
	let mut chars = Vec::new();
	if !parts(&cells[first], &mut chars) || !parts(&cells[second], &mut chars) || !joins(&chars) {
		return Join::Alone;
	}
	if first == col {
		let columns = second - first + usize::from(cells[second].width);
		Join::First { chars, columns }
	} else {
		Join::Second
	}
}

#[cfg(test)]
#[path = "cell_joins_tests.rs"]
pub(crate) mod tests;
