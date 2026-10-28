//! Whole-cell ASCII stores preserve styles, character validity and slice bounds.
use super::ascii::write;
use crate::term::Cell;

fn template() -> Cell {
	Cell {
		ch: '\u{10ffff}',
		fg: 0x123456,
		bg: 0xabcdef,
		attrs: 0xff,
		width: 1,
		extra: 0x7654,
	}
}

#[test]
fn every_byte_becomes_a_valid_character_without_changing_style() {
	let bytes: Vec<_> = (0..=255u8).collect();
	let base = template();
	let mut cells = vec![Cell::default(); bytes.len()];
	write(&mut cells, &bytes, base);
	let expected: Vec<_> = bytes
		.iter()
		.map(|&b| Cell {
			ch: b as char,
			..base
		})
		.collect();
	assert_eq!(cells, expected);
}

#[test]
fn old_character_bits_are_cleared_from_the_template() {
	for ch in ['\0', ' ', '猫', '🦀', '\u{10ffff}'] {
		let base = Cell { ch, ..template() };
		let bytes = b" !0Az~";
		let mut cells = vec![base; bytes.len()];
		write(&mut cells, bytes, base);
		for (cell, &byte) in cells.iter().zip(bytes) {
			assert_eq!(
				*cell,
				Cell {
					ch: byte as char,
					..base
				}
			);
		}
	}
}

#[test]
fn all_style_fields_survive_including_high_bits() {
	let styles = [
		Cell::default(),
		Cell {
			fg: u32::MAX,
			bg: 0,
			..template()
		},
		Cell {
			fg: 0,
			bg: u32::MAX,
			..template()
		},
		Cell {
			attrs: 0,
			width: 0,
			extra: 0,
			..template()
		},
		Cell {
			attrs: 0x80,
			width: 2,
			extra: u16::MAX,
			..template()
		},
		Cell {
			attrs: 1,
			width: u8::MAX,
			extra: 0x8000,
			..template()
		},
	];
	for base in styles {
		let mut cells = [Cell::default(); 17];
		write(&mut cells, b"0123456789abcdefg", base);
		for (cell, byte) in cells.iter().zip(b"0123456789abcdefg") {
			assert_eq!(
				*cell,
				Cell {
					ch: *byte as char,
					..base
				}
			);
		}
	}
}

#[test]
fn all_lengths_and_cell_offsets_leave_neighbours_unchanged() {
	let sentinel = Cell {
		ch: '猫',
		width: 2,
		..template()
	};
	for offset in 0..17 {
		for length in [0, 1, 2, 3, 4, 7, 8, 15, 16, 17, 31, 32, 33, 120, 129] {
			let bytes: Vec<_> = (0..length).map(|i| b' ' + (i % 95) as u8).collect();
			let mut cells = vec![sentinel; offset + length + 17];
			let mut expected = cells.clone();
			for (cell, byte) in expected[offset..offset + length].iter_mut().zip(&bytes) {
				*cell = Cell {
					ch: *byte as char,
					..template()
				};
			}
			write(&mut cells[offset..offset + length], &bytes, template());
			assert_eq!(cells, expected, "offset {offset} length {length}");
		}
	}
}

#[test]
fn unaligned_input_byte_slices_are_supported() {
	let input: Vec<_> = (0..=255u8).collect();
	for start in 0..32 {
		let bytes = &input[start..start + 129];
		let mut cells = vec![Cell::default(); bytes.len()];
		write(&mut cells, bytes, template());
		let expected: Vec<_> = bytes
			.iter()
			.map(|&b| Cell {
				ch: b as char,
				..template()
			})
			.collect();
		assert_eq!(cells, expected, "input offset {start}");
	}
}

#[test]
fn length_mismatch_panics_before_modifying_any_cell() {
	for (cells_len, bytes_len) in [(0, 1), (1, 0), (2, 3), (3, 2), (16, 17), (17, 16)] {
		let mut cells = vec![template(); cells_len];
		let expected = cells.clone();
		let bytes = vec![b'x'; bytes_len];
		let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			write(&mut cells, &bytes, Cell::default());
		}));
		assert!(result.is_err(), "{cells_len} cells, {bytes_len} bytes");
		assert_eq!(cells, expected);
	}
}
