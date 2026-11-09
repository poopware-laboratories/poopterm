//! Tests of `morx` shaping on tables built by `morx_build_tests`.
use super::{face::Table, morx::Morx, morx_build_tests::*};

/// Runs `morx` built from `chains` over `glyphs`.
fn run(chains: &[Vec<u8>], glyphs: &[u16]) -> Vec<u16> {
	let data = morx(chains);
	let table = Table {
		offset: 0,
		len: data.len(),
	};
	let mut glyphs = glyphs.to_vec();
	if let Some(morx) = Morx::parse(&data, table) {
		morx.apply(&data, &mut glyphs);
	}
	glyphs
}

/// One chain with the default feature bit 1 on and these subtables.
fn run_all(subtables: &[Vec<u8>], glyphs: &[u16]) -> Vec<u16> {
	run(&[chain(1, 0, subtables)], glyphs)
}

fn substitute(pairs: &[(u16, u16)]) -> Vec<u8> {
	subtable(NONCONTEXTUAL | BOTH_DIRECTIONS, 1, &lookup6(pairs))
}

/// Classes 4, 5, 6 for glyphs A, B, C; entry 0 does nothing.
fn classes() -> Vec<u8> {
	lookup8(A, &[4, 5, 6])
}

/// A ligature subtable where A then B form `LIG_AB`: A is pushed in state
/// 0, B in state 2 pops both.
fn ligature_ab() -> Vec<u8> {
	let rows = vec![
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 3, 0, 1, 2, 0],
	];
	let entries = vec![
		vec![0, 0, 0],
		vec![2, 0x8000, 0],
		vec![0, 0xa000, 0],
		vec![2, 0, 0],
	];
	let components = [0u16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2]
		.iter()
		.flat_map(|&n| be16(n))
		.collect();
	let ligatures = [0u16, LIG_AB].iter().flat_map(|&n| be16(n)).collect();
	let extras = [
		actions(&[(false, false, 0), (true, false, 0)]),
		components,
		ligatures,
	];
	let body = state_table(7, &classes(), &rows, &entries, &extras);
	subtable(LIGATURE | BOTH_DIRECTIONS, 1, &body)
}

#[test]
fn a_noncontextual_subtable_replaces_each_glyph_it_lists() {
	let out = run_all(&[substitute(&[(A, 30), (C, 31)])], &[A, B, C, A]);
	assert_eq!(out, [30, B, 31, 30]);
}

#[test]
fn a_noncontextual_subtable_reads_every_lookup_format() {
	for lookup in [
		lookup0(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40]),
		lookup2(&[(10, 10, 40)]),
		lookup4(&[(10, 10, vec![40])]),
		lookup6(&[(10, 40)]),
		lookup8(10, &[40]),
		lookup10(2, 10, &[40]),
	] {
		let table = subtable(NONCONTEXTUAL | BOTH_DIRECTIONS, 1, &lookup);
		assert_eq!(run_all(&[table], &[B, A]), [B, 40]);
	}
}

#[test]
fn a_ligature_joins_its_components_into_one_glyph() {
	assert_eq!(run_all(&[ligature_ab()], &[A, B]), [LIG_AB]);
}

#[test]
fn a_ligature_inside_other_glyphs_leaves_them_alone() {
	assert_eq!(run_all(&[ligature_ab()], &[C, A, B, C]), [C, LIG_AB, C]);
	assert_eq!(run_all(&[ligature_ab()], &[A, B, A, B]), [LIG_AB, LIG_AB]);
}

#[test]
fn only_the_components_nearest_the_action_are_joined() {
	assert_eq!(run_all(&[ligature_ab()], &[A, A, B]), [A, LIG_AB]);
}

#[test]
fn glyphs_that_do_not_make_a_ligature_are_unchanged() {
	for glyphs in [&[A][..], &[B, A], &[A, C, B], &[C], &[], &[B, B]] {
		assert_eq!(run_all(&[ligature_ab()], glyphs), glyphs, "{glyphs:?}");
	}
}

