//! The vector views of a cell (`bits`, `words` and the helpers that edit
//! words) against the cell's bytes in memory, which is what they replaced.
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
fn words_are_the_bytes_of_the_cell_in_four_lanes() {
	for cell in cells() {
		let lanes = memory(cell).to_le_bytes();
		let expected: Vec<u32> = lanes
			.chunks(4)
			.map(|lane| u32::from_le_bytes(lane.try_into().unwrap()))
			.collect();
		assert_eq!(cell.words().to_array().as_slice(), expected, "{cell:?}");
		assert_eq!(cell.words().to_array()[0], u32::from(cell.ch));
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
fn words_edited_by_width_and_char_are_the_words_of_the_edited_cell() {
	let mut rng = Rng::new(7);
	for cell in cells() {
		let template = Cell { width: 0, ..cell };
		for width in [0, 1, 2, 255] {
			let edited = Cell::with_width(template.words(), width);
			assert_eq!(
				edited,
				Cell {
					width: width as u8,
					..template
				}
				.words()
			);
		}
		let other = random(&mut rng).ch;
		let blank = Cell::without_char(cell.words());
		assert_eq!(blank.to_array()[0], 0);
		assert_eq!(&blank.to_array()[1..], &cell.words().to_array()[1..]);
		let swapped = Cell::with_char(blank, u32::from(other));
		assert_eq!(
			swapped,
			Cell { ch: other, ..cell }.words(),
			"{cell:?} with {other:?}"
		);
	}
}

#[test]
fn words_stored_through_words_mut_read_back_as_those_cells() {
	let cells = cells();
	let mut grid = vec![Cell::default(); cells.len()];
	// SAFETY: every word stored comes from `Cell::words` of a valid cell.
	let words = unsafe { Cell::words_mut(&mut grid) };
	assert_eq!(words.len(), cells.len());
	for (word, cell) in words.iter_mut().zip(&cells) {
		*word = cell.words();
	}
	assert_eq!(grid, cells);
}

#[test]
fn words_mut_views_exactly_the_cells_it_was_given() {
	let mut grid: Vec<Cell> = (0..8)
		.map(|i| Cell {
			fg: i,
			..Cell::default()
		})
		.collect();
	let marked = Cell {
		ch: 'x',
		width: 2,
		..Cell::default()
	};
	// SAFETY: the word stored comes from `Cell::words` of a valid cell.
	let words = unsafe { Cell::words_mut(&mut grid[2..5]) };
	assert_eq!(words.len(), 3);
	assert_eq!(
		words[0],
		Cell {
			fg: 2,
			..Cell::default()
		}
		.words()
	);
	*words.last_mut().unwrap() = marked.words();
	assert_eq!(grid[4], marked);
	assert_eq!(
		grid[5],
		Cell {
			fg: 5,
			..Cell::default()
		}
	);
	assert_eq!(
		grid[1],
		Cell {
			fg: 1,
			..Cell::default()
		}
	);
	// SAFETY: nothing is stored.
	assert!(unsafe { Cell::words_mut(&mut []) }.is_empty());
}
