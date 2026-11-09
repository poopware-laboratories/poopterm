//! Tests of the AAT lookup tables `morx` uses for classes and substitutions.
use super::{morx_build_tests::*, morx_lookup::lookup};

#[test]
fn format_0_has_a_value_for_each_glyph() {
	let table = lookup0(&[7, 8, 9]);
	assert_eq!(lookup(&table, 0, 0), Some(7));
	assert_eq!(lookup(&table, 0, 2), Some(9));
	assert_eq!(lookup(&table, 0, 3), None, "past the end of the array");
}

#[test]
fn format_2_finds_glyphs_in_a_segment_by_binary_search() {
	let table = lookup2(&[(3, 5, 100), (10, 10, 200), (20, 90, 300)]);
	for (glyph, want) in [
		(3, Some(100)),
		(5, Some(100)),
		(10, Some(200)),
		(20, Some(300)),
		(90, Some(300)),
	] {
		assert_eq!(lookup(&table, 0, glyph), want, "glyph {glyph}");
	}
	for glyph in [0, 2, 6, 9, 11, 19, 91, u16::MAX] {
		assert_eq!(lookup(&table, 0, glyph), None, "glyph {glyph}");
	}
}

#[test]
fn format_4_reads_values_by_offset_per_segment() {
	let table = lookup4(&[(5, 7, vec![50, 60, 70]), (20, 21, vec![1, 2])]);
	assert_eq!(lookup(&table, 0, 5), Some(50));
	assert_eq!(lookup(&table, 0, 7), Some(70));
	assert_eq!(lookup(&table, 0, 21), Some(2));
	assert_eq!(lookup(&table, 0, 8), None);
	assert_eq!(lookup(&table, 0, 4), None);
}

#[test]
fn format_6_matches_single_glyphs() {
	let table = lookup6(&[(2, 20), (9, 90), (400, 4000)]);
	assert_eq!(lookup(&table, 0, 2), Some(20));
	assert_eq!(lookup(&table, 0, 400), Some(4000));
	assert_eq!(lookup(&table, 0, 3), None);
	assert_eq!(lookup(&table, 0, 401), None);
}

#[test]
fn format_8_covers_a_run_from_the_first_glyph() {
	let table = lookup8(30, &[1, 2, 3]);
	assert_eq!(lookup(&table, 0, 30), Some(1));
	assert_eq!(lookup(&table, 0, 32), Some(3));
	assert_eq!(lookup(&table, 0, 29), None);
	assert_eq!(lookup(&table, 0, 33), None);
}

#[test]
fn format_10_reads_values_of_one_to_four_bytes() {
	assert_eq!(lookup(&lookup10(1, 4, &[9, 8]), 0, 5), Some(8));
	assert_eq!(lookup(&lookup10(2, 4, &[900, 800]), 0, 4), Some(900));
	// Wider values keep their low 16 bits, as a glyph or class would.
	assert_eq!(lookup(&lookup10(4, 4, &[0x0001_0203]), 0, 4), Some(0x0203));
	assert_eq!(lookup(&lookup10(3, 4, &[0x010203]), 0, 4), Some(0x0203));
	assert_eq!(lookup(&lookup10(2, 4, &[900]), 0, 5), None);
}

#[test]
fn a_table_inside_larger_data_is_read_from_its_offset() {
	let mut data = vec![0xee; 6];
	data.extend(lookup6(&[(1, 11), (2, 22)]));
	assert_eq!(lookup(&data, 6, 2), Some(22));
	assert_eq!(lookup(&data, 6, 3), None);
}

#[test]
fn an_unknown_format_finds_nothing() {
	for format in [1u16, 3, 5, 7, 9, 11, 0xffff] {
		let mut table = lookup8(0, &[1, 2, 3]);
		table[..2].copy_from_slice(&format.to_be_bytes());
		assert_eq!(lookup(&table, 0, 1), None, "format {format}");
	}
}

#[test]
fn truncated_tables_find_nothing_instead_of_failing() {
	let tables = [
		lookup0(&[1, 2, 3]),
		lookup2(&[(1, 3, 5), (9, 9, 6)]),
		lookup4(&[(1, 3, vec![1, 2, 3])]),
		lookup6(&[(1, 2), (3, 4)]),
		lookup8(1, &[1, 2, 3]),
		lookup10(2, 1, &[1, 2, 3]),
	];
	for table in &tables {
		for len in 0..table.len() {
			for glyph in [0, 1, 2, 3, 9, 300] {
				let _ = lookup(&table[..len], 0, glyph);
			}
		}
	}
}

#[test]
fn an_offset_past_the_data_finds_nothing() {
	let table = lookup8(0, &[1]);
	assert_eq!(lookup(&table, table.len() + 100, 0), None);
	assert_eq!(lookup(&table, usize::MAX, 0), None);
}

#[test]
fn a_segment_count_larger_than_the_data_is_survived() {
	let mut table = lookup2(&[(1, 2, 3)]);
	table[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
	assert_eq!(lookup(&table, 0, 1), None);
	let mut table = lookup6(&[(1, 2)]);
	table[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
	assert_eq!(lookup(&table, 0, 1), None);
}
