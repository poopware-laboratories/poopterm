//! Edge cases of `GDEF` skipping: which lookup flag wins when several
//! apply, glyphs without a class, and subtables a font leaves out, cuts
//! short or stores in a format of its own.
use super::{gdef, gdef_fields};
use crate::fonts::{
	face::Table,
	gsub::tests::{F, classes, table},
	otl::Gdef,
};

/// Whether `bytes`, read as a whole `GDEF`, makes a lookup with `flag` and
/// mark filtering set `set` skip `glyph`.
fn skips(bytes: &[u8], flag: u16, set: u16, glyph: u16) -> bool {
	let table = Table {
		offset: 0,
		len: bytes.len(),
	};
	Gdef::parse(bytes, table)
		.expect("a GDEF")
		.skips(bytes, flag, set, glyph)
}

/// Every flag that skips glyphs, alone and together.
const FLAGS: [u16; 6] = [0x0002, 0x0004, 0x0008, 0x0010, 0x0100, 0x021e];

#[test]
fn glyphs_without_a_class_are_never_skipped() {
	// `gdef` classes glyphs 1 to 9 only.
	for flag in FLAGS {
		for glyph in [0, 10, 65535] {
			assert!(!skips(&gdef(2), flag, 0, glyph), "{flag:#x} {glyph}");
		}
	}
}

#[test]
fn without_glyph_classes_nothing_is_skipped() {
	let no_classes = table(vec![F::V(1), F::V(0), F::Null, F::Null, F::Null, F::Null]);
	for flag in FLAGS {
		for glyph in 1..10 {
			assert!(!skips(&no_classes, flag, 0, glyph), "{flag:#x} {glyph}");
		}
	}
	let header_only = table(vec![F::V(1), F::V(0)]);
	assert!(!skips(&header_only, 0x0008, 0, 5));
}

#[test]
fn base_and_ligature_flags_leave_marks_alone() {
	for mark in 5..10 {
		assert!(!skips(&gdef(0), 0x0006, 0, mark), "{mark}");
	}
	assert!(skips(&gdef(0), 0x0006, 0, 1) && skips(&gdef(0), 0x0006, 0, 4));
}

#[test]
fn ignoring_marks_wins_over_sets_and_attachment_types() {
	// Mark 5 is in set 0 and attachment class 1, yet a lookup ignoring
	// marks skips it.
	assert!(!skips(&gdef(2), 0x0110, 0, 5));
	assert!(skips(&gdef(2), 0x0118, 0, 5));
}

#[test]
fn a_filtering_set_wins_over_the_attachment_type() {
	// Mark 7 is in set 0 though of attachment class 2; mark 6 is of class 1
	// but not in the set.
	assert!(!skips(&gdef(2), 0x0110, 0, 7));
	assert!(skips(&gdef(2), 0x0110, 0, 6));
	assert!(skips(&gdef(2), 0x0100, 0, 7), "by type alone, 7 is skipped");
}

#[test]
fn an_attachment_type_without_mark_classes_skips_every_mark() {
	let glyph_classes = classes(&[(1, 3, 1), (5, 9, 3)]);
	let no_marks = table(vec![
		F::V(1),
		F::V(0),
		F::O(glyph_classes),
		F::Null,
		F::Null,
		F::Null,
	]);
	for mark in 5..10 {
		assert!(skips(&no_marks, 0x0100, 0, mark), "{mark}");
	}
	assert!(!skips(&no_marks, 0x0100, 0, 1), "bases stay");
}

#[test]
fn mark_sets_in_another_format_hold_no_marks() {
	let mut sets = gdef(2);
	// The mark glyph sets table is the last one laid out; find its format.
	let at = usize::from(u16::from_be_bytes([sets[12], sets[13]]));
	sets[at + 1] = 2;
	for mark in [5, 7, 9] {
		assert!(skips(&sets, 0x0010, 0, mark), "{mark}");
		assert!(skips(&sets, 0x0010, 1, mark), "{mark}");
	}
}

#[test]
fn later_minor_versions_keep_their_mark_sets() {
	// Versions after 1.2 add fields after the mark sets offset.
	let newer = gdef_fields(3, true);
	assert!(!skips(&newer, 0x0010, 0, 5));
	assert!(skips(&newer, 0x0010, 0, 6));
	assert!(!skips(&newer, 0x0010, 1, 9));
}
