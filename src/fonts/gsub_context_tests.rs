//! Tests for contextual substitution: each format of contextual and chained
//! rules, how nested lookups change the matched glyphs, nesting bounds, and
//! reverse chained substitution.
use crate::fonts::gsub::tests::{
	F, classes, coverage, gsub, lookup, multiple, shape, single, table, values,
};

/// A rule set holding `rules`.
fn rule_set(rules: Vec<Vec<u8>>) -> Vec<u8> {
	let mut fields = vec![F::V(rules.len() as u16)];
	fields.extend(rules.into_iter().map(F::O));
	table(fields)
}

/// A contextual rule: the input after the first glyph, then the
/// `(sequence index, lookup)` records.
fn rule(input: &[u16], records: &[(u16, u16)]) -> Vec<u8> {
	let mut fields = values(&[input.len() as u16 + 1, records.len() as u16]);
	fields.extend(values(input));
	for &(index, lookup) in records {
		fields.extend(values(&[index, lookup]));
	}
	table(fields)
}

/// A chained rule: backtrack (nearest first), the input after the first
/// glyph, lookahead, and records.
fn chain_rule(back: &[u16], input: &[u16], ahead: &[u16], records: &[(u16, u16)]) -> Vec<u8> {
	let mut fields = vec![F::V(back.len() as u16)];
	fields.extend(values(back));
	fields.push(F::V(input.len() as u16 + 1));
	fields.extend(values(input));
	fields.push(F::V(ahead.len() as u16));
	fields.extend(values(ahead));
	fields.push(F::V(records.len() as u16));
	for &(index, lookup) in records {
		fields.extend(values(&[index, lookup]));
	}
	table(fields)
}

/// Format 1 or 2 subtable (`kind` 5 or 6): the header `fields` after the
/// format, then the rule sets, `None` for sets left out.
fn with_sets(format: u16, mut fields: Vec<F>, sets: Vec<Option<Vec<u8>>>) -> Vec<u8> {
	fields.insert(0, F::V(format));
	fields.push(F::V(sets.len() as u16));
	fields.extend(sets.into_iter().map(|set| set.map_or(F::Null, F::O)));
	table(fields)
}

/// Shapes `glyphs` with lookup 0, a lookup of `kind` with one `subtable`,
/// and the `nested` lookups from 1 that its rules call (not featured).
fn run(kind: u16, subtable: Vec<u8>, nested: Vec<Vec<u8>>, glyphs: &[u16]) -> Vec<u16> {
	let mut lookups = vec![lookup(kind, 0, vec![subtable])];
	lookups.extend(nested);
	shape(&gsub(&[(b"ccmp", &[0])], lookups), None, glyphs)
}

/// Lookup 1 of the tests: 2 → 20, 3 → 30.
fn to_tens() -> Vec<u8> {
	lookup(1, 0, vec![single(&[(1, 10), (2, 20), (3, 30)])])
}

