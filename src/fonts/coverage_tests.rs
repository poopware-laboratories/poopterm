//! Tests that the coverage the font database records is exact: for every
//! font, the characters it lists are those the font engine draws, so
//! fallback never loads a font for a character it lacks.
use super::{
	Font,
	cmap::Cmap,
	coverage::{Ranges, covers, tests::ranges},
	db,
	fallback::{SystemFallback, candidates},
	fallback_tests::{font_mapping, with_cmap},
	sfnt::read_faces,
};
use std::{fs, io::Cursor, path::PathBuf, sync::Arc};

const BUNDLED: &[u8] = include_bytes!("../../assets/font.ttf");

/// The coverage the database records for the first face of `bytes`.
fn scanned(bytes: &[u8]) -> Ranges {
	read_faces(&mut Cursor::new(bytes)).unwrap()[0]
		.coverage
		.to_vec()
}

/// The characters below `end` the engine draws from `bytes`, looked up one
/// by one; U+FFFF, a noncharacter only format 4's end segment lists, is
/// left out as coverage leaves it out.
fn drawn(bytes: &[u8], end: u32) -> Ranges {
	let font = Font::from_bytes(bytes.to_vec(), 0).unwrap();
	let mapped = (0..end)
		.filter(|&code| code != 0xffff)
		.filter_map(char::from_u32)
		.filter(|&ch| font.lookup_glyph_index(ch) != 0);
	ranges(mapped.map(|ch| (ch.into(), ch.into())))
}

/// A `cmap` table with one subtable for `platform` and `encoding`.
fn cmap(platform: u16, encoding: u16, subtable: &[u8]) -> Vec<u8> {
	let mut table = Vec::new();
	for value in [0, 1, platform, encoding, 0, 12] {
		table.extend(u16::to_be_bytes(value));
	}
	table.extend(subtable);
	table
}

/// A format 4 segment: start, end, idDelta and its glyph array.
type Segment<'a> = (u16, u16, u16, &'a [u16]);

/// A format 4 subtable of `(start, end, delta, glyphs)` segments: `delta`
/// alone when `glyphs` is empty, else a glyph array, then the end segment.
fn format4(segments: &[Segment]) -> Vec<u8> {
	let mut segments = segments.to_vec();
	segments.push((0xffff, 0xffff, 1, &[]));
	let count = segments.len() as u16;
	let mut table = Vec::new();
	for value in [4, 0, 0, 2 * count, 0, 0, 0] {
		table.extend(u16::to_be_bytes(value));
	}
	let words = |table: &mut Vec<u8>, pick: fn(&Segment) -> u16| {
		segments
			.iter()
			.for_each(|segment| table.extend(pick(segment).to_be_bytes()))
	};
	words(&mut table, |&(_, end, _, _)| end);
	table.extend([0, 0]);
	words(&mut table, |&(start, _, _, _)| start);
	words(&mut table, |&(_, _, delta, _)| delta);
	// idRangeOffset: from its own word to the segment's glyphs, which
	// follow the offsets in order.
	let mut glyphs = Vec::new();
	for (i, (_, _, _, array)) in segments.iter().enumerate() {
		let offset = if array.is_empty() {
			0
		} else {
			2 * (segments.len() - i + glyphs.len())
		};
		table.extend((offset as u16).to_be_bytes());
		glyphs.extend_from_slice(array);
	}
	glyphs
		.iter()
		.for_each(|glyph| table.extend(glyph.to_be_bytes()));
	let length = table.len() as u16;
	table[2..4].copy_from_slice(&length.to_be_bytes());
	table
}

/// A format 12 subtable of `(first, last, first glyph)` groups.
fn format12(groups: &[(u32, u32, u32)]) -> Vec<u8> {
	let mut table = vec![0, 12, 0, 0];
	table.extend((16 + 12 * groups.len() as u32).to_be_bytes());
	table.extend(0u32.to_be_bytes());
	table.extend((groups.len() as u32).to_be_bytes());
	for &(first, last, glyph) in groups {
		[first, last, glyph]
			.iter()
			.for_each(|value| table.extend(value.to_be_bytes()));
	}
	table
}

