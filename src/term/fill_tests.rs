//! Cell fills against `slice::fill`, at every short length and alignment,
//! through every kernel the CPU can run.
use super::Cell;

type Filler = (&'static str, fn(&mut [Cell], Cell));

fn fillers() -> Vec<Filler> {
	let mut all: Vec<Filler> = vec![("dispatched", Cell::fill), ("pairs", super::fill::pairs)];
	if crate::cpu::V3::detect().is_some() {
		all.push(("v3 pairs", |cells, cell| {
			crate::cpu::run!(
				crate::cpu::V3::detect().unwrap(),
				super::fill::pairs(cells, cell)
			)
		}));
	}
	all
}

/// Every byte differs, so swapped or dropped halves of the cell show.
const STYLED: Cell = Cell {
	ch: 'q',
	fg: 0x0102_0304,
	bg: 0x0506_0708,
	attrs: 9,
	width: 2,
	extra: 0x0a0b,
};

/// A different cell per index, to catch writes past either end.
fn guard(i: usize) -> Cell {
	Cell {
		ch: char::from(b'a' + (i % 26) as u8),
		fg: i as u32,
		..Cell::default()
	}
}

#[test]
fn every_short_length_at_every_alignment_fills_exactly_its_range() {
	for (name, fill) in fillers() {
		// Cells are 16-byte aligned; odd starts misalign the 32-byte stores.
		for start in 0..3 {
			for len in 0..=40 {
				let mut cells: Vec<Cell> = (0..start + len + 3).map(guard).collect();
				let mut expected = cells.clone();
				expected[start..start + len].fill(STYLED);
				fill(&mut cells[start..start + len], STYLED);
				assert_eq!(cells, expected, "{name}, start {start}, len {len}");
			}
		}
	}
}

/// xorshift64: deterministic pseudo-random cells.
fn random_cell(state: &mut u64) -> Cell {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	let bits = *state;
	Cell {
		ch: char::from_u32((bits % 0x11_0000) as u32).unwrap_or('\u{fffd}'),
		fg: bits as u32,
		bg: (bits >> 32) as u32,
		attrs: (bits >> 8) as u8,
		width: (bits % 3) as u8,
		extra: (bits >> 16) as u16,
	}
}

#[test]
fn long_runs_of_random_cells_match_slice_fill() {
	let mut state = 0x9e37_79b9_7f4a_7c15;
	for (name, fill) in fillers() {
		for len in [255, 256, 257, 1000, 4097] {
			let cell = random_cell(&mut state);
			let mut cells: Vec<Cell> = (0..len + 2).map(guard).collect();
			let mut expected = cells.clone();
			expected[1..=len].fill(cell);
			fill(&mut cells[1..=len], cell);
			assert!(cells == expected, "{name}, len {len}, {cell:?}");
		}
	}
}

#[test]
fn refilling_replaces_every_byte_of_the_old_cells() {
	for (name, fill) in fillers() {
		let mut cells = vec![STYLED; 9];
		fill(&mut cells, Cell::default());
		assert!(
			cells
				.iter()
				.all(|cell| cell.bits() == Cell::default().bits()),
			"{name}"
		);
	}
}