/// A, B, C form `LIG_ABC` through three actions, the sum of the components
/// 0 + 3 + 7 reaching its entry in the ligature table.
fn ligature_abc() -> Vec<u8> {
	// Columns: end, out of bounds, deleted, spare, A, B, C.
	let rows = vec![
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 3, 0, 1, 2, 0],
		vec![0, 0, 5, 0, 1, 0, 4],
	];
	let entries = vec![
		vec![0, 0, 0],
		vec![2, 0x8000, 0],
		vec![3, 0x8000, 0],
		vec![2, 0, 0],
		vec![0, 0xa000, 0],
		vec![3, 0, 0],
	];
	let mut components = vec![0u16; 13];
	components[usize::from(B)] = 3;
	components[usize::from(C)] = 7;
	let mut ligatures = vec![0u16; 11];
	ligatures[10] = LIG_ABC;
	let words = |values: Vec<u16>| values.iter().flat_map(|&n| be16(n)).collect();
	let list = actions(&[(false, false, 0), (false, false, 0), (true, false, 0)]);
	let extras = [list, words(components), words(ligatures)];
	let body = state_table(7, &classes(), &rows, &entries, &extras);
	subtable(LIGATURE | BOTH_DIRECTIONS, 1, &body)
}

#[test]
fn a_ligature_of_three_components_sums_their_parts() {
	assert_eq!(run_all(&[ligature_abc()], &[A, B, C]), [LIG_ABC]);
	assert_eq!(
		run_all(&[ligature_abc()], &[C, A, B, C, A]),
		[C, LIG_ABC, A]
	);
}

#[test]
fn a_ligature_of_three_needs_all_three() {
	assert_eq!(run_all(&[ligature_abc()], &[A, B]), [A, B]);
	assert_eq!(run_all(&[ligature_abc()], &[A, C]), [A, C]);
}

#[test]
fn a_ligature_may_run_over_glyphs_an_earlier_one_deleted() {
	// A B A B: the first pair joins, deleted B stays in the array while the
	// second pair is read, and nothing deleted survives at the end.
	let out = run_all(&[ligature_ab()], &[A, B, A, B, C]);
	assert_eq!(out, [LIG_AB, LIG_AB, C]);
}

/// A contextual subtable: after A (marked), B replaces the marked glyph from
/// substitution 0 and itself from substitution 1.
fn contextual() -> Vec<u8> {
	let rows = vec![
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 3, 0, 1, 2, 0],
	];
	let none = 0xffff;
	let entries = vec![
		vec![0, 0, none, none],
		vec![2, 0x8000, none, none],
		vec![0, 0, 0, 1],
		vec![2, 0, none, none],
	];
	let subs = substitutions(&[lookup6(&[(A, 30)]), lookup6(&[(B, 31)])]);
	let body = state_table(7, &classes(), &rows, &entries, &[subs]);
	subtable(CONTEXTUAL | BOTH_DIRECTIONS, 1, &body)
}

#[test]
fn a_contextual_subtable_substitutes_the_marked_and_the_current_glyph() {
	assert_eq!(run_all(&[contextual()], &[A, B]), [30, 31]);
	assert_eq!(run_all(&[contextual()], &[C, A, B, C]), [C, 30, 31, C]);
}

#[test]
fn a_contextual_subtable_changes_nothing_outside_its_context() {
	for glyphs in [&[B][..], &[A], &[A, C, B], &[B, A], &[C, C]] {
		assert_eq!(run_all(&[contextual()], glyphs), glyphs, "{glyphs:?}");
	}
}

#[test]
fn subtables_run_in_order_each_seeing_the_one_before() {
	// C becomes A, then A B joins; run the other way round nothing joins.
	let to_a = substitute(&[(C, A)]);
	assert_eq!(run_all(&[to_a.clone(), ligature_ab()], &[C, B]), [LIG_AB]);
	assert_eq!(run_all(&[ligature_ab(), to_a], &[C, B]), [A, B]);
}

#[test]
fn a_subtable_runs_only_when_its_feature_flag_is_on() {
	let off = subtable(NONCONTEXTUAL | BOTH_DIRECTIONS, 2, &lookup6(&[(A, 30)]));
	assert_eq!(run(&[chain(1, 0, std::slice::from_ref(&off))], &[A]), [A]);
	assert_eq!(run(&[chain(3, 0, std::slice::from_ref(&off))], &[A]), [30]);
	assert_eq!(run(&[chain(2, 0, &[off])], &[A]), [30]);
}

#[test]
fn a_subtable_for_vertical_text_alone_is_skipped() {
	let vertical = subtable(NONCONTEXTUAL | VERTICAL, 1, &lookup6(&[(A, 30)]));
	assert_eq!(run_all(&[vertical], &[A]), [A]);
}