/// The bundled font's glyph count (`maxp.numGlyphs`): glyph ids from it
/// on are not drawn.
fn glyph_count() -> u32 {
	let be = |at: usize, len: usize| {
		BUNDLED[at..at + len]
			.iter()
			.fold(0, |n, &b| n << 8 | u32::from(b))
	};
	let record = (0..be(4, 2) as usize)
		.map(|i| 12 + 16 * i)
		.find(|&at| &BUNDLED[at..at + 4] == b"maxp")
		.unwrap();
	be(be(record + 8, 4) as usize + 4, 2)
}

/// Checks a font built from `table` both ways, below `end`, and returns
/// the coverage for the caller to pin.
fn agrees(table: &[u8], end: u32) -> Ranges {
	let bytes = with_cmap(table);
	let coverage = scanned(&bytes);
	assert_eq!(coverage, drawn(&bytes, end), "{table:02x?}");
	coverage
}

#[test]
fn the_bundled_font_covers_exactly_what_it_draws_in_all_of_unicode() {
	let coverage = scanned(BUNDLED);
	assert_eq!(coverage, drawn(BUNDLED, 0x11_0000));
	assert!(coverage.len() > 10, "{coverage:x?}");
}

#[test]
fn format4_deltas_that_wrap_to_glyph_0_are_not_coverage() {
	// U+0041 + 0xFFBF wraps to glyph 0: 'A' is not drawn, 'B' to 'Z' are.
	let table = cmap(3, 1, &format4(&[(0x41, 0x5a, 0xffbf, &[])]));
	assert_eq!(agrees(&table, 0x1_0000), [(0x42, 0x5a)]);
}

#[test]
fn format4_glyph_arrays_cover_only_their_non_zero_entries() {
	// ⏰ to ⏷ with ⏲, ⏵ and ⏶ unmapped, as a font that has only some of a
	// block's symbols lists them; a delta applies to array entries too.
	let array: &[u16] = &[5, 6, 0, 7, 8, 0, 0, 9];
	let table = cmap(3, 1, &format4(&[(0x23f0, 0x23f7, 0, array)]));
	let want = [(0x23f0, 0x23f1), (0x23f3, 0x23f4), (0x23f7, 0x23f7)];
	assert_eq!(agrees(&table, 0x1_0000), want);
	// Entry 5 plus a delta of 0xFFFB wraps to glyph 0.
	let table = cmap(3, 1, &format4(&[(0x23f0, 0x23f7, 0xfffb, array)]));
	assert_eq!(
		agrees(&table, 0x1_0000),
		[(0x23f1, 0x23f1), (0x23f3, 0x23f4), (0x23f7, 0x23f7)]
	);
}

#[test]
fn glyphs_past_the_glyph_count_are_not_coverage() {
	let count = glyph_count();
	// U+0100 maps to the last glyph, U+0101 to the first one past it.
	let table = cmap(3, 10, &format12(&[(0x100, 0x101, count - 1)]));
	assert_eq!(agrees(&table, 0x200), [(0x100, 0x100)]);
	let table = cmap(3, 10, &format12(&[(0x100, 0x1ff, count)]));
	assert_eq!(agrees(&table, 0x200), []);
}

#[test]
fn format12_groups_start_glyph_0_overflow_and_past_unicode_are_handled() {
	let count = glyph_count();
	let groups = [
		// Glyph 0 for the first character only.
		(0x23f4, 0x23f6, 0),
		// Glyph ids past 0xFFFF are not drawn.
		(0x2600, 0x2601, 0x1_0000),
		// The last group runs past Unicode; only its first part counts.
		(0x10_fffe, 0xffff_ffff, 1),
	];
	let table = cmap(3, 10, &format12(&groups));
	let coverage = agrees(&table, 0x11_0000);
	assert_eq!(coverage, [(0x23f5, 0x23f6), (0x10_fffe, 0x10_ffff)]);
	assert!(count > 2);
}

