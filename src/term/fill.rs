//! Filling runs of cells with AVX2, two cells per store; baseline x86-64
//! code splits each cell into two 8-byte stores.
use std::arch::x86_64::*;

use super::Cell;

/// Callable only where the CPU supports AVX2.
#[target_feature(enable = "avx2")]
pub(super) fn fill(cells: &mut [Cell], cell: Cell) {
	let bits = cell.bits();
	let one = _mm_set_epi64x((bits >> 64) as i64, bits as i64);
	let two = _mm256_broadcastsi128_si256(one);
	let (pairs, rest) = cells.as_chunks_mut::<2>();
	for pair in pairs {
		// SAFETY: two cells are 32 writable bytes.
		unsafe { _mm256_storeu_si256(pair.as_mut_ptr().cast(), two) };
	}
	if let [last] = rest {
		// SAFETY: a cell is 16 writable bytes, aligned to 16.
		unsafe { _mm_store_si128((last as *mut Cell).cast(), one) };
	}
}