#[test]
fn glyph_rules_apply_their_lookups_where_the_sequence_matches() {
	let set = rule_set(vec![rule(&[2, 3], &[(1, 1)]), rule(&[2], &[(0, 1)])]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	assert_eq!(
		run(5, subtable.clone(), vec![to_tens()], &[1, 2, 3]),
		[1, 20, 3]
	);
	assert_eq!(
		run(5, subtable.clone(), vec![to_tens()], &[1, 2, 4]),
		[10, 2, 4]
	);
	assert_eq!(
		run(5, subtable, vec![to_tens()], &[1, 3]),
		[1, 3],
		"no rule matches"
	);
}

#[test]
fn a_match_moves_past_the_whole_sequence() {
	// 1 2 and 2 2 both match; in 1 2 2 the second 2 starts no new match
	// inside the first, and nothing follows it.
	let starting_one = rule_set(vec![rule(&[2], &[(0, 1)])]);
	let starting_two = rule_set(vec![rule(&[2], &[(0, 1)])]);
	let subtable = with_sets(
		1,
		vec![F::O(coverage(&[1, 2]))],
		vec![Some(starting_one), Some(starting_two)],
	);
	assert_eq!(
		run(5, subtable.clone(), vec![to_tens()], &[1, 2, 2]),
		[10, 2, 2]
	);
	assert_eq!(
		run(5, subtable, vec![to_tens()], &[2, 2, 1, 2]),
		[20, 2, 10, 2]
	);
}

#[test]
fn class_rules_match_by_the_classes_of_the_glyphs() {
	// Classes: 1 and 4 are 1, 2 and 5 are 2; rules for class 1 starts.
	let classes = classes(&[(1, 1, 1), (2, 2, 2), (4, 4, 1), (5, 5, 2)]);
	let set = rule_set(vec![rule(&[2], &[(1, 1)])]);
	let subtable = with_sets(
		2,
		vec![F::O(coverage(&[1, 4])), F::O(classes)],
		vec![None, Some(set)],
	);
	let cases: [(&[u16], &[u16]); 4] = [
		(&[4, 2], &[4, 20]),
		(&[1, 5], &[1, 5]), // 5 is class 2, but lookup 1 has no 5
		(&[1, 2], &[1, 20]),
		(&[1, 1], &[1, 1]),
	];
	for (glyphs, expected) in cases {
		assert_eq!(
			run(5, subtable.clone(), vec![to_tens()], glyphs),
			expected,
			"{glyphs:?}"
		);
	}
}

#[test]
fn each_class_has_its_own_rule_set() {
	// Class 1 (glyph 1) rules call lookup 1 on the next glyph, class 2
	// (glyph 2) rules on the glyph itself.
	let subtable = with_sets(
		2,
		vec![
			F::O(coverage(&[1, 2])),
			F::O(classes(&[(1, 1, 1), (2, 2, 2)])),
		],
		vec![
			None,
			Some(rule_set(vec![rule(&[0], &[(1, 1)])])),
			Some(rule_set(vec![rule(&[0], &[(0, 1)])])),
		],
	);
	assert_eq!(run(5, subtable.clone(), vec![to_tens()], &[1, 3]), [1, 30]);
	assert_eq!(run(5, subtable, vec![to_tens()], &[2, 3]), [20, 3]);
}

#[test]
fn a_class_without_a_rule_set_matches_nothing() {
	let classes = classes(&[(1, 1, 3)]);
	let set = rule_set(vec![rule(&[], &[(0, 1)])]);
	let subtable = with_sets(
		2,
		vec![F::O(coverage(&[1])), F::O(classes)],
		vec![Some(set)],
	);
	assert_eq!(
		run(5, subtable, vec![to_tens()], &[1]),
		[1],
		"class 3 is past the sets"
	);
}

#[test]
fn coverage_rules_match_a_coverage_per_glyph() {
	let subtable = table(vec![
		F::V(3),
		F::V(2),
		F::V(1),
		F::O(coverage(&[1, 2])),
		F::O(coverage(&[3])),
		F::V(1),
		F::V(1),
	]);
	assert_eq!(run(5, subtable.clone(), vec![to_tens()], &[2, 3]), [2, 30]);
	assert_eq!(
		run(5, subtable.clone(), vec![to_tens()], &[1, 3, 3]),
		[1, 30, 3]
	);
	assert_eq!(run(5, subtable, vec![to_tens()], &[3, 3]), [3, 3]);
}

#[test]
fn chained_glyph_rules_check_the_glyphs_before_and_after() {
	// Backtrack 6 then 5 (nearest first) before 1, lookahead 8 after it.
	let set = rule_set(vec![chain_rule(&[6, 5], &[], &[8], &[(0, 1)])]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	let cases: [(&[u16], &[u16]); 5] = [
		(&[5, 6, 1, 8], &[5, 6, 10, 8]),
		(&[6, 5, 1, 8], &[6, 5, 1, 8]),
		(&[5, 6, 1], &[5, 6, 1]),
		(&[6, 1, 8], &[6, 1, 8]),
		(&[5, 6, 1, 9], &[5, 6, 1, 9]),
	];
	for (glyphs, expected) in cases {
		assert_eq!(
			run(6, subtable.clone(), vec![to_tens()], glyphs),
			expected,
			"{glyphs:?}"
		);
	}
}

#[test]
fn chained_class_rules_use_a_class_definition_per_part() {
	// Backtrack classes: 7 is 1. Input: 1 is 1, 2 is 2. Lookahead: none
	// given, so everything is class 0.
	let subtable = with_sets(
		2,
		vec![
			F::O(coverage(&[1])),
			F::O(classes(&[(7, 7, 1)])),
			F::O(classes(&[(1, 1, 1), (2, 2, 2)])),
			F::Null,
		],
		vec![
			None,
			Some(rule_set(vec![chain_rule(&[1], &[2], &[0], &[(1, 1)])])),
		],
	);
	assert_eq!(
		run(6, subtable.clone(), vec![to_tens()], &[7, 1, 2, 9]),
		[7, 1, 20, 9]
	);
	assert_eq!(
		run(6, subtable.clone(), vec![to_tens()], &[8, 1, 2, 9]),
		[8, 1, 2, 9]
	);
	assert_eq!(
		run(6, subtable, vec![to_tens()], &[7, 1, 2]),
		[7, 1, 2],
		"lookahead missing"
	);
}

#[test]
fn each_input_class_of_a_chained_rule_has_its_own_set() {
	let subtable = with_sets(
		2,
		vec![
			F::O(coverage(&[1, 2])),
			F::Null,
			F::O(classes(&[(1, 1, 1), (2, 2, 2)])),
			F::Null,
		],
		vec![
			None,
			Some(rule_set(vec![chain_rule(&[], &[], &[], &[(0, 1)])])),
			Some(rule_set(vec![chain_rule(&[0], &[], &[], &[(0, 1)])])),
		],
	);
	assert_eq!(run(6, subtable.clone(), vec![to_tens()], &[1, 2]), [10, 20]);
	assert_eq!(
		run(6, subtable, vec![to_tens()], &[2, 1]),
		[2, 10],
		"2 needs a glyph before"
	);
}

#[test]
fn a_chained_coverage_rule_starts_only_on_its_first_coverage() {
	// Input {2}, lookahead {3}: a 1 before a 3 is no start.
	let subtable = table(vec![
		F::V(3),
		F::V(0),
		F::V(1),
		F::O(coverage(&[2])),
		F::V(1),
		F::O(coverage(&[3])),
		F::V(1),
		F::V(0),
		F::V(1),
	]);
	assert_eq!(run(6, subtable.clone(), vec![to_tens()], &[1, 3]), [1, 3]);
	assert_eq!(run(6, subtable, vec![to_tens()], &[2, 3]), [20, 3]);
}

#[test]
fn chained_coverage_rules_match_like_noto_tag_sequences() {
	// Noto deletes tag characters (here 50 to 52) after a black flag (40) or
	// another tag: backtrack {40, 50..=52}, input {50..=52}.
	let subtable = table(vec![
		F::V(3),
		F::V(1),
		F::O(coverage(&[40, 50, 51, 52])),
		F::V(1),
		F::O(coverage(&[50, 51, 52])),
		F::V(0),
		F::V(1),
		F::V(0),
		F::V(1),
	]);
	let delete = lookup(2, 0, vec![multiple(&[(50, &[]), (51, &[]), (52, &[])])]);
	assert_eq!(
		run(6, subtable.clone(), vec![delete.clone()], &[40, 50, 51, 52]),
		[40]
	);
	assert_eq!(
		run(6, subtable, vec![delete], &[41, 50, 51]),
		[41, 50],
		"a tag after a tag goes"
	);
}

#[test]
fn later_records_follow_glyphs_an_earlier_one_inserted_or_deleted() {
	let expand = lookup(2, 0, vec![multiple(&[(1, &[3, 4])])]);
	let delete = lookup(2, 0, vec![multiple(&[(1, &[])])]);
	let records = [(0, 1), (1, 2)];
	let set = || rule_set(vec![rule(&[2], &records)]);
	let subtable = || with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set())]);
	assert_eq!(
		run(5, subtable(), vec![expand, to_tens()], &[1, 2, 5]),
		[3, 4, 20, 5]
	);
	assert_eq!(
		run(5, subtable(), vec![delete, to_tens()], &[1, 2, 5]),
		[20, 5]
	);
}

