//! Tests of joining marks to the cell just written.
use std::sync::Arc;

use super::join_marks;
use crate::{
	graphics::kitty::PLACEHOLDER,
	term::{Cell, Extras, NO_EXTRA},
};

/// A linked, styled cell, so tests can check that only the extra changes.
fn linked() -> (Arc<Extras>, Cell) {
	let mut extras = Arc::new(Extras::default());
	let link = Arc::make_mut(&mut extras)
		.with_hyperlink(NO_EXTRA, 7)
		.unwrap();
	let cell = Cell {
		ch: 'e',
		fg: 0x112233,
		bg: 0x445566,
		attrs: 0x5,
		width: 1,
		extra: link,
	};
	(extras, cell)
}

/// The cell with its extra replaced by `extra`.
fn with_extra(cell: Cell, extra: u16) -> Cell {
	Cell { extra, ..cell }
}

#[test]
fn leading_marks_join_and_the_next_character_is_taken() {
	let (mut extras, mut cell) = linked();
	let before = cell;
	let rest = "\u{301}\u{302}xyz";
	assert_eq!(
		join_marks(&mut extras, &mut cell, rest.into(), 3),
		(5, Some(('x', 1)))
	);
	let extra = cell.extra;
	assert_eq!(extras.marks(extra), ['\u{301}', '\u{302}']);
	assert_eq!(extras.hyperlink(extra), 7);
	assert_eq!(cell, with_extra(before, extra));
}

#[test]
fn a_character_without_room_is_left_for_the_caller() {
	let (mut extras, mut cell) = linked();
	assert_eq!(
		join_marks(&mut extras, &mut cell, "\u{301}猫".into(), 1),
		(2, None)
	);
	assert_eq!(
		join_marks(&mut extras, &mut cell, "猫".into(), 2),
		(3, Some(('猫', 2)))
	);
	assert_eq!(extras.marks(cell.extra), ['\u{301}']);
}

#[test]
fn placeholders_are_never_taken() {
	let (mut extras, mut cell) = linked();
	let rest = format!("\u{301}{PLACEHOLDER}");
	assert_eq!(
		join_marks(&mut extras, &mut cell, rest.as_str().into(), 5),
		(2, None)
	);
}

#[test]
fn characters_after_a_joiner_join_without_room() {
	let (mut extras, mut cell) = linked();
	let rest = "\u{200d}👩\u{200d}👧z";
	assert_eq!(
		join_marks(&mut extras, &mut cell, rest.into(), 0),
		(14, None)
	);
	let extra = cell.extra;
	assert_eq!(extras.marks(extra), ['\u{200d}', '👩', '\u{200d}', '👧']);
	// A trailing joiner is consumed; the next write continues the cluster.
	let (mut extras, mut cell) = linked();
	assert_eq!(
		join_marks(&mut extras, &mut cell, "\u{200d}".into(), 0),
		(3, None)
	);
	assert_eq!(extras.marks(cell.extra), ['\u{200d}']);
}

#[test]
fn nothing_to_join_leaves_the_cell_unchanged() {
	let (mut extras, mut cell) = linked();
	let before = cell;
	assert_eq!(join_marks(&mut extras, &mut cell, "".into(), 4), (0, None));
	assert_eq!(
		join_marks(&mut extras, &mut cell, "ab".into(), 4),
		(1, Some(('a', 1)))
	);
	assert_eq!(cell, before);
}

#[test]
fn repeated_marks_reuse_the_cluster_without_copying_the_table() {
	let (mut extras, mut first) = linked();
	let mut second = first;
	join_marks(&mut extras, &mut first, "\u{301}".into(), 1);
	// A shared table is only copied on a miss.
	let shared = extras.clone();
	join_marks(&mut extras, &mut second, "\u{301}".into(), 1);
	assert!(Arc::ptr_eq(&shared, &extras));
	assert_eq!(first, second);
}
