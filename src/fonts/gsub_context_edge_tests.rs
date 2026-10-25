//! Edge cases of contextual substitution: choosing among rules, the order
//! and reach of lookup records, where matching goes on after a rule, the
//! nesting bound, glyphs skipped by `GDEF` classes, and reverse chained
//! substitution looking backwards.
use super::{chain_rule, reverse, rule, rule_set, run, to_tens, with_sets};
use crate::fonts::{
	gsub::tests::{F, classes, coverage, gsub, lookup, multiple, shape, single, table, values},
	otl::tests::gdef,
};

/// A format 1 contextual subtable: one rule set for the covered `first`.
fn glyph_rules(first: u16, rules: Vec<Vec<u8>>) -> Vec<u8> {
	with_sets(
		1,
		vec![F::O(coverage(&[first]))],
		vec![Some(rule_set(rules))],
	)
}

/// [`run`] with lookup 0 flagged `flag`, against [`gdef`]: glyphs 1 to 3
/// are bases, 4 a ligature, 5 to 9 marks.
fn run_flagged(
	flag: u16,
	kind: u16,
	subtable: Vec<u8>,
	nested: Vec<Vec<u8>>,
	glyphs: &[u16],
) -> Vec<u16> {
	let mut lookups = vec![lookup(kind, flag, vec![subtable])];
	lookups.extend(nested);
	shape(&gsub(&[(b"ccmp", &[0])], lookups), Some(&gdef(0)), glyphs)
}

#[test]
fn the_first_rule_of_a_set_that_matches_applies() {
	// 1 2 3 calls lookup 1 on the 3, 1 2 on the 2; the longer is listed
	// first, so it wins where both match.
	let subtable = || glyph_rules(1, vec![rule(&[2, 3], &[(2, 1)]), rule(&[2], &[(1, 1)])]);
	assert_eq!(run(5, subtable(), vec![to_tens()], &[1, 2, 3]), [1, 2, 30]);
	assert_eq!(run(5, subtable(), vec![to_tens()], &[1, 2, 4]), [1, 20, 4]);
	let shorter_first = glyph_rules(1, vec![rule(&[2], &[(1, 1)]), rule(&[2, 3], &[(2, 1)])]);
	assert_eq!(
		run(5, shorter_first, vec![to_tens()], &[1, 2, 3]),
		[1, 20, 3]
	);
}

