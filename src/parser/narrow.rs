//! Prints runs of narrow characters of up to three bytes (ASCII, Latin, Greek,
//! Cyrillic, punctuation, box drawing) sixteen bytes at a time, without
//! decoding them one by one.
use std::simd::{
	Mask, Select, Simd,
	cmp::{SimdPartialEq, SimdPartialOrd},
	num::SimdUint,
	simd_swizzle,
};

use std::simd::u32x4;

use crate::term::Cell;

/// Bytes examined per step.
pub(super) const CHUNK: usize = 16;
/// Narrow characters in a row before the fast path is tried.
pub(super) const RUN: usize = 4;
/// The first lead byte of characters from U+3000 on.
const WIDE_LEAD: u8 = 0xe3;

/// One bit per 32 characters, set when all of them are one cell wide or C1
/// controls (which never reach here): characters in those blocks need no
/// width lookup. Padded to cover every `u16`.
const NARROW_BLOCKS: [u64; 32] = {
	let mut words = [0; 32];
	let below_3000 = [
		0x5832_8fef_f0ff_ffd6,
		0x8499_a000_8000_0028,
		0x90ff_ffff_fbff_00e1,
		0xffff_3f3d_4012_fdce,
		0xc800_7fff_7cff_ff34,
		0x000f_7f7f_faff_ffff,
	];
	let mut i = 0;
	while i < below_3000.len() {
		words[i] = below_3000[i];
		i += 1;
	}
	words
};

/// Whether `c` is in one of [`NARROW_BLOCKS`].
#[inline(always)]
fn in_narrow_block(c: u16) -> bool {
	NARROW_BLOCKS[usize::from(c >> 11)] >> (c >> 5 & 63) & 1 != 0
}

/// Writes the characters in `bytes`, valid UTF-8 starting at a character,
/// to the start of `row`, one cell each like `cell` (a narrow cell from
/// [`Cell::words`] without its character), when all of
/// them are one cell wide and below U+10000. A character cut by the end of
/// the chunk is left for the next call. Returns the bytes and cells used and
/// the last character, or `None` (writing nothing) when a character needs
/// the general path or the characters do not fit.
#[inline(always)]
pub(super) fn print_chunk(
	bytes: &[u8; CHUNK],
	row: &mut [u32x4],
	cell: u32x4,
) -> Option<(usize, usize, char)> {
	let b = Simd::from_array(*bytes);
	// Lead bytes from `WIDE_LEAD` start U+3000 and up: CJK, Hangul and emoji, which
	// are wide, and the rest of the planes, which this path does not decode.
	// Controls and DEL never reach here, but cost nothing to rule out.
	let control = b.simd_lt(Simd::splat(0x20)) | b.simd_eq(Simd::splat(0x7f));
	if (b.simd_ge(Simd::splat(WIDE_LEAD)) | control).any() {
		return None;
	}
	// The bytes one and two before each lane; those before the chunk, which
	// starts at a character, count as ASCII.
	let prev = simd_swizzle!(b, [0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
	let prev = Mask::<i8, CHUNK>::from_bitmask(!1).select(prev, Simd::splat(0));
	let prev2 = simd_swizzle!(prev, [0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
	let at_least = |v: Simd<u8, CHUNK>, lo: u8| v.simd_ge(Simd::splat(lo));
	let lead = at_least(b, 0xc0);
	let lead3 = at_least(b, 0xe0);
	// Lanes after a three-byte lead decode as two-byte too, but never end a
	// character, so their cells are overwritten.
	let after_lead = at_least(prev, 0xc0);
	let after_lead3 = at_least(prev, 0xe0);
	let ends_lead3 = at_least(prev2, 0xe0);
	let len = if lead.test(CHUNK - 1) {
		CHUNK - 1
	} else if lead3.test(CHUNK - 2) {
		CHUNK - 2
	} else {
		CHUNK
	};
	let ends = (!(lead | after_lead3)).to_bitmask() & ((1 << len) - 1);
	let cells = ends.count_ones() as usize;
	if cells > row.len() {
		return None;
	}
	let wide = |v: Simd<u8, CHUNK>| v.cast::<u16>();
	let low = wide(b & Simd::splat(0x3f));
	let two = wide(prev & Simd::splat(0x1f)) << 6 | low;
	let three = wide(prev2 & Simd::splat(0x0f)) << 12 | wide(prev & Simd::splat(0x3f)) << 6 | low;
	let chars = ends_lead3
		.cast::<i16>()
		.select(three, after_lead.cast::<i16>().select(two, wide(b)))
		.to_array();
	let mut multibyte = ends & at_least(b, 0x80).to_bitmask();
	while multibyte != 0 {
		let c = chars[multibyte.trailing_zeros() as usize];
		if !in_narrow_block(c)
			// SAFETY: lanes ending a character hold it, and valid UTF-8
			// never encodes a surrogate.
			&& crate::width::width(unsafe { char::from_u32_unchecked(c.into()) }) != 1
		{
			return None;
		}
		multibyte &= multibyte - 1;
	}
	let row = &mut row[..cells];
	let (mut x, mut ends) = (0, ends);
	while ends != 0 {
		row[x] = Cell::with_char(cell, chars[ends.trailing_zeros() as usize] as u32);
		(x, ends) = (x + 1, ends & (ends - 1));
	}
	debug_assert_eq!(x, cells);
	// SAFETY: lanes ending a character hold it, and valid UTF-8 never
	// encodes a surrogate.
	Some((len, cells, unsafe {
		char::from_u32_unchecked(row[cells - 1][0])
	}))
}

#[cfg(test)]
#[path = "narrow_tests.rs"]
mod tests;
