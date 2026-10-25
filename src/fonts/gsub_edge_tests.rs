//! Edge cases of the substitution driver and the simple lookups: table
//! versions, empty and missing parts, several subtables, malformed
//! offsets and counts, and the bound on work per cluster.
use super::{
	CCMP, F, coverage, coverage_ranges, gsub, gsub_with, ligatures, lookup, multiple, shape, single,
	table, values,
};
use crate::fonts::{face::Table, gsub::Gsub};

fn parses(bytes: &[u8]) -> bool {
	Gsub::parse(
		bytes,
		Table {
			offset: 0,
			len: bytes.len(),
		},
	)
	.is_some()
}

#[test]
fn version_one_point_one_tables_shape_like_one_point_zero() {
	// 1.1 adds a feature variations offset after the lookup list's.
	let mut newer = gsub(&[(CCMP, &[0])], vec![lookup(1, 0, vec![single(&[(1, 2)])])]);
	newer[3] = 1;
	assert_eq!(shape(&newer, None, &[1]), [2]);
}

#[test]
fn a_table_without_scripts_or_language_systems_shapes_nothing() {
	let no_scripts = table(vec![
		F::V(1),
		F::V(0),
		F::O(table(values(&[0]))),
		F::O(table(values(&[0]))),
		F::O(table(values(&[0]))),
	]);
	assert!(!parses(&no_scripts));
	// A script with neither a default nor any language system.
	let empty_script = table(vec![
		F::V(1),
		F::V(0),
		F::O(table(vec![
			F::V(1),
			F::V(0x4446),
			F::V(0x4c54),
			F::O(table(values(&[0, 0]))),
		])),
		F::O(table(values(&[0]))),
		F::O(table(values(&[0]))),
	]);
	assert!(!parses(&empty_script));
}

#[test]
fn shaping_nothing_gives_nothing() {
	let gsub = gsub(
		&[(CCMP, &[0])],
		vec![lookup(2, 0, vec![multiple(&[(1, &[2, 3])])])],
	);
	assert_eq!(shape(&gsub, None, &[]), [] as [u16; 0]);
}

#[test]
fn a_feature_without_lookups_or_a_lookup_without_subtables_changes_nothing() {
	let gsub = gsub(&[(CCMP, &[]), (b"liga", &[0])], vec![lookup(1, 0, vec![])]);
	assert_eq!(shape(&gsub, None, &[1, 2]), [1, 2]);
}

#[test]
fn the_first_subtable_that_covers_a_glyph_applies_and_no_later_one_sees_its_result() {
	let gsub = gsub(
		&[(CCMP, &[0])],
		vec![lookup(
			1,
			0,
			vec![single(&[(1, 10)]), single(&[(2, 20), (10, 11)])],
		)],
	);
	assert_eq!(shape(&gsub, None, &[1, 2, 3]), [10, 20, 3]);
}

#[test]
fn ligature_first_glyphs_may_be_covered_by_ranges() {
	// Coverage format 2: glyphs 4 to 6 are the sets 0 to 2.
	let set = |glyph: u16| table(vec![F::V(1), F::O(table(values(&[glyph, 2, 9])))]);
	let subtable = table(vec![
		F::V(1),
		F::O(coverage_ranges(&[(4, 6, 0)])),
		F::V(3),
		F::O(set(40)),
		F::O(set(50)),
		F::O(set(60)),
	]);
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(4, 0, vec![subtable])]);
	assert_eq!(
		shape(&gsub, None, &[4, 9, 5, 9, 6, 9, 7, 9]),
		[40, 50, 60, 7, 9]
	);
}

#[test]
fn missing_sets_and_sequences_change_nothing() {
	// A covered glyph whose ligature set, sequence or alternate set offset
	// is 0, or past the count of them.
	for kind in [2, 3, 4] {
		let null = table(vec![F::V(1), F::O(coverage(&[1, 2])), F::V(1), F::Null]);
		let gsub = gsub(&[(CCMP, &[0])], vec![lookup(kind, 0, vec![null])]);
		assert_eq!(shape(&gsub, None, &[1, 2]), [1, 2], "type {kind}");
	}
}

#[test]
fn subtable_formats_a_lookup_type_does_not_define_change_nothing() {
	let single_three = table(vec![F::V(3), F::O(coverage(&[1])), F::V(1), F::V(5)]);
	let multiple_two = table(vec![F::V(2), F::O(coverage(&[1])), F::V(0)]);
	let gsub = gsub(
		&[(CCMP, &[0, 1])],
		vec![
			lookup(1, 0, vec![single_three]),
			lookup(2, 0, vec![multiple_two]),
		],
	);
	assert_eq!(shape(&gsub, None, &[1]), [1]);
}

#[test]
fn a_sequence_running_past_the_font_is_refused() {
	// The sequence says three glyphs but holds one, and the font ends there.
	let sequence = table(values(&[3, 7]));
	let subtable = table(vec![F::V(1), F::O(coverage(&[1])), F::V(1), F::O(sequence)]);
	let gsub = gsub(&[(CCMP, &[0])], vec![lookup(2, 0, vec![subtable])]);
	assert_eq!(shape(&gsub, None, &[1]), [1]);
}

#[test]
fn deleting_glyphs_at_the_end_and_every_glyph_works() {
	let gsub = gsub(
		&[(CCMP, &[0])],
		vec![lookup(2, 0, vec![multiple(&[(9, &[])])])],
	);
	assert_eq!(shape(&gsub, None, &[1, 9]), [1]);
	assert_eq!(shape(&gsub, None, &[9, 1, 9]), [1]);
	assert_eq!(shape(&gsub, None, &[9; 10]), [] as [u16; 0]);
}

