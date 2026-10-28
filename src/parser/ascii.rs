//! ASCII row stores using the existing whole-cell vector representation.
use crate::term::Cell;

/// Writes one byte per cell, preserving every other field of `template`.
/// Each byte is a valid Unicode scalar; the tokenizer supplies printable ASCII.
#[inline(always)]
pub(super) fn write(cells: &mut [Cell], bytes: &[u8], template: Cell) {
	assert_eq!(cells.len(), bytes.len());
	let template = Cell::without_char(template.words());
	// SAFETY: every stored lane 0 comes from a u8, hence is a valid char;
	// the other lanes preserve a whole Cell's initialized fields.
	let words = unsafe { Cell::words_mut(cells) };
	for (cell, &byte) in words.iter_mut().zip(bytes) {
		*cell = Cell::with_char(template, u32::from(byte));
	}
}