#[test]
fn a_rule_left_out_of_its_set_is_passed_over() {
	let set = table(vec![F::V(2), F::Null, F::O(rule(&[2], &[(1, 1)]))]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	assert_eq!(run(5, subtable, vec![to_tens()], &[1, 2]), [1, 20]);
}

#[test]
fn records_apply_in_their_order_not_by_position() {
	// Lookup 1 makes 1 a 2, lookup 2 makes 2 a 3: both at the first glyph.
	let to_two = lookup(1, 0, vec![single(&[(1, 2)])]);
	let to_three = lookup(1, 0, vec![single(&[(2, 3)])]);
	let nested = || vec![to_two.clone(), to_three.clone()];
	let in_order = glyph_rules(1, vec![rule(&[], &[(0, 1), (0, 2)])]);
	assert_eq!(run(5, in_order, nested(), &[1]), [3]);
	let reversed = glyph_rules(1, vec![rule(&[], &[(0, 2), (0, 1)])]);
	assert_eq!(run(5, reversed, nested(), &[1]), [2]);
}

#[test]
fn a_rule_with_no_records_matches_and_changes_nothing() {
	// The match still moves past the sequence, so the 1 inside it is not
	// matched again by the next rule.
	let subtable = glyph_rules(1, vec![rule(&[1], &[]), rule(&[], &[(0, 1)])]);
	assert_eq!(run(5, subtable, vec![to_tens()], &[1, 1]), [1, 1]);
}

#[test]
fn matching_goes_on_after_each_match() {
	let subtable = glyph_rules(1, vec![rule(&[2], &[(0, 1)])]);
	assert_eq!(
		run(5, subtable, vec![to_tens()], &[1, 2, 1, 2, 1]),
		[10, 2, 10, 2, 1]
	);
}

#[test]
fn glyphs_a_record_inserts_are_not_matched_again() {
	// 1 → 1 1 inside the context: without moving past the insert, this
	// would grow until the glyph bound.
	let expand = lookup(2, 0, vec![multiple(&[(1, &[1, 1])])]);
	let subtable = glyph_rules(1, vec![rule(&[], &[(0, 1)])]);
	assert_eq!(run(5, subtable.clone(), vec![expand.clone()], &[1]), [1, 1]);
	assert_eq!(run(5, subtable, vec![expand], &[1, 2, 1]), [1, 1, 2, 1, 1]);
}

#[test]
fn a_rule_deleting_its_whole_match_goes_on_at_the_next_glyph() {
	let delete = lookup(2, 0, vec![multiple(&[(1, &[]), (2, &[])])]);
	let subtable = glyph_rules(1, vec![rule(&[2], &[(1, 1), (0, 1)])]);
	assert_eq!(
		run(5, subtable.clone(), vec![delete.clone()], &[1, 2, 3]),
		[3]
	);
	assert_eq!(
		run(5, subtable, vec![delete], &[1, 2, 1, 2]),
		[] as [u16; 0]
	);
}

#[test]
fn a_record_pointing_past_a_deleted_end_is_skipped() {
	// Deleting the 2 leaves nothing at sequence index 1 for lookup 2.
	let delete = lookup(2, 0, vec![multiple(&[(2, &[])])]);
	let subtable = glyph_rules(1, vec![rule(&[2], &[(1, 1), (1, 2)])]);
	assert_eq!(run(5, subtable, vec![delete, to_tens()], &[1, 2]), [1]);
	let kept = glyph_rules(1, vec![rule(&[2], &[(1, 1), (1, 2)])]);
	let delete_three = lookup(2, 0, vec![multiple(&[(3, &[])])]);
	assert_eq!(
		run(5, kept, vec![delete_three, to_tens()], &[1, 2]),
		[1, 20]
	);
}

/// Shapes `glyphs` through `depth` contextual lookups, each calling the next
/// at the first glyph, the last calling a single substitution 1 → 2.
fn nested_chain(depth: u16) -> Vec<u16> {
	let mut lookups: Vec<Vec<u8>> = (1..=depth)
		.map(|next| lookup(5, 0, vec![glyph_rules(1, vec![rule(&[], &[(0, next)])])]))
		.collect();
	lookups.push(lookup(1, 0, vec![single(&[(1, 2)])]));
	shape(&gsub(&[(b"ccmp", &[0])], lookups), None, &[1])
}

#[test]
fn contextual_lookups_nest_eight_deep_and_no_deeper() {
	assert_eq!(nested_chain(1), [2]);
	assert_eq!(nested_chain(8), [2]);
	assert_eq!(nested_chain(9), [1]);
}

#[test]
fn class_rules_without_a_class_definition_put_every_glyph_in_class_zero() {
	let set = rule_set(vec![rule(&[0], &[(1, 1)])]);
	let subtable = with_sets(2, vec![F::O(coverage(&[1])), F::Null], vec![Some(set)]);
	assert_eq!(run(5, subtable.clone(), vec![to_tens()], &[1, 2]), [1, 20]);
	assert_eq!(
		run(5, subtable, vec![to_tens()], &[2, 2]),
		[2, 2],
		"2 is not covered"
	);
}

#[test]
fn class_rules_still_need_the_first_glyph_covered() {
	// Glyph 3 is in class 1 like 1, but only 1 is covered.
	let set = rule_set(vec![rule(&[], &[(0, 1)])]);
	let subtable = with_sets(
		2,
		vec![F::O(coverage(&[1])), F::O(classes(&[(1, 1, 1), (3, 3, 1)]))],
		vec![None, Some(set)],
	);
	assert_eq!(run(5, subtable, vec![to_tens()], &[1, 3]), [10, 3]);
}

/// A format 3 contextual subtable over `coverages`, with `records`.
fn coverage_rule(coverages: &[&[u16]], records: &[(u16, u16)]) -> Vec<u8> {
	let mut fields = vec![
		F::V(3),
		F::V(coverages.len() as u16),
		F::V(records.len() as u16),
	];
	fields.extend(coverages.iter().map(|glyphs| F::O(coverage(glyphs))));
	for &(index, lookup) in records {
		fields.extend(values(&[index, lookup]));
	}
	table(fields)
}

#[test]
fn a_coverage_rule_without_coverages_matches_nothing() {
	assert_eq!(
		run(5, coverage_rule(&[], &[(0, 1)]), vec![to_tens()], &[1]),
		[1]
	);
	let one = coverage_rule(&[&[1, 2]], &[(0, 1)]);
	assert_eq!(run(5, one, vec![to_tens()], &[2, 1, 3]), [20, 10, 3]);
}

#[test]
fn a_coverage_rule_needs_every_glyph_in_its_own_coverage() {
	let subtable = || coverage_rule(&[&[1], &[2, 3], &[1]], &[(1, 1)]);
	assert_eq!(run(5, subtable(), vec![to_tens()], &[1, 3, 1]), [1, 30, 1]);
	assert_eq!(run(5, subtable(), vec![to_tens()], &[1, 1, 1]), [1, 1, 1]);
	assert_eq!(
		run(5, subtable(), vec![to_tens()], &[1, 3]),
		[1, 3],
		"too short"
	);
}

#[test]
fn chained_rules_need_their_context_inside_the_cluster() {
	// 1 after a 2 and before a 3; at either end of the cluster there is
	// nothing to match.
	let set = rule_set(vec![chain_rule(&[2], &[], &[3], &[(0, 1)])]);
	let subtable = || with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set.clone())]);
	assert_eq!(run(6, subtable(), vec![to_tens()], &[2, 1, 3]), [2, 10, 3]);
	assert_eq!(run(6, subtable(), vec![to_tens()], &[1, 3]), [1, 3]);
	assert_eq!(run(6, subtable(), vec![to_tens()], &[2, 1]), [2, 1]);
}