#[test]
fn rearrangement_and_insertion_subtables_are_left_out() {
	for kind in [REARRANGEMENT, INSERTION] {
		let table = subtable(kind | BOTH_DIRECTIONS, 1, &[0; 40]);
		assert_eq!(run_all(&[table, substitute(&[(A, 30)])], &[A, B]), [30, B]);
	}
}

#[test]
fn every_chain_runs_with_its_own_default_flags() {
	let only_two = subtable(NONCONTEXTUAL | BOTH_DIRECTIONS, 2, &lookup6(&[(A, 30)]));
	let both = [
		chain(1, 0, std::slice::from_ref(&only_two)),
		chain(2, 1, &[only_two]),
	];
	assert_eq!(run(&both, &[A, B]), [30, B]);
}

#[test]
fn a_chain_may_list_feature_records_before_its_subtables() {
	let table = substitute(&[(A, 30)]);
	assert_eq!(run(&[chain(1, 13, &[table])], &[A]), [30]);
}

#[test]
fn a_table_of_an_unknown_version_is_not_read() {
	let mut data = morx(&[chain(1, 0, &[substitute(&[(A, 30)])])]);
	for version in [0u16, 1, 4, 0xffff] {
		data[..2].copy_from_slice(&version.to_be_bytes());
		let table = Table {
			offset: 0,
			len: data.len(),
		};
		assert!(Morx::parse(&data, table).is_none(), "version {version}");
	}
	for version in [2u16, 3] {
		data[..2].copy_from_slice(&version.to_be_bytes());
		let table = Table {
			offset: 0,
			len: data.len(),
		};
		assert!(Morx::parse(&data, table).is_some(), "version {version}");
	}
}

#[test]
fn a_table_inside_font_data_is_read_at_its_offset() {
	let tables = morx(&[chain(1, 0, &[substitute(&[(A, 30)])])]);
	let data = [vec![0xee; 10], tables.clone(), vec![0xdd; 10]].concat();
	let table = Table {
		offset: 10,
		len: tables.len(),
	};
	let mut glyphs = vec![A];
	Morx::parse(&data, table).unwrap().apply(&data, &mut glyphs);
	assert_eq!(glyphs, [30]);
}

#[test]
fn a_state_machine_that_never_advances_ends() {
	// Entry 1 keeps the state and does not advance: no glyph ever passes.
	let rows = vec![vec![1; 7], vec![1; 7]];
	let entries = vec![vec![0, 0, 0xffff, 0xffff], vec![0, 0x4000, 0xffff, 0xffff]];
	let subs = substitutions(&[lookup6(&[(A, 30)])]);
	let body = state_table(7, &classes(), &rows, &entries, &[subs]);
	let table = subtable(CONTEXTUAL | BOTH_DIRECTIONS, 1, &body);
	assert_eq!(run_all(&[table], &[A, B, C]), [A, B, C]);
}

#[test]
fn a_ligature_that_pops_an_empty_stack_changes_nothing() {
	let rows = vec![vec![1; 7], vec![1; 7]];
	let entries = vec![vec![0, 0, 0], vec![0, 0x2000, 0]];
	let extras = [
		actions(&[(false, false, 0), (true, false, 0)]),
		vec![0; 40],
		vec![0; 40],
	];
	let body = state_table(7, &classes(), &rows, &entries, &extras);
	let table = subtable(LIGATURE | BOTH_DIRECTIONS, 1, &body);
	assert_eq!(run_all(&[table], &[A, B]), [A, B]);
}

#[test]
fn only_so_many_components_are_held_for_one_ligature() {
	// Every A is pushed; B pops one and writes 30 there. With 32 held, the
	// 32nd A (index 31) is the top whatever comes after it.
	let rows = vec![vec![0, 0, 0, 0, 1, 2, 0]; 2];
	let entries = vec![vec![0, 0, 0], vec![0, 0x8000, 0], vec![0, 0x2000, 0]];
	let mut components = vec![0u16; 13];
	components[usize::from(A)] = 1;
	let words = |values: Vec<u16>| -> Vec<u8> { values.iter().flat_map(|&n| be16(n)).collect() };
	let extras = [
		actions(&[(true, false, 0)]),
		words(components),
		words(vec![0, 30]),
	];
	let body = state_table(7, &classes(), &rows, &entries, &extras);
	let table = subtable(LIGATURE | BOTH_DIRECTIONS, 1, &body);
	let mut glyphs = vec![A; 40];
	glyphs.push(B);
	let out = run_all(&[table], &glyphs);
	let mut want = glyphs.clone();
	want[31] = 30;
	assert_eq!(out, want);
}