#[test]
fn records_past_the_match_or_to_missing_lookups_are_skipped() {
	let set = rule_set(vec![rule(&[2], &[(5, 1), (0, 9), (1, 1)])]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	assert_eq!(run(5, subtable, vec![to_tens()], &[1, 2]), [1, 20]);
}

#[test]
fn rules_that_call_themselves_stop_at_the_nesting_bound() {
	// Lookup 0 matches 1 and calls itself there, forever if allowed.
	let set = rule_set(vec![rule(&[], &[(0, 0)])]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	assert_eq!(run(5, subtable, vec![], &[1, 1, 2]), [1, 1, 2]);
}

#[test]
fn a_nested_contextual_lookup_applies_its_own_rules() {
	// Lookup 0 calls lookup 1 at the 2; lookup 1 matches 2 3 and calls 2.
	let outer = with_sets(
		1,
		vec![F::O(coverage(&[1]))],
		vec![Some(rule_set(vec![rule(&[2, 3], &[(1, 1)])]))],
	);
	let inner = with_sets(
		1,
		vec![F::O(coverage(&[2]))],
		vec![Some(rule_set(vec![rule(&[3], &[(1, 2)])]))],
	);
	let nested = vec![lookup(5, 0, vec![inner]), to_tens()];
	assert_eq!(run(5, outer, nested, &[1, 2, 3]), [1, 2, 30]);
}

/// A reverse chained subtable: covered `glyphs` become `substitutes` when
/// followed by one of `ahead`.
fn reverse(glyphs: &[u16], ahead: &[u16], substitutes: &[u16]) -> Vec<u8> {
	let mut fields = vec![
		F::V(1),
		F::O(coverage(glyphs)),
		F::V(0),
		F::V(1),
		F::O(coverage(ahead)),
		F::V(substitutes.len() as u16),
	];
	fields.extend(values(substitutes));
	table(fields)
}

#[test]
fn reverse_substitution_runs_from_the_end_and_sees_its_own_output() {
	// 1 becomes 10 before a 2 or a 10: from the end, every 1 in a run does.
	let subtable = reverse(&[1], &[2, 10], &[10]);
	assert_eq!(run(8, subtable.clone(), vec![], &[1, 1, 2]), [10, 10, 2]);
	assert_eq!(run(8, subtable, vec![], &[1, 1, 3]), [1, 1, 3]);
}

#[test]
fn reverse_substitution_takes_each_glyphs_own_substitute() {
	let subtable = reverse(&[1, 2], &[3], &[10, 20]);
	assert_eq!(
		run(8, subtable.clone(), vec![], &[2, 3, 1, 3]),
		[20, 3, 10, 3]
	);
	let short = reverse(&[1, 2], &[3], &[10]);
	assert_eq!(
		run(8, short, vec![], &[2, 3]),
		[2, 3],
		"no substitute listed for 2"
	);
}

#[test]
fn an_extension_holding_reverse_substitution_still_runs_from_the_end() {
	let extension = table(vec![
		F::V(1),
		F::V(8),
		F::O32(reverse(&[1], &[2, 10], &[10])),
	]);
	let lookups = vec![lookup(7, 0, vec![extension])];
	let gsub = crate::fonts::gsub::tests::gsub(&[(b"ccmp", &[0])], lookups);
	assert_eq!(shape(&gsub, None, &[1, 1, 2]), [10, 10, 2]);
}

#[test]
fn reverse_substitution_is_refused_inside_a_context() {
	let set = rule_set(vec![rule(&[], &[(0, 1)])]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	let nested = vec![lookup(8, 0, vec![reverse(&[1], &[2], &[10])])];
	assert_eq!(run(5, subtable, nested, &[1, 2]), [1, 2]);
}

#[test]
fn an_unpaired_regional_indicator_pair_becomes_one_placeholder_like_noto() {
	// Class 1 is the indicators 1 to 3; a pair calls single (to 99) at the
	// first and delete at the second.
	let set = rule_set(vec![rule(&[1], &[(0, 1), (1, 2)])]);
	let subtable = with_sets(
		2,
		vec![F::O(coverage(&[1, 2, 3])), F::O(classes(&[(1, 3, 1)]))],
		vec![None, Some(set)],
	);
	let nested = vec![
		lookup(1, 0, vec![single(&[(1, 99), (2, 99), (3, 99)])]),
		lookup(2, 0, vec![multiple(&[(1, &[]), (2, &[]), (3, &[])])]),
	];
	assert_eq!(run(5, subtable.clone(), nested.clone(), &[1, 3]), [99]);
	assert_eq!(
		run(5, subtable.clone(), nested.clone(), &[2, 1, 3]),
		[99, 3]
	);
	assert_eq!(run(5, subtable, nested, &[2, 7]), [2, 7]);
}

#[test]
fn malformed_context_tables_change_nothing_and_never_panic() {
	let set = rule_set(vec![rule(&[2], &[(1, 1)])]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	assert_eq!(run(5, subtable.clone(), vec![to_tens()], &[1, 2]), [1, 20]);
	for len in 0..subtable.len() {
		let cut = subtable[..len].to_vec();
		run(5, cut.clone(), vec![to_tens()], &[1, 2]);
		run(6, cut, vec![to_tens()], &[1, 2]);
	}
	let mut unknown = subtable;
	unknown[1] = 4;
	assert_eq!(run(5, unknown, vec![to_tens()], &[1, 2]), [1, 2]);
	let empty = table(values(&[3, 0, 0]));
	assert_eq!(
		run(5, empty, vec![to_tens()], &[1]),
		[1],
		"no glyphs in a rule"
	);
}

#[path = "gsub_context_edge_tests.rs"]
mod edge;
