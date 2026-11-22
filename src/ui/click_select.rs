//! Selecting with clicks: a second click on the same cell soon after the
//! first selects its word, a third its whole row, a fourth starts over.
use std::time::Instant;

use super::{
	selection::{CellPoint, Selection},
	window_pointer::DOUBLE_CLICK,
};

/// Characters that join letters and digits into one word, so a double
/// click takes a whole path, URL or option: `src/app/mod.rs:42`,
/// `--color=auto`, `user@host`.
const WORD_JOINERS: &str = "-_./~:@?&=%+#$";

/// The presses so far: when and where the last was, and how many came in
/// a row there.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Clicks {
	last: Option<(Instant, CellPoint)>,
	count: u8,
}

impl Clicks {
	/// Counts a press on `point` at `at`: 1 for a single click, 2 for a
	/// double, 3 for a triple, then 1 again.
	pub(crate) fn press(&mut self, at: Instant, point: CellPoint) -> u8 {
		let repeated = self.last.is_some_and(|(when, where_)| {
			where_ == point
				&& at
					.checked_duration_since(when)
					.is_some_and(|gap| gap <= DOUBLE_CLICK)
		});
		self.count = if repeated && self.count < 3 {
			self.count + 1
		} else {
			1
		};
		self.last = Some((at, point));
		self.count
	}
}

/// What kind of character a cell holds, for finding a word's edges.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
	Word,
	Space,
	Other,
}

fn class(ch: char) -> Class {
	match ch {
		_ if ch.is_alphanumeric() || WORD_JOINERS.contains(ch) => Class::Word,
		_ if ch.is_whitespace() || ch == '\0' => Class::Space,
		_ => Class::Other,
	}
}

/// What a press counted `count` times selects in the row of `point`,
/// whose cells hold `chars` (a wide character in both its cells): its
/// word, a run of spaces or one other character for 2, the row up to its
/// last character for 3 (nothing for a blank row), nothing for a single
/// click.
pub(crate) fn selection(count: u8, chars: &[char], point: CellPoint) -> Option<Selection> {
	let col = point.col.min(chars.len().checked_sub(1)?);
	let (start, end) = match count {
		2 => {
			let kind = class(chars[col]);
			let same = |i: &usize| class(chars[*i]) == kind && (kind != Class::Other || *i == col);
			let start = (0..=col).rev().take_while(same).last().unwrap_or(col);
			let end = (col..chars.len()).take_while(same).last().unwrap_or(col);
			(start, end)
		}
		// The row up to its last character: its padding is no text.
		3 => (0, chars.iter().rposition(|&ch| class(ch) != Class::Space)?),
		_ => return None,
	};
	let at = |col| CellPoint::new(point.row, col);
	Some(Selection {
		start: at(start),
		end: at(end),
		block: false,
	})
}

/// The characters of a row of `cells`, one a cell: the second cell of a
/// wide character holds it again, so the two select together.
pub(crate) fn chars(cells: &[crate::term::Cell]) -> Vec<char> {
	let mut out: Vec<char> = Vec::with_capacity(cells.len());
	for cell in cells {
		let ch = match (cell.width, out.last()) {
			(0, Some(&wide)) => wide,
			_ => cell.ch,
		};
		out.push(ch);
	}
	out
}