#[test]
fn a_lying_format12_group_count_reads_only_the_groups_present() {
	let mut subtable = format12(&[(0x41, 0x5a, 1)]);
	subtable[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
	assert_eq!(agrees(&cmap(3, 10, &subtable), 0x1_0000), [(0x41, 0x5a)]);
}

#[test]
fn format0_and_format6_subtables_are_coverage_too() {
	// Format 0: 256 one-byte glyph ids; 'A' to 'C' mapped.
	let mut format0 = vec![0, 0, 1, 6, 0, 0];
	format0.extend((0..=255u8).map(|ch| {
		if (0x41..=0x43).contains(&ch) {
			ch - 0x40
		} else {
			0
		}
	}));
	assert_eq!(agrees(&cmap(3, 1, &format0), 0x1_0000), [(0x41, 0x43)]);
	// Format 6: U+0400 on, four glyphs, the third unmapped.
	let mut format6 = Vec::new();
	for value in [6, 18, 0, 0x400, 4, 1, 2, 0, 3] {
		format6.extend(u16::to_be_bytes(value));
	}
	assert_eq!(
		agrees(&cmap(3, 1, &format6), 0x1_0000),
		[(0x400, 0x401), (0x403, 0x403)]
	);
	// One character is the smallest table.
	let mut single = Vec::new();
	for value in [6, 12, 0, 0x2603, 1, 7] {
		single.extend(u16::to_be_bytes(value));
	}
	assert_eq!(agrees(&cmap(3, 1, &single), 0x1_0000), [(0x2603, 0x2603)]);
	// A format 6 table with no characters covers none.
	let mut empty = Vec::new();
	for value in [6, 10, 0, 0x400, 0] {
		empty.extend(u16::to_be_bytes(value));
	}
	assert_eq!(agrees(&cmap(3, 1, &empty), 0x1_0000), []);
}

#[test]
fn symbol_subtables_the_engine_reads_are_coverage() {
	// Windows symbol fonts (3, 0) list their glyphs at U+F000 and up; the
	// engine draws them, so fallback must find them.
	let table = cmap(
		3,
		0,
		&format4(&[(0xf020, 0xf07e, 1u16.wrapping_sub(0xf020), &[])]),
	);
	assert_eq!(agrees(&table, 0x1_0000), [(0xf020, 0xf07e)]);
}

#[test]
fn every_character_of_one_page_gets_its_own_best_font() {
	// 0.ttf, the smaller, has only ⏴; 1.ttf has both. ⏵ is found in
	// 1.ttf first, and ⏴ still goes to 0.ttf, its best ranked font.
	let dir = std::env::temp_dir().join(format!("poopterm-coverage-{}-own", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	fs::write(dir.join("0.ttf"), font_mapping(&[('⏴', 'A')])).unwrap();
	fs::write(dir.join("1.ttf"), font_mapping(&[('⏴', 'B'), ('⏵', 'C')])).unwrap();
	let (fonts, _) = db::refresh(std::slice::from_ref(&dir), None);
	assert_eq!(candidates(&fonts, '⏴', false).len(), 2);
	let mut system = SystemFallback::new(move || fonts);
	let mut loaded = Vec::new();
	assert_eq!(system.find('⏵', false, &mut loaded), Some(0));
	assert_eq!(
		system.find('⏴', false, &mut loaded),
		Some(1),
		"0.ttf, newly loaded"
	);
	let bundled = Font::from_bytes(BUNDLED, 0).unwrap();
	assert_eq!(
		loaded[1].lookup_glyph_index('⏴'),
		bundled.lookup_glyph_index('A')
	);
	fs::remove_dir_all(dir).unwrap();
}

#[test]
fn faces_with_the_same_characters_share_one_set_in_the_cache() {
	let dir = std::env::temp_dir().join(format!("poopterm-coverage-{}-shared", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	for name in ["a.ttf", "b.ttf"] {
		fs::write(dir.join(name), BUNDLED).unwrap();
	}
	fs::write(dir.join("c.ttf"), font_mapping(&[('⏵', 'A')])).unwrap();
	let (cache, _) = db::refresh(std::slice::from_ref(&dir), None);
	let text = db::encode(&cache);
	assert_eq!(text.matches("\t=0\t").count(), 1, "{text}");
	assert!(text.contains("\t23f5\t"), "{text}");
	let read = db::decode(&text).unwrap();
	assert_eq!(read, cache);
	let coverage = |file: usize| &read.files[file].faces[0].coverage;
	assert!(Arc::ptr_eq(coverage(0), coverage(1)));
	assert!(!Arc::ptr_eq(coverage(0), coverage(2)));
	// A reference to a set that comes later is rejected.
	let later = text.replacen("\t=0\t", "\t=1\t", 1);
	assert_eq!(db::decode(&later), None);
	fs::remove_dir_all(dir).unwrap();
}

/// Every installed face: `cargo test --release -- --ignored installed_coverage`.
#[test]
#[ignore = "reads the fonts installed on this machine"]
fn installed_coverage_is_what_each_font_draws() {
	let cache = super::system_fonts();
	let (mut checked, mut failures) = (0, Vec::new());
	for ((path, index), face) in cache.faces() {
		let Some(font) = fs::read(path)
			.ok()
			.and_then(|bytes| Font::from_bytes(bytes, index).ok())
		else {
			continue;
		};
		// Every covered character, and the ones on either side of a range.
		let edges = face
			.coverage
			.iter()
			.flat_map(|&(first, last)| [first.wrapping_sub(1), last + 1]);
		let listed = face.coverage.iter().flat_map(|&(first, last)| first..=last);
		for code in listed.chain(edges).filter(|&code| code != 0xffff) {
			let Some(ch) = char::from_u32(code) else {
				continue;
			};
			checked += 1;
			if covers(&face.coverage, ch) != (font.lookup_glyph_index(ch) != 0) {
				failures.push(format!(
					"{} ({}): U+{code:04X}",
					face.family,
					PathBuf::from(path).display()
				));
			}
		}
	}
	for failure in failures.iter().take(50) {
		eprintln!("{failure}");
	}
	eprintln!("{checked} characters checked, {} disagree", failures.len());
	assert!(checked > 0 && failures.is_empty());
}

#[test]
fn format12_groups_out_of_order_or_overlapping_follow_the_lookup() {
	let tables = [
		// Out of order: the binary search misses the first group.
		format12(&[(0x2600, 0x26ff, 1), (0x41, 0x5a, 1)]),
		// Overlapping: which group maps U+0050 depends on the search.
		format12(&[(0x41, 0x5a, 1), (0x50, 0x7a, 40), (0x100, 0x17f, 3)]),
		// Reversed group in the middle.
		format12(&[(0x41, 0x5a, 1), (0x80, 0x70, 1), (0x100, 0x17f, 3)]),
		// Adjacent groups are in order, and merge.
		format12(&[(0x41, 0x4f, 1), (0x50, 0x5a, 16)]),
		// A last group ending at u32::MAX cannot be followed.
		format12(&[(0x41, 0x5a, 1), (0x10_0000, u32::MAX, 1)]),
	];
	let coverage: Vec<_> = tables
		.iter()
		.map(|subtable| agrees(&cmap(3, 10, subtable), 0x11_0000))
		.collect();
	assert_eq!(coverage[3], [(0x41, 0x5a)]);
	// Glyphs 1 to the last: one character fewer than the glyph count.
	let last = 0x10_0000 + glyph_count() - 2;
	assert_eq!(coverage[4], [(0x41, 0x5a), (0x10_0000, last)]);
	assert!(!coverage[1].is_empty() && !coverage[2].is_empty());
}

#[test]
fn format4_segments_with_repeated_ends_or_unreadable_arrays_follow_the_lookup() {
	let tables = [
		// Two segments end at U+005A: the first is the one found.
		format4(&[(0x41, 0x5a, 0xffc0, &[]), (0x50, 0x5a, 0, &[])]),
		// A segment starting inside the one before it.
		format4(&[(0x41, 0x5a, 0xffc0, &[]), (0x50, 0x7a, 0xffc0, &[])]),
	];
	let coverage: Vec<_> = tables
		.iter()
		.map(|subtable| agrees(&cmap(3, 1, subtable), 0x1_0000))
		.collect();
	assert_eq!(coverage[0], [(0x41, 0x5a)]);
	assert_eq!(coverage[1], [(0x41, 0x7a)]);
	// A segment count larger than the table: the engine reads its arrays
	// on past the table, the database stops at its end and records less.
	let mut lying = format4(&[(0x41, 0x5a, 0xffc0, &[])]);
	lying[6..8].copy_from_slice(&40u16.to_be_bytes());
	let bytes = with_cmap(&cmap(3, 1, &lying));
	let drawn = drawn(&bytes, 0x1_0000);
	let scanned = scanned(&bytes);
	assert!(!drawn.is_empty());
	assert!(scanned.iter().all(|&(first, last)| {
		(first..=last).all(|code| covers(&drawn, char::from_u32(code).unwrap()))
	}));
}

#[test]
fn a_glyph_array_running_past_its_table_covers_only_what_the_table_holds() {
	// The array lists three glyphs for ten characters. The engine reads on
	// into whatever bytes follow the table in the file; the database reads
	// only the table, so it records the three and never claims more.
	let short = cmap(3, 1, &format4(&[(0x2190, 0x2199, 0, &[3, 4, 5])]));
	let bytes = with_cmap(&short);
	assert_eq!(scanned(&bytes), [(0x2190, 0x2192)]);
	let drawn = drawn(&bytes, 0x1_0000);
	assert!(covers(&drawn, '←') && covers(&drawn, '→'));
	assert!(scanned(&bytes).iter().all(|&(first, last)| {
		(first..=last).all(|code| covers(&drawn, char::from_u32(code).unwrap()))
	}));
}

/// Checks [`Cmap::coverage`] against [`Cmap::glyph`] on one `cmap` table,
/// for every character below `end` but U+FFFF: no font around it, so
/// malformed tables cannot send either past the bytes the other sees.
fn agrees_in_table(table: &[u8], end: u32) -> Ranges {
	let engine = Cmap::parse(table, 0).unwrap();
	let coverage = engine.coverage(table, u16::MAX);
	let mapped = (0..end).filter(|&code| code != 0xffff).filter(|&code| {
		let glyph = engine.glyph(table, code);
		glyph != 0 && glyph < u16::MAX
	});
	assert_eq!(
		coverage,
		ranges(mapped.map(|code| (code, code))),
		"{table:02x?}"
	);
	coverage
}

#[test]
fn malformed_tables_are_covered_exactly_as_the_lookup_reads_them() {
	// A reversed group steers the binary search away from the group after
	// it: U+0100 to U+017F are listed but never drawn.
	let steered = format12(&[(0x41, 0x5a, 1), (0x200, 0x50, 1), (0x100, 0x17f, 3)]);
	assert_eq!(
		agrees_in_table(&cmap(3, 10, &steered), 0x1_0000),
		[(0x41, 0x5a)]
	);
	// A group running past Unicode, in order: up to U+10FFFF only.
	let past = format12(&[(0x41, 0x5a, 1), (0x10_fff0, 0x20_0000, 1)]);
	assert_eq!(
		agrees_in_table(&cmap(3, 10, &past), 0x11_0000),
		[(0x41, 0x5a), (0x10_fff0, 0x10_ffff)]
	);
	// A segment count past the end of the table: the lookup cannot read
	// the ends it searches, and draws nothing.
	let mut lying = format4(&[(0x41, 0x5a, 0xffc0, &[])]);
	lying[6..8].copy_from_slice(&40u16.to_be_bytes());
	assert_eq!(agrees_in_table(&cmap(3, 1, &lying), 0x1_0000), []);
	// Two groups sharing U+005A: the lookup finds the second, which maps
	// it to glyph 0.
	let shared = format12(&[(0x41, 0x5a, 1), (0x5a, 0x60, 0)]);
	assert_eq!(
		agrees_in_table(&cmap(3, 10, &shared), 0x1_0000),
		[(0x41, 0x59), (0x5b, 0x60)]
	);
	// Segment counts a little past the real two: some of the arrays the
	// lookup reads are there, shifted, and some are not.
	for count in 2..=12u16 {
		let mut table = format4(&[(0x41, 0x5a, 0xffc0, &[])]);
		table[6..8].copy_from_slice(&(2 * count).to_be_bytes());
		agrees_in_table(&cmap(3, 1, &table), 0x1_0000);
	}
	// Groups that are in order but end at u32::MAX are walked to Unicode's
	// end; a group after one is out of order.
	let wide = format12(&[
		(0x41, 0x5a, 1),
		(0x10_fff0, u32::MAX, 1),
		(u32::MAX, u32::MAX, 1),
	]);
	assert_eq!(
		agrees_in_table(&cmap(3, 10, &wide), 0x11_0000),
		[(0x41, 0x5a), (0x10_fff0, 0x10_ffff)]
	);
}
