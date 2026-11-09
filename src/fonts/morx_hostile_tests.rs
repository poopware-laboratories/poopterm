//! `morx` comes from font files, which are untrusted: whatever the bytes, it
//! must neither panic nor run for long.
use super::{face::Table, morx::Morx, morx_build_tests::*};

fn contextual_and_ligature_font() -> Vec<u8> {
	let classes = lookup8(A, &[4, 5, 6]);
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
	let words = |values: &[u16]| -> Vec<u8> { values.iter().flat_map(|&n| be16(n)).collect() };
	let extras = [
		actions(&[(false, false, 0), (true, false, 0)]),
		words(&[0; 13]),
		words(&[0, LIG_AB]),
	];
	let ligature = subtable(
		LIGATURE | BOTH_DIRECTIONS,
		1,
		&state_table(7, &classes, &rows, &entries, &extras),
	);
	let context_entries = vec![
		vec![0, 0, 0xffff, 0xffff],
		vec![2, 0x8000, 0, 0],
		vec![0, 0, 0, 0],
	];
	let subs = substitutions(&[lookup6(&[(A, 30)])]);
	let context = subtable(
		CONTEXTUAL | BOTH_DIRECTIONS,
		1,
		&state_table(7, &classes, &rows[..3], &context_entries, &[subs]),
	);
	let noncontextual = subtable(NONCONTEXTUAL | BOTH_DIRECTIONS, 1, &lookup2(&[(A, C, 40)]));
	morx(&[chain(1, 2, &[noncontextual, ligature, context])])
}

/// Applies the table as `data` to some glyphs; it must come back.
fn survives(data: &[u8]) {
	let table = Table {
		offset: 0,
		len: data.len(),
	};
	if let Some(morx) = Morx::parse(data, table) {
		for glyphs in [
			vec![],
			vec![A],
			vec![A, B, C, A, B],
			vec![A; 64],
			vec![0, 0xffff, 5],
		] {
			let mut glyphs = glyphs;
			morx.apply(data, &mut glyphs);
			assert!(glyphs.len() <= 64);
		}
	}
}

#[test]
fn the_unbroken_table_is_survived() {
	survives(&contextual_and_ligature_font());
}

#[test]
fn every_truncation_is_survived() {
	let data = contextual_and_ligature_font();
	for len in 0..data.len() {
		survives(&data[..len]);
	}
}

#[test]
fn every_byte_set_to_an_extreme_is_survived() {
	let data = contextual_and_ligature_font();
	for at in 0..data.len() {
		for value in [0x00, 0x01, 0x7f, 0x80, 0xff] {
			let mut broken = data.clone();
			broken[at] = value;
			survives(&broken);
		}
	}
}

#[test]
fn every_two_byte_word_set_to_an_extreme_is_survived() {
	let data = contextual_and_ligature_font();
	for at in 0..data.len() - 1 {
		for word in [0x0000u16, 0x7fff, 0x8000, 0xffff] {
			let mut broken = data.clone();
			broken[at..at + 2].copy_from_slice(&word.to_be_bytes());
			survives(&broken);
		}
	}
}

#[test]
fn counts_far_larger_than_the_data_are_survived() {
	let mut data = contextual_and_ligature_font();
	// The chain count, then the chain's length, feature and subtable counts.
	for at in [4, 12, 16, 20] {
		let saved = data.clone();
		data[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
		survives(&data);
		data = saved;
	}
}

#[test]
fn a_subtable_shorter_than_its_header_does_not_stall_the_chain() {
	let mut data = contextual_and_ligature_font();
	// The first subtable's length is at 8 + 16 + 24.
	data[48..52].copy_from_slice(&0u32.to_be_bytes());
	survives(&data);
}

#[test]
fn a_chain_after_a_broken_one_is_not_reached() {
	let broken = chain(
		1,
		0,
		&[subtable(NONCONTEXTUAL | BOTH_DIRECTIONS, 1, &[0xff; 3])],
	);
	let good = chain(
		1,
		0,
		&[subtable(
			NONCONTEXTUAL | BOTH_DIRECTIONS,
			1,
			&lookup6(&[(A, 30)]),
		)],
	);
	let data = morx(&[broken, good]);
	let table = Table {
		offset: 0,
		len: data.len(),
	};
	let mut glyphs = vec![A];
	Morx::parse(&data, table).unwrap().apply(&data, &mut glyphs);
	assert_eq!(
		glyphs,
		[30],
		"the lookup finds nothing, then the next chain runs"
	);
}

#[test]
fn many_chains_are_capped() {
	// Every chain moves any glyph one on, so each chain that runs shows.
	let next: Vec<u16> = (1..=300).collect();
	let each = chain(
		1,
		0,
		&[subtable(
			NONCONTEXTUAL | BOTH_DIRECTIONS,
			1,
			&lookup0(&next),
		)],
	);
	let data = morx(&vec![each; 100]);
	let table = Table {
		offset: 0,
		len: data.len(),
	};
	let mut glyphs = vec![A];
	Morx::parse(&data, table).unwrap().apply(&data, &mut glyphs);
	assert_eq!(glyphs, [A + 16], "only sixteen chains run");
}

#[test]
fn many_subtables_are_capped() {
	let next: Vec<u16> = (1..=600).collect();
	let each = subtable(NONCONTEXTUAL | BOTH_DIRECTIONS, 1, &lookup0(&next));
	let data = morx(&[chain(1, 0, &vec![each; 400])]);
	let table = Table {
		offset: 0,
		len: data.len(),
	};
	let mut glyphs = vec![A];
	Morx::parse(&data, table).unwrap().apply(&data, &mut glyphs);
	assert_eq!(glyphs, [A + 256], "only 256 subtables run");
}
