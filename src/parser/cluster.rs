//! Grapheme clusters built while printing a run: marks and joined characters
//! appended to the cell just written.
use std::sync::Arc;

use crate::{
	graphics::kitty::PLACEHOLDER,
	term::{Cell, Extras},
	vt::Text,
};

/// Joins the marks at the start of `rest`, and after a joiner the characters
/// they join, to the cluster of `cell`, as printing them one at a time would. Then takes the next character if it
/// fits in `room` columns and is not a placeholder. Returns the bytes of
/// `rest` used and the character taken, with its width.
///
/// `cell` was just written by the run, so it has no marks of its own yet.
#[cold]
#[inline(never)]
pub(super) fn join_marks(
	extras: &mut Arc<Extras>,
	cell: &mut Cell,
	rest: Text<'_>,
	room: usize,
) -> (usize, Option<(char, usize)>) {
	let mut extra = cell.extra;
	let (mut chars, mut joining) = (rest.chars(), false);
	let mut used = 0;
	let taken = loop {
		let Some(c) = chars.next() else {
			break None;
		};
		// `room` is less than the terminal's width, so widths need no clamping.
		let width = crate::width::width(c);
		if width != 0 && !joining {
			break (width <= room && c != PLACEHOLDER).then_some((c, width));
		}
		let id = match extras.remembered_mark(extra, c) {
			None => Arc::make_mut(extras).with_mark(extra, c),
			hit => hit,
		};
		extra = id.unwrap_or(extra);
		joining = extras.marks(extra).last() == Some(&'\u{200d}');
		used = rest.as_bytes().len() - chars.as_bytes().len();
	};
	cell.extra = extra;
	match taken {
		Some((c, _)) => (used + c.len_utf8(), taken),
		None => (used, None),
	}
}

#[cfg(test)]
#[path = "cluster_tests.rs"]
mod tests;
