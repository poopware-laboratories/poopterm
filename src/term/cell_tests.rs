//! A cell's 128-bit view, `bits`, against the cell's bytes in memory.
use super::Cell;
use crate::bench::corpus::Rng;

/// The cell's 16 bytes as they lie in memory, read as one little-endian word.
fn memory(cell: Cell) -> u128 {
	// SAFETY: `Cell` is 16 bytes of plain integers with no padding, so every
	// byte is initialized.
	unsafe { std::mem::transmute::<Cell, u128>(cell) }
}

/// A cell with every field random and every character a valid one.
fn random(rng: &mut Rng) -> Cell {
	let ch = loop {
		if let Some(ch) = char::from_u32(rng.next() as u32 % 0x11_0000) {
			break ch;
		}
	};
	let bits = rng.next();
	Cell {
		ch,
		fg: rng.next() as u32,
		bg: (rng.next() >> 32) as u32,
		attrs: bits as u8,
		width: (bits >> 8) as u8,
		extra: (bits >> 16) as u16,
	}
}

/// Cells whose fields sit at their extremes, where a shift or a mask that
/// is off by a byte shows.
fn extremes() -> Vec<Cell> {
	let full = Cell {
		ch: char::MAX,
		fg: u32::MAX,
		bg: u32::MAX,
		attrs: u8::MAX,
		width: u8::MAX,
		extra: u16::MAX,
	};
	vec![
		Cell::default(),
		full,
		Cell {
			ch: '\0',
			fg: 0,
			bg: 0,
			attrs: 0,
			width: 0,
			extra: 0,
		},
		Cell {
			extra: 0x8001,
			..Cell::default()
		},
		Cell {
			width: 0x80,
			..Cell::default()
		},
		Cell {
			attrs: 0x80,
			..Cell::default()
		},
		Cell {
			bg: 0x8000_0001,
			..Cell::default()
		},
		Cell {
			ch: '\u{d7ff}',
			..full
		},
		Cell {
			ch: '\u{e000}',
			..full
		},
	]
}

fn cells() -> Vec<Cell> {
	let mut rng = Rng::new(0x5eed_ce11);
	let mut cells = extremes();
	cells.extend((0..2000).map(|_| random(&mut rng)));
	cells
}

#[test]
fn bits_are_the_bytes_of_the_cell_in_memory() {
	for cell in cells() {
		assert_eq!(cell.bits(), memory(cell), "{cell:?}");
	}
}

#[test]
fn bits_are_equal_exactly_when_the_cells_are() {
	let cells = cells();
	for pair in cells.windows(2) {
		assert_eq!(
			pair[0].bits() == pair[1].bits(),
			pair[0] == pair[1],
			"{pair:?}"
		);
	}
	// Each field alone must move the bits.
	let base = Cell::default();
	let one_off = [
		Cell { ch: 'b', ..base },
		Cell {
			fg: base.fg ^ 1,
			..base
		},
		Cell {
			bg: base.bg ^ 1,
			..base
		},
		Cell {
			attrs: base.attrs ^ 1,
			..base
		},
		Cell {
			width: base.width ^ 1,
			..base
		},
		Cell {
			extra: base.extra ^ 1,
			..base
		},
	];
	for cell in one_off {
		assert_ne!(cell.bits(), base.bits(), "{cell:?}");
	}
}

#[test]
fn put_writes_the_template_with_the_character_over_any_cell() {
	let mut rng = Rng::new(0x9e7);
	let cells = cells();
	for pair in cells.windows(2) {
		let (old, template) = (pair[0], pair[1]);
		let ch = random(&mut rng).ch;
		let mut slot = old;
		Cell::put(&mut slot, template, ch);
		assert_eq!(
			slot,
			Cell { ch, ..template },
			"{old:?} <- {template:?} {ch:?}"
		);
		assert_eq!(
			slot.bits() >> 32,
			template.bits() >> 32,
			"fields after the character"
		);
	}
}

#[test]
fn put_touches_only_its_own_slot() {
	let fence = Cell {
		ch: '#',
		fg: 1,
		bg: 2,
		attrs: 3,
		width: 1,
		extra: 4,
	};
	let template = Cell {
		ch: '\0',
		width: 2,
		extra: 0xbeef,
		..Cell::default()
	};
	for at in 0..5 {
		let mut row = [fence; 5];
		Cell::put(&mut row[at], template, '猫');
		for (i, cell) in row.iter().enumerate() {
			let expected = if i == at {
				Cell {
					ch: '猫',
					..template
				}
			} else {
				fence
			};
			assert_eq!(*cell, expected, "put at {at}, cell {i}");
		}
	}
}