#[test]
fn backtrack_glyphs_are_listed_nearest_first() {
	// Backtrack [2, 3] means a 3 then a 2 right before the 1.
	let set = rule_set(vec![chain_rule(&[2, 3], &[], &[], &[(0, 1)])]);
	let subtable = || with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set.clone())]);
	assert_eq!(run(6, subtable(), vec![to_tens()], &[3, 2, 1]), [3, 2, 10]);
	assert_eq!(run(6, subtable(), vec![to_tens()], &[2, 3, 1]), [2, 3, 1]);
}

#[test]
fn context_glyphs_are_matched_but_never_substituted() {
	// Records may only reach the input: index 1 past the single input
	// glyph is the lookahead 2, which stays.
	let set = rule_set(vec![chain_rule(&[3], &[], &[2], &[(1, 1), (0, 1)])]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	assert_eq!(run(6, subtable, vec![to_tens()], &[3, 1, 2]), [3, 10, 2]);
}

#[test]
fn chained_class_rules_without_definitions_use_class_zero_for_that_part() {
	// No backtrack classes: any glyph before is class 0. Input and
	// lookahead put 1 in class 1, 3 in class 2.
	let defined = classes(&[(1, 1, 1), (3, 3, 2)]);
	let set = rule_set(vec![chain_rule(&[0], &[], &[2], &[(0, 1)])]);
	let subtable = with_sets(
		2,
		vec![
			F::O(coverage(&[1])),
			F::Null,
			F::O(defined.clone()),
			F::O(defined),
		],
		vec![None, Some(set)],
	);
	assert_eq!(
		run(6, subtable.clone(), vec![to_tens()], &[3, 1, 3]),
		[3, 10, 3]
	);
	assert_eq!(run(6, subtable, vec![to_tens()], &[3, 1, 1]), [3, 1, 1]);
}

/// A format 3 chained subtable over coverage lists, with `records`.
fn chained_coverages(
	back: &[&[u16]],
	input: &[&[u16]],
	ahead: &[&[u16]],
	records: &[(u16, u16)],
) -> Vec<u8> {
	let mut fields = vec![F::V(3)];
	for part in [back, input, ahead] {
		fields.push(F::V(part.len() as u16));
		fields.extend(part.iter().map(|glyphs| F::O(coverage(glyphs))));
	}
	fields.push(F::V(records.len() as u16));
	for &(index, lookup) in records {
		fields.extend(values(&[index, lookup]));
	}
	table(fields)
}

#[test]
fn a_chained_coverage_rule_without_input_matches_nothing() {
	let empty = chained_coverages(&[], &[], &[&[1]], &[(0, 1)]);
	assert_eq!(run(6, empty, vec![to_tens()], &[1, 1]), [1, 1]);
}

#[test]
fn chained_coverage_rules_check_backtrack_input_and_lookahead_in_turn() {
	let subtable = || chained_coverages(&[&[5], &[4]], &[&[1], &[2]], &[&[3]], &[(1, 1)]);
	assert_eq!(
		run(6, subtable(), vec![to_tens()], &[4, 5, 1, 2, 3]),
		[4, 5, 1, 20, 3]
	);
	for unmatched in [
		[4, 4, 1, 2, 3],
		[4, 5, 1, 3, 3],
		[4, 5, 1, 2, 2],
		[5, 4, 1, 2, 3],
	] {
		assert_eq!(run(6, subtable(), vec![to_tens()], &unmatched), unmatched);
	}
}

#[test]
fn ignored_marks_are_stepped_over_in_input_and_context() {
	// Lookup 0 ignores marks (5 to 9): 1 2 matches through marks, after a
	// 3 and before a 3, and the marks stay where they were.
	let set = rule_set(vec![chain_rule(&[3], &[2], &[3], &[(1, 1)])]);
	let subtable = || with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set.clone())]);
	let glyphs = [3, 5, 1, 6, 2, 7, 3];
	assert_eq!(
		run_flagged(8, 6, subtable(), vec![to_tens()], &glyphs),
		[3, 5, 1, 6, 20, 7, 3]
	);
	assert_eq!(
		run_flagged(0, 6, subtable(), vec![to_tens()], &glyphs),
		glyphs,
		"marks count"
	);
}

