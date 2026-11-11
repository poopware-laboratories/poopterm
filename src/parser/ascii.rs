//! ASCII row stores: one whole cell per byte.
use crate::term::Cell;

/// Writes one byte per cell, preserving every other field of `template`.
/// The tokenizer supplies printable ASCII; any byte is a valid `char`.
#[inline(always)]
pub(super) fn write(cells: &mut [Cell], bytes: &[u8], template: Cell) {
	assert_eq!(cells.len(), bytes.len());
	for (cell, &byte) in cells.iter_mut().zip(bytes) {
		Cell::put(cell, template, char::from(byte));
	}
}
