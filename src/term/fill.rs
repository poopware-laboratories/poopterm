//! Filling runs of cells two at a time. Run under [`crate::cpu::V3::run`],
//! a fill of cell pairs becomes one 32-byte store per pair; baseline
//! x86-64 code splits each cell into two 8-byte stores.
use super::Cell;

/// Sets every cell of `cells` to `cell`, a pair of cells per store.
#[inline(always)]
pub(super) fn pairs(cells: &mut [Cell], cell: Cell) {
	let (pairs, rest) = cells.as_chunks_mut::<2>();
	pairs.fill([cell; 2]);
	rest.fill(cell);
}
