//! `morx` through a whole font: an sfnt file carrying the table, shaped the
//! way the renderer does it, through `Face` and `Font`.
use super::{face::Face, font::Font, morx_build_tests::*};

const BUNDLED: &[u8] = include_bytes!("../../assets/font.ttf");

/// The bundled font with `morx` added (or replaced) as its table.
fn with_morx(table: &[u8]) -> Vec<u8> {
	let be = |at: usize, len: usize| {
		BUNDLED[at..at + len]
			.iter()
			.fold(0, |n, &b| n << 8 | b as usize)
	};
	let mut tables: Vec<([u8; 4], Vec<u8>)> = (0..be(4, 2))
		.map(|i| {
			let at = 12 + 16 * i;
			let (offset, len) = (be(at + 8, 4), be(at + 12, 4));
			let tag = BUNDLED[at..at + 4].try_into().unwrap();
			(tag, BUNDLED[offset..offset + len].to_vec())
		})
		.filter(|(tag, _)| tag != b"morx")
		.collect();
	tables.push((*b"morx", table.to_vec()));
	let mut out = BUNDLED[..4].to_vec();
	out.extend((tables.len() as u16).to_be_bytes());
	out.extend([0; 6]);
	let mut data = Vec::new();
	for (tag, bytes) in &tables {
		out.extend(tag);
		out.extend([0; 4]);
		out.extend(
			(12 + 16 * tables.len() + data.len())
				.to_be_bytes()
				.map(|b| b)[4..]
				.iter(),
		);
		out.extend((bytes.len() as u32).to_be_bytes());
		data.extend(bytes);
		data.resize(data.len().div_ceil(4) * 4, 0);
	}
	out.extend(data);
	out
}

fn face(bytes: Vec<u8>) -> Face {
	Face::parse(bytes.into(), 0).unwrap()
}

/// `morx` that joins the glyphs of `a` then `b` into the glyph of `into`.
fn joining(a: char, b: char, into: char) -> Vec<u8> {
	let plain = face(BUNDLED.to_vec());
	let glyph = |ch| plain.glyph_index(ch);
	let (ga, gb) = (glyph(a), glyph(b));
	let mut pairs = [(ga, 4), (gb, 5)];
	pairs.sort_unstable();
	let classes = lookup6(&pairs);
	let rows = vec![
		vec![0, 0, 0, 0, 1, 0],
		vec![0, 0, 0, 0, 1, 0],
		vec![0, 0, 3, 0, 1, 2],
	];
	let entries = vec![
		vec![0, 0, 0],
		vec![2, 0x8000, 0],
		vec![0, 0xa000, 0],
		vec![2, 0, 0],
	];
	let mut components = vec![0u16; usize::from(ga.max(gb)) + 1];
	components[usize::from(gb)] = 1;
	let words = |values: &[u16]| -> Vec<u8> { values.iter().flat_map(|&n| be16(n)).collect() };
	let list = actions(&[(false, false, 0), (true, false, 0)]);
	let extras = [list, words(&components), words(&[0, glyph(into)])];
	let body = state_table(6, &classes, &rows, &entries, &extras);
	morx(&[chain(
		1,
		0,
		&[subtable(LIGATURE | BOTH_DIRECTIONS, 1, &body)],
	)])
}

#[test]
fn a_face_with_morx_joins_a_pair_into_its_ligature_glyph() {
	let face = face(with_morx(&joining('A', 'B', 'C')));
	let glyph = |ch| face.glyph_index(ch);
	assert_eq!(face.shape(&['A', 'B']), [glyph('C')]);
	assert_eq!(
		face.shape(&['x', 'A', 'B', 'y']),
		[glyph('x'), glyph('C'), glyph('y')]
	);
}

#[test]
fn a_face_without_morx_keeps_the_glyphs_apart() {
	let face = face(BUNDLED.to_vec());
	let glyph = |ch| face.glyph_index(ch);
	assert_eq!(face.shape(&['A', 'B']), [glyph('A'), glyph('B')]);
}

#[test]
fn a_pair_the_table_does_not_join_stays_apart() {
	let face = face(with_morx(&joining('A', 'B', 'C')));
	let glyph = |ch| face.glyph_index(ch);
	assert_eq!(face.shape(&['B', 'A']), [glyph('B'), glyph('A')]);
	assert_eq!(face.shape(&['A', 'A']), [glyph('A'), glyph('A')]);
}

#[test]
fn a_font_reports_a_joined_sequence_as_a_ligature() {
	let font = Font::from_bytes(with_morx(&joining('A', 'B', 'C')), 0).unwrap();
	assert_eq!(
		font.ligature(&['A', 'B']),
		Some(font.lookup_glyph_index('C'))
	);
	assert_eq!(font.ligature(&['B', 'A']), None);
	assert_eq!(font.ligature(&['A']), None, "one character is no sequence");
}

#[test]
fn a_variation_selector_the_table_ignores_is_left_out_before_joining() {
	let font = Font::from_bytes(with_morx(&joining('A', 'B', 'C')), 0).unwrap();
	let joined = font.ligature(&['A', '\u{fe0f}', 'B']);
	assert_eq!(joined, Some(font.lookup_glyph_index('C')));
}

#[test]
fn a_cluster_is_cut_to_the_most_glyphs_one_may_have() {
	let face = face(with_morx(&joining('A', 'B', 'C')));
	assert!(face.shape(&vec!['A'; 500]).len() <= 64);
	assert!(face.shape(&['A', 'B'].repeat(200)).len() <= 64);
}

#[test]
fn a_morx_of_an_unknown_version_changes_nothing() {
	let mut table = joining('A', 'B', 'C');
	table[..2].copy_from_slice(&1u16.to_be_bytes());
	let face = face(with_morx(&table));
	let glyph = |ch| face.glyph_index(ch);
	assert_eq!(face.shape(&['A', 'B']), [glyph('A'), glyph('B')]);
}

#[test]
fn a_truncated_morx_does_not_stop_the_font_loading() {
	let table = joining('A', 'B', 'C');
	for len in [0, 1, 7, 8, 20, table.len() - 1] {
		let face = face(with_morx(&table[..len]));
		let glyph = |ch| face.glyph_index(ch);
		assert_eq!(
			face.shape(&['A', 'B']),
			[glyph('A'), glyph('B')],
			"{len} bytes"
		);
	}
}
