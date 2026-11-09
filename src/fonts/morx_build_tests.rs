//! Builders for tiny `morx` tables, written from the AAT specification, so
//! tests need no installed font: lookup tables, state tables, subtables.

pub(super) fn be16(value: u16) -> [u8; 2] {
	value.to_be_bytes()
}

pub(super) fn be32(value: u32) -> [u8; 4] {
	value.to_be_bytes()
}

fn words(values: &[u16]) -> Vec<u8> {
	values.iter().flat_map(|&value| be16(value)).collect()
}

/// Format 0: one value per glyph from 0.
pub(super) fn lookup0(values: &[u16]) -> Vec<u8> {
	[&be16(0)[..], &words(values)].concat()
}

/// Format 2: `(first, last, value)` segments in order.
pub(super) fn lookup2(segments: &[(u16, u16, u16)]) -> Vec<u8> {
	let mut out = [
		&be16(2)[..],
		&be16(6),
		&be16(segments.len() as u16),
		&[0; 6],
	]
	.concat();
	for &(first, last, value) in segments {
		out.extend(words(&[last, first, value]));
	}
	out
}

/// Format 4: `(first, last, values)` segments in order, each value list
/// after the units, reached by an offset from the table's start.
pub(super) fn lookup4(segments: &[(u16, u16, Vec<u16>)]) -> Vec<u8> {
	let header = 12 + 6 * segments.len();
	let mut out = [
		&be16(4)[..],
		&be16(6),
		&be16(segments.len() as u16),
		&[0; 6],
	]
	.concat();
	let mut arrays = Vec::new();
	for (first, last, values) in segments {
		out.extend(words(&[*last, *first, (header + arrays.len()) as u16]));
		arrays.extend(words(values));
	}
	out.extend(arrays);
	out
}

/// Format 6: `(glyph, value)` pairs in order.
pub(super) fn lookup6(pairs: &[(u16, u16)]) -> Vec<u8> {
	let mut out = [&be16(6)[..], &be16(4), &be16(pairs.len() as u16), &[0; 6]].concat();
	for &(glyph, value) in pairs {
		out.extend(words(&[glyph, value]));
	}
	out
}

/// Format 8: values for the glyphs from `first`.
pub(super) fn lookup8(first: u16, values: &[u16]) -> Vec<u8> {
	[&words(&[8, first, values.len() as u16])[..], &words(values)].concat()
}

/// Format 10: values of `size` bytes for the glyphs from `first`.
pub(super) fn lookup10(size: u16, first: u16, values: &[u32]) -> Vec<u8> {
	let mut out = words(&[10, size, first, values.len() as u16]);
	for value in values {
		out.extend(&value.to_be_bytes()[4 - usize::from(size)..]);
	}
	out
}

/// An extended state table: `classes` columns, `states` rows of entry
/// numbers, `entries` as words, and the `extras` blobs the header points at
/// after the three offsets, laid out after the entry table.
pub(super) fn state_table(
	classes: u32,
	class_lookup: &[u8],
	states: &[Vec<u16>],
	entries: &[Vec<u16>],
	extras: &[Vec<u8>],
) -> Vec<u8> {
	let header = 16 + 4 * extras.len();
	let class_at = header;
	let states_at = class_at + class_lookup.len();
	let state_bytes: Vec<u8> = states.iter().flat_map(|row| words(row)).collect();
	let entries_at = states_at + state_bytes.len();
	let entry_bytes: Vec<u8> = entries.iter().flat_map(|entry| words(entry)).collect();
	let mut at = entries_at + entry_bytes.len();
	let mut out = [
		be32(classes),
		be32(class_at as u32),
		be32(states_at as u32),
		be32(entries_at as u32),
	]
	.concat();
	for extra in extras {
		out.extend(be32(at as u32));
		at += extra.len();
	}
	out.extend(class_lookup);
	out.extend(state_bytes);
	out.extend(entry_bytes);
	extras.iter().for_each(|extra| out.extend(extra));
	out
}

/// Ligature actions: each `(last, store, offset)`.
pub(super) fn actions(list: &[(bool, bool, i32)]) -> Vec<u8> {
	list
		.iter()
		.flat_map(|&(last, store, offset)| {
			let word = u32::from(last) << 31 | u32::from(store) << 30 | (offset as u32 & 0x3fff_ffff);
			be32(word)
		})
		.collect()
}

/// Contextual substitution tables: offsets from the start, then the lookups.
pub(super) fn substitutions(lookups: &[Vec<u8>]) -> Vec<u8> {
	let mut at = 4 * lookups.len();
	let mut out = Vec::new();
	for lookup in lookups {
		out.extend(be32(at as u32));
		at += lookup.len();
	}
	lookups.iter().for_each(|lookup| out.extend(lookup));
	out
}

pub(super) const NONCONTEXTUAL: u32 = 4;
pub(super) const LIGATURE: u32 = 2;
pub(super) const CONTEXTUAL: u32 = 1;
pub(super) const REARRANGEMENT: u32 = 0;
pub(super) const INSERTION: u32 = 5;
pub(super) const VERTICAL: u32 = 0x8000_0000;
pub(super) const BOTH_DIRECTIONS: u32 = 0x2000_0000;

/// A subtable: `coverage` (type and flags), the features it belongs to.
pub(super) fn subtable(coverage: u32, features: u32, body: &[u8]) -> Vec<u8> {
	[
		&be32(12 + body.len() as u32)[..],
		&be32(coverage),
		&be32(features),
		body,
	]
	.concat()
}

/// A chain with `defaults` flags on, `feature_entries` empty feature
/// records, and `subtables`.
pub(super) fn chain(defaults: u32, feature_entries: u32, subtables: &[Vec<u8>]) -> Vec<u8> {
	let body: Vec<u8> = subtables.concat();
	let length = 16 + 12 * feature_entries as usize + body.len();
	[
		&be32(defaults)[..],
		&be32(length as u32),
		&be32(feature_entries),
		&be32(subtables.len() as u32),
		&vec![0; 12 * feature_entries as usize],
		&body,
	]
	.concat()
}

/// A `morx` table of version 3 with `chains`.
pub(super) fn morx(chains: &[Vec<u8>]) -> Vec<u8> {
	[
		&be16(3)[..],
		&be16(0),
		&be32(chains.len() as u32),
		&chains.concat(),
	]
	.concat()
}

/// The classes the tests use: end of text 0, out of bounds 1, deleted 2.
pub(super) const A: u16 = 10;
pub(super) const B: u16 = 11;
pub(super) const C: u16 = 12;
pub(super) const LIG_AB: u16 = 20;
pub(super) const LIG_ABC: u16 = 21;
