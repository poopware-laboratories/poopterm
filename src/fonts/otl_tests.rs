//! Tests for the OpenType Layout common tables: coverage and class lookups,
//! and lookups skipping glyphs by their `GDEF` classes.
use super::{class, coverage};
use crate::fonts::gsub::tests::{
	F, classes, coverage as coverage_list, coverage_ranges, gsub, ligatures, lookup, shape, single,
	table, values,
};

#[test]
fn a_glyph_list_gives_each_listed_glyph_its_index() {
	let list = coverage_list(&[3, 8, 20, 400]);
	for (index, glyph) in [3, 8, 20, 400].into_iter().enumerate() {
		assert_eq!(coverage(&list, 0, glyph), Some(index as u16));
	}
	for missing in [0, 4, 19, 21, 401, 65535] {
		assert_eq!(coverage(&list, 0, missing), None, "{missing}");
	}
}

#[test]
fn ranges_count_on_from_their_first_coverage_index() {
	let ranges = coverage_ranges(&[(10, 12, 0), (20, 29, 3), (40, 40, 13)]);
	let cases = [(10, 0), (12, 2), (20, 3), (25, 8), (29, 12), (40, 13)];
	for (glyph, index) in cases {
		assert_eq!(coverage(&ranges, 0, glyph), Some(index), "{glyph}");
	}
	for missing in [9, 13, 19, 30, 39, 41] {
		assert_eq!(coverage(&ranges, 0, missing), None, "{missing}");
	}
}

#[test]
fn a_coverage_index_past_sixteen_bits_is_none() {
	let ranges = coverage_ranges(&[(10, 20, 65530)]);
	assert_eq!(coverage(&ranges, 0, 15), Some(65535));
	assert_eq!(coverage(&ranges, 0, 16), None);
}

#[test]
fn unknown_formats_and_truncated_tables_cover_nothing() {
	let mut unknown = coverage_list(&[1, 2]);
	unknown[1] = 3;
	assert_eq!(coverage(&unknown, 0, 1), None);
	let full = coverage_ranges(&[(1, 5, 0), (7, 9, 5)]);
	for len in 0..full.len() {
		// Whatever is left must not panic, and finds only what it holds.
		let found = coverage(&full[..len], 0, 8);
		assert!(found.is_none() || len == full.len(), "{len}");
	}
	assert_eq!(coverage(&full, 0, 8), Some(6));
	assert_eq!(
		coverage(&full, full.len(), 8),
		None,
		"an offset past the end"
	);
}

#[test]
fn an_unsorted_list_only_misses_glyphs() {
	// The search may miss glyphs of a list out of order, but never gives a
	// glyph another one's index.
	let glyphs = [50, 10, 30, 20, 40];
	let unsorted = coverage_list(&glyphs);
	assert_eq!(coverage(&unsorted, 0, 30), Some(2), "the middle is found");
	assert_eq!(coverage(&unsorted, 0, 50), None, "missed");
	for (index, glyph) in glyphs.into_iter().enumerate() {
		let found = coverage(&unsorted, 0, glyph);
		assert!(
			found.is_none() || found == Some(index as u16),
			"{glyph}: {found:?}"
		);
	}
}

#[test]
fn class_arrays_give_classes_from_their_start_glyph() {
	let array = table(values(&[1, 10, 3, 4, 0, 6]));
	let cases = [
		(9, 0),
		(10, 4),
		(11, 0),
		(12, 6),
		(13, 0),
		(0, 0),
		(65535, 0),
	];
	for (glyph, expected) in cases {
		assert_eq!(class(&array, 0, glyph), expected, "{glyph}");
	}
}

#[test]
fn class_ranges_give_their_class_and_others_class_zero() {
	let ranges = classes(&[(5, 9, 1), (10, 10, 2), (100, 200, 7)]);
	let cases = [
		(4, 0),
		(5, 1),
		(9, 1),
		(10, 2),
		(11, 0),
		(99, 0),
		(150, 7),
		(201, 0),
	];
	for (glyph, expected) in cases {
		assert_eq!(class(&ranges, 0, glyph), expected, "{glyph}");
	}
}

#[test]
fn malformed_class_tables_put_everything_in_class_zero() {
	let mut unknown = classes(&[(1, 9, 3)]);
	unknown[1] = 9;
	assert_eq!(class(&unknown, 0, 5), 0);
	let full = classes(&[(1, 9, 3)]);
	for len in 0..full.len() {
		assert_eq!(class(&full[..len], 0, 5), 0, "{len}");
	}
	let array = table(values(&[1, 65535, 2, 5, 5]));
	assert_eq!(class(&array, 0, 65535), 5, "the last glyph");
	assert_eq!(class(&array, 0, 0), 0, "no wrap past it");
}

/// Glyph classes: 1 to 3 bases, 4 a ligature, 5 to 9 marks; marks 5 and 6
/// in attachment class 1, 7 and 8 in class 2; mark set 0 is {5, 7}, set 1
/// is {9}.
fn gdef(minor: u16) -> Vec<u8> {
	gdef_fields(minor, minor >= 2)
}