#[test]
fn an_ignored_first_glyph_is_never_matched() {
	let subtable = glyph_rules(5, vec![rule(&[], &[(0, 1)])]);
	let marks = lookup(1, 0, vec![single(&[(5, 50)])]);
	assert_eq!(
		run_flagged(8, 5, subtable.clone(), vec![marks.clone()], &[5]),
		[5]
	);
	assert_eq!(run_flagged(0, 5, subtable, vec![marks], &[5]), [50]);
}

#[test]
fn nested_lookups_skip_glyphs_by_their_own_flags() {
	// The outer lookup matches 1 5 2 with marks counted; the nested
	// ligature 1 + 2 ignores marks, so it reaches past the 5.
	let ligature = crate::fonts::gsub::tests::ligatures(&[(1, &[(100, &[2])])]);
	let set = rule_set(vec![rule(&[5, 2], &[(0, 1)])]);
	let subtable = with_sets(1, vec![F::O(coverage(&[1]))], vec![Some(set)]);
	let nested = vec![lookup(4, 8, vec![ligature])];
	assert_eq!(run_flagged(0, 5, subtable, nested, &[1, 5, 2]), [100, 5]);
}

/// A reverse chained subtable: covered `glyphs` become `substitutes` after
/// one of `back`.
fn reverse_after(glyphs: &[u16], back: &[u16], substitutes: &[u16]) -> Vec<u8> {
	let mut fields = vec![
		F::V(1),
		F::O(coverage(glyphs)),
		F::V(1),
		F::O(coverage(back)),
		F::V(0),
		F::V(substitutes.len() as u16),
	];
	fields.extend(values(substitutes));
	table(fields)
}

#[test]
fn reverse_substitution_looks_back_at_glyphs_it_has_not_reached() {
	// From the end, each 1 looks at the original glyph before it.
	let subtable = reverse_after(&[1], &[2], &[10]);
	assert_eq!(run(8, subtable.clone(), vec![], &[2, 1, 1]), [2, 10, 1]);
	assert_eq!(run(8, subtable, vec![], &[1, 2, 1]), [1, 2, 10]);
	// Its own output is never its context: going forwards, the first 1
	// would become a 2 and so would the next.
	let feeding = reverse_after(&[1], &[2], &[2]);
	assert_eq!(run(8, feeding, vec![], &[2, 1, 1]), [2, 2, 1]);
}

#[test]
fn reverse_substitution_of_another_format_changes_nothing() {
	let mut subtable = reverse(&[1], &[2], &[10]);
	subtable[1] = 2;
	assert_eq!(run(8, subtable, vec![], &[1, 2]), [1, 2]);
}

#[test]
fn reverse_substitution_steps_over_ignored_marks() {
	let subtable = reverse(&[1], &[2], &[10]);
	assert_eq!(
		run_flagged(8, 8, subtable.clone(), vec![], &[1, 5, 2]),
		[10, 5, 2]
	);
	assert_eq!(run_flagged(0, 8, subtable, vec![], &[1, 5, 2]), [1, 5, 2]);
	let marks = reverse(&[5], &[2], &[50]);
	assert_eq!(
		run_flagged(8, 8, marks.clone(), vec![], &[5, 2]),
		[5, 2],
		"an ignored mark stays"
	);
	assert_eq!(run_flagged(0, 8, marks, vec![], &[5, 2]), [50, 2]);
}