#[test]
fn an_expansion_is_not_substituted_again_by_the_same_lookup() {
	// 1 → 1 1 would loop forever if the lookup went on inside its output.
	let gsub = gsub(
		&[(CCMP, &[0])],
		vec![lookup(2, 0, vec![multiple(&[(1, &[1, 1])])])],
	);
	assert_eq!(shape(&gsub, None, &[1]), [1, 1]);
	assert_eq!(shape(&gsub, None, &[1, 2, 1]), [1, 1, 2, 1, 1]);
}

#[test]
fn a_later_lookup_sees_an_earlier_ones_expansion() {
	let gsub = gsub(
		&[(CCMP, &[0, 1])],
		vec![
			lookup(2, 0, vec![multiple(&[(1, &[2, 3])])]),
			lookup(4, 0, vec![ligatures(&[(3, &[(30, &[4])])])]),
		],
	);
	assert_eq!(shape(&gsub, None, &[1, 4]), [2, 30]);
}

#[test]
fn a_broken_first_extension_skips_its_lookup_and_a_later_one_only_itself() {
	let good = table(vec![F::V(1), F::V(1), F::O32(single(&[(1, 2)]))]);
	let wrong_format = table(vec![F::V(2), F::V(1), F::O32(single(&[(1, 9)]))]);
	let broken_first = gsub(
		&[(CCMP, &[0])],
		vec![lookup(7, 0, vec![wrong_format.clone(), good.clone()])],
	);
	assert_eq!(
		shape(&broken_first, None, &[1]),
		[1],
		"the lookup's type is unknown"
	);
	let broken_later = gsub(
		&[(CCMP, &[0])],
		vec![lookup(7, 0, vec![good, wrong_format])],
	);
	assert_eq!(shape(&broken_later, None, &[1]), [2]);
}

#[test]
fn shaping_stops_when_its_work_runs_out() {
	// A lookup tries a subtable per unit of work: past the budget, the one
	// that would match after thousands of misses is never reached.
	let run = |misses: u16| {
		// One miss shared by every offset but the last, which matches.
		let (miss, hit) = (single(&[(7, 8)]), single(&[(1, 2)]));
		let start = 6 + 2 * (misses + 1);
		let mut busy = [1, 0, misses + 1].to_vec();
		busy.extend(std::iter::repeat_n(start, misses.into()));
		busy.push(start + miss.len() as u16);
		let mut busy: Vec<u8> = busy.iter().flat_map(|value| value.to_be_bytes()).collect();
		busy.extend(miss.iter().chain(&hit));
		let gsub = gsub(&[(CCMP, &[0])], vec![busy]);
		shape(&gsub, None, &[1])
	};
	assert_eq!(run(100), [2]);
	assert_eq!(run(5000), [1]);
}

#[test]
fn every_lookup_type_together_shapes_in_order() {
	// Single, multiple, alternate, ligature, in lookup list order.
	let alternates = table(vec![
		F::V(1),
		F::O(coverage(&[3])),
		F::V(1),
		F::O(table(values(&[2, 31, 32]))),
	]);
	let gsub = gsub(
		&[(CCMP, &[0, 1]), (b"rlig", &[2]), (b"liga", &[3])],
		vec![
			lookup(1, 0, vec![single(&[(1, 2)])]),
			lookup(2, 0, vec![multiple(&[(2, &[3, 4])])]),
			lookup(3, 0, vec![alternates]),
			lookup(4, 0, vec![ligatures(&[(31, &[(99, &[4])])])]),
		],
	);
	assert_eq!(shape(&gsub, None, &[1, 5]), [99, 5]);
}

#[test]
fn scripts_listing_features_out_of_order_still_run_lookups_in_list_order() {
	let scripts = vec![(
		b"DFLT",
		table(vec![F::V(0), F::V(0xffff), F::V(2), F::V(1), F::V(0)]),
	)];
	let gsub = gsub_with(
		scripts,
		&[(CCMP, &[0]), (b"liga", &[1])],
		vec![
			lookup(1, 0, vec![single(&[(1, 2)])]),
			lookup(1, 0, vec![single(&[(2, 3)])]),
		],
	);
	assert_eq!(shape(&gsub, None, &[1]), [3]);
}

#[test]
fn a_language_system_record_past_a_scripts_count_is_ignored() {
	// No default and no language systems, though a record follows; with a
	// count of 1 the same record shapes.
	let with_count = |count: u16| {
		let enabling = table(vec![F::V(0), F::V(0xffff), F::V(1), F::V(0)]);
		let script = table(vec![
			F::V(0),
			F::V(count),
			F::V(0x454e),
			F::V(0x4720),
			F::O(enabling),
		]);
		let features = table(values(&[0, 1, 0]));
		table(vec![
			F::V(1),
			F::V(0),
			F::O(table(vec![
				F::V(1),
				F::V(0x4446),
				F::V(0x4c54),
				F::O(script),
			])),
			F::O(table(vec![
				F::V(1),
				F::V(0x6363),
				F::V(0x6d70),
				F::O(features),
			])),
			F::O(table(vec![
				F::V(1),
				F::O(lookup(1, 0, vec![single(&[(1, 2)])])),
			])),
		])
	};
	assert!(!parses(&with_count(0)));
	assert_eq!(shape(&with_count(1), None, &[1]), [2]);
}