/// [`gdef`] with or without the mark glyph sets field, whatever the
/// version says.
fn gdef_fields(minor: u16, sets_field: bool) -> Vec<u8> {
	let glyph_classes = classes(&[(1, 3, 1), (4, 4, 2), (5, 9, 3)]);
	let attach = classes(&[(5, 6, 1), (7, 8, 2)]);
	let sets = table(vec![
		F::V(1),
		F::V(2),
		F::O32(coverage_list(&[5, 7])),
		F::O32(coverage_list(&[9])),
	]);
	let mut fields = vec![
		F::V(1),
		F::V(minor),
		F::O(glyph_classes),
		F::Null,
		F::Null,
		F::O(attach),
	];
	if sets_field {
		fields.push(F::O(sets));
	}
	table(fields)
}

/// A ligature 1 + 2 → 100 (both bases), in a lookup with `flag` and, when
/// given, mark filtering set `set`.
fn ligature_with(flag: u16, set: Option<u16>) -> Vec<u8> {
	let mut lookup = lookup(4, flag, vec![ligatures(&[(1, &[(100, &[2])])])]);
	if let Some(set) = set {
		// The set follows the subtable offsets: splice it in and move the
		// subtable two bytes on.
		lookup.splice(8..8, set.to_be_bytes());
		let offset = u16::from_be_bytes([lookup[6], lookup[7]]) + 2;
		lookup[6..8].copy_from_slice(&offset.to_be_bytes());
	}
	gsub(&[(b"ccmp", &[0])], vec![lookup])
}

#[test]
fn ignoring_marks_lets_a_ligature_reach_past_them_and_keeps_them_after() {
	let gsub = ligature_with(0x0008, None);
	assert_eq!(shape(&gsub, Some(&gdef(0)), &[1, 5, 6, 2]), [100, 5, 6]);
	assert_eq!(
		shape(&gsub, Some(&gdef(0)), &[1, 3, 2]),
		[1, 3, 2],
		"a base stops it"
	);
	assert_eq!(
		shape(&gsub, None, &[1, 5, 2]),
		[1, 5, 2],
		"without GDEF nothing is a mark"
	);
	let plain = ligature_with(0, None);
	assert_eq!(
		shape(&plain, Some(&gdef(0)), &[1, 5, 2]),
		[1, 5, 2],
		"marks count"
	);
}

#[test]
fn ignoring_ligatures_skips_only_ligatures() {
	let gsub = ligature_with(0x0004, None);
	assert_eq!(shape(&gsub, Some(&gdef(0)), &[1, 4, 2]), [100, 4]);
	assert_eq!(shape(&gsub, Some(&gdef(0)), &[1, 5, 2]), [1, 5, 2]);
}

#[test]
fn ignoring_base_glyphs_skips_bases_between_components() {
	// Components 1 and 2 are bases themselves, so a lookup ignoring bases
	// never starts on them.
	let gsub = ligature_with(0x0002, None);
	assert_eq!(shape(&gsub, Some(&gdef(0)), &[1, 2]), [1, 2]);
	let marks = gsub_marks(0x0002);
	assert_eq!(shape(&marks, Some(&gdef(0)), &[5, 3, 6]), [101, 3]);
}

/// A ligature of marks 5 + 6 → 101, with `flag`.
fn gsub_marks(flag: u16) -> Vec<u8> {
	gsub(
		&[(b"ccmp", &[0])],
		vec![lookup(4, flag, vec![ligatures(&[(5, &[(101, &[6])])])])],
	)
}

#[test]
fn a_mark_attachment_type_skips_marks_of_other_types() {
	// Class 1 marks (5, 6) are kept, class 2 (7, 8) skipped.
	let gsub = ligature_with(0x0100, None);
	assert_eq!(shape(&gsub, Some(&gdef(0)), &[1, 7, 8, 2]), [100, 7, 8]);
	assert_eq!(shape(&gsub, Some(&gdef(0)), &[1, 6, 2]), [1, 6, 2]);
	assert_eq!(
		shape(&gsub, Some(&gdef(0)), &[1, 9, 2]),
		[100, 9],
		"no class: skipped"
	);
}

#[test]
fn a_mark_filtering_set_skips_marks_outside_it() {
	let gsub = ligature_with(0x0010, Some(0));
	let sets = gdef(2);
	assert_eq!(shape(&gsub, Some(&sets), &[1, 6, 9, 2]), [100, 6, 9]);
	assert_eq!(
		shape(&gsub, Some(&sets), &[1, 5, 2]),
		[1, 5, 2],
		"5 is in the set"
	);
	let missing = ligature_with(0x0010, Some(4));
	assert_eq!(
		shape(&missing, Some(&sets), &[1, 5, 2]),
		[100, 5],
		"no such set"
	);
	let old = gdef_fields(0, true);
	assert_eq!(
		shape(&gsub, Some(&old), &[1, 5, 2]),
		[100, 5],
		"no sets before GDEF 1.2"
	);
}

#[test]
fn a_gdef_of_another_major_version_is_refused() {
	let mut newer = gdef(0);
	newer[1] = 2;
	assert!(
		super::Gdef::parse(
			&newer,
			super::Table {
				offset: 0,
				len: newer.len()
			}
		)
		.is_none()
	);
	let flagged = gsub(
		&[(b"ccmp", &[0])],
		vec![lookup(1, 0x0008, vec![single(&[(5, 50)])])],
	);
	assert_eq!(
		shape(&flagged, Some(&gdef(0)), &[5, 1]),
		[5, 1],
		"a skipped glyph is kept"
	);
}