#[test]
fn a_glyph_the_class_table_lacks_is_out_of_bounds_not_the_end() {
	// Class 1 substitutes; class 0, the end of text, must not.
	let rows = vec![vec![0, 1, 0, 0, 0, 0, 0]; 2];
	let entries = vec![vec![0, 0, 0xffff, 0xffff], vec![0, 0, 0xffff, 0]];
	let subs = substitutions(&[lookup6(&[(A, 31), (99, 30)])]);
	let body = state_table(7, &classes(), &rows, &entries, &[subs]);
	let table = subtable(CONTEXTUAL | BOTH_DIRECTIONS, 1, &body);
	assert_eq!(run_all(std::slice::from_ref(&table), &[99]), [30]);
	assert_eq!(run_all(&[table], &[A]), [A], "A has a class of its own");
}
#[test]
fn a_store_action_leaves_the_ligature_for_the_next_action() {
	// Pop B (store): B's place holds the ligature of its component, pushed
	// back; pop it again (last): the sum of both components picks the glyph.
	let rows = vec![
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 3, 0, 1, 2, 0],
	];
	let entries = vec![
		vec![0, 0, 0],
		vec![2, 0x8000, 0],
		vec![0, 0xa000, 0],
		vec![2, 0, 0],
	];
	let mut components = vec![0u16; 32];
	components[usize::from(B)] = 1;
	components[usize::from(LIG_AB)] = 4;
	let mut ligatures = vec![0u16; 8];
	ligatures[1] = LIG_AB;
	ligatures[5] = 30;
	let words = |values: Vec<u16>| -> Vec<u8> { values.iter().flat_map(|&n| be16(n)).collect() };
	let list = actions(&[(false, true, 0), (true, false, 0)]);
	let extras = [list, words(components), words(ligatures)];
	let body = state_table(7, &classes(), &rows, &entries, &extras);
	let table = subtable(LIGATURE | BOTH_DIRECTIONS, 1, &body);
	assert_eq!(run_all(&[table], &[A, B]), [A, 30]);
}

#[test]
fn a_component_offset_may_be_negative() {
	// The offset -2 moves B's lookup to component 9, which holds 1.
	let rows = vec![
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 3, 0, 1, 2, 0],
	];
	let entries = vec![
		vec![0, 0, 0],
		vec![2, 0x8000, 0],
		vec![0, 0xa000, 0],
		vec![2, 0, 0],
	];
	let mut components = vec![0u16; 13];
	components[9] = 1;
	let mut ligatures = vec![0u16; 4];
	ligatures[1] = LIG_AB;
	let words = |values: Vec<u16>| -> Vec<u8> { values.iter().flat_map(|&n| be16(n)).collect() };
	let list = actions(&[(false, false, -2), (true, false, 0)]);
	let extras = [list, words(components), words(ligatures)];
	let body = state_table(7, &classes(), &rows, &entries, &extras);
	let table = subtable(LIGATURE | BOTH_DIRECTIONS, 1, &body);
	assert_eq!(run_all(&[table], &[A, B]), [LIG_AB]);
}

#[test]
fn an_entry_that_does_not_advance_acts_again_on_the_same_glyph() {
	// State 0 on A moves to state 2 without advancing; state 2 on A then
	// replaces it. Advancing would meet B in state 2, which does nothing.
	let rows = vec![
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 0, 0, 1, 0, 0],
		vec![0, 0, 0, 0, 2, 0, 0],
	];
	let entries = vec![
		vec![0, 0, 0xffff, 0xffff],
		vec![2, 0x4000, 0xffff, 0xffff],
		vec![0, 0, 0xffff, 0],
	];
	let subs = substitutions(&[lookup6(&[(A, 30)])]);
	let body = state_table(7, &classes(), &rows, &entries, &[subs]);
	let table = subtable(CONTEXTUAL | BOTH_DIRECTIONS, 1, &body);
	assert_eq!(run_all(&[table], &[A, B]), [30, B]);
}
