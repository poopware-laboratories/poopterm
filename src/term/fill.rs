//! Filling runs of cells with AVX2, two cells per store; baseline x86-64
//! code splits each cell into two 8-byte stores.
//!
//! Safe code: compiled with AVX2, a fill of cell pairs becomes one 32-byte
//! store per pair, the same instructions the intrinsics gave.
use super::Cell;

/// Callable only where the CPU supports AVX2.
#[target_feature(enable = "avx2")]
pub(super) fn fill(cells: &mut [Cell], cell: Cell) {
	let (pairs, rest) = cells.as_chunks_mut::<2>();
	pairs.fill([cell; 2]);
	rest.fill(cell);
}
