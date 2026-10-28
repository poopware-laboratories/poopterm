//! UTF-8 validation with SIMD table lookups, chosen at runtime.
//!
//! The lookup algorithm of Keiser and Lemire, "Validating UTF-8 In Less Than
//! One Instruction Per Byte" (2021): three 16-entry tables, indexed by the
//! nibbles of each byte and the byte before it, flag every invalid two-byte
//! pattern; a separate check requires the continuation bytes of three- and
//! four-byte characters. `core::simd` lowers its dynamic swizzles to table
//! lookups only when `core` itself was built with SSSE3, so the kernels use
//! `core::arch` directly on x86. Aarch64 uses safe portable SIMD, where
//! dynamic swizzles lower to NEON table lookups.
#![cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]

#[cfg(any(target_arch = "aarch64", test))]
#[path = "utf8_simd.rs"]
pub(super) mod neon;

/// Whether `bytes` is entirely valid UTF-8.
#[inline]
pub(crate) fn is_valid(bytes: &[u8]) -> bool {
	#[cfg(target_arch = "x86_64")]
	if std::arch::is_x86_feature_detected!("avx2") {
		// SAFETY: the CPU supports AVX2.
		return unsafe { x86::is_valid(bytes) };
	}
	#[cfg(target_arch = "aarch64")]
	if bytes.len() >= 8 {
		// Short strings cost less to validate than to set up vector tables.
		return neon::is_valid(bytes);
	}
	portable(bytes)
}

/// The reference every SIMD variant must agree with.
pub(super) fn portable(bytes: &[u8]) -> bool {
	std::str::from_utf8(bytes).is_ok()
}

// Error flags; a byte pair is invalid when all three lookups share one.
const TOO_SHORT: u8 = 1 << 0;
const TOO_LONG: u8 = 1 << 1;
const OVERLONG_3: u8 = 1 << 2;
const TOO_LARGE: u8 = 1 << 3;
const SURROGATE: u8 = 1 << 4;
const OVERLONG_2: u8 = 1 << 5;
/// Shared by two cases that never meet: F0 8x is overlong, F5+ 8x too large.
const TOO_LARGE_1000: u8 = 1 << 6;
const OVERLONG_4: u8 = 1 << 6;
/// Two continuation bytes in a row: an error unless a three- or four-byte
/// character needs them, which the separate check accounts for.
pub(super) const TWO_CONTS: u8 = 1 << 7;
const CARRY: u8 = TOO_SHORT | TOO_LONG | TWO_CONTS;

/// A 16-entry table of a `const fn` of the nibble.
macro_rules! table {
	($flags:ident) => {{
		let mut table = [0; 16];
		let mut nibble = 0;
		while nibble < 16 {
			table[nibble] = $flags(nibble as u8);
			nibble += 1;
		}
		table
	}};
}

/// By the previous byte's high nibble: what it starts or continues.
pub(super) const PREV_HIGH: [u8; 16] = table!(prev_high);
const fn prev_high(nibble: u8) -> u8 {
	match nibble {
		0x0..=0x7 => TOO_LONG,
		0x8..=0xb => TWO_CONTS,
		0xc => TOO_SHORT | OVERLONG_2,
		0xd => TOO_SHORT,
		0xe => TOO_SHORT | OVERLONG_3 | SURROGATE,
		_ => TOO_SHORT | TOO_LARGE | TOO_LARGE_1000 | OVERLONG_4,
	}
}

/// By the previous byte's low nibble, which tells lead bytes apart.
pub(super) const PREV_LOW: [u8; 16] = table!(prev_low);
const fn prev_low(nibble: u8) -> u8 {
	match nibble {
		0x0 => CARRY | OVERLONG_3 | OVERLONG_2 | OVERLONG_4,
		0x1 => CARRY | OVERLONG_2,
		0x2 | 0x3 => CARRY,
		0x4 => CARRY | TOO_LARGE,
		0xd => CARRY | TOO_LARGE | TOO_LARGE_1000 | SURROGATE,
		_ => CARRY | TOO_LARGE | TOO_LARGE_1000,
	}
}

/// By this byte's high nibble: continuation bytes by range, or not one.
pub(super) const HIGH: [u8; 16] = table!(high);
const fn high(nibble: u8) -> u8 {
	match nibble {
		0x8 => TOO_LONG | OVERLONG_2 | TWO_CONTS | OVERLONG_3 | TOO_LARGE_1000 | OVERLONG_4,
		0x9 => TOO_LONG | OVERLONG_2 | TWO_CONTS | OVERLONG_3 | TOO_LARGE,
		0xa | 0xb => TOO_LONG | OVERLONG_2 | TWO_CONTS | SURROGATE | TOO_LARGE,
		_ => TOO_SHORT,
	}
}

/// Subtracted with saturation from a block's last three bytes: nonzero
/// means a character starts there that needs more bytes than remain.
pub(super) const INCOMPLETE: [u8; 32] = {
	let mut max = [0xff; 32];
	max[29] = 0xf0 - 1;
	max[30] = 0xe0 - 1;
	max[31] = 0xc0 - 1;
	max
};

#[cfg(target_arch = "x86_64")]
pub(super) mod x86 {
	use std::arch::x86_64::*;

	use super::{HIGH, INCOMPLETE, PREV_HIGH, PREV_LOW};

	/// A 16-entry table repeated in both 128-bit lanes, for `vpshufb`.
	#[target_feature(enable = "avx2")]
	fn table(entries: &[u8; 16]) -> __m256i {
		// SAFETY: 16 readable bytes.
		let half = unsafe { _mm_loadu_si128(entries.as_ptr().cast()) };
		_mm256_broadcastsi128_si256(half)
	}

	#[target_feature(enable = "avx2")]
	fn high_nibbles(bytes: __m256i) -> __m256i {
		_mm256_and_si256(_mm256_srli_epi16(bytes, 4), _mm256_set1_epi8(0x0f))
	}

	/// Errors in `input`, given the block before it.
	#[target_feature(enable = "avx2")]
	fn errors(input: __m256i, prev: __m256i, tables: &[__m256i; 3]) -> __m256i {
		// The previous block's high half next to this block's low half, so
		// that byte shifts can reach across the 128-bit lane boundary.
		let joined = _mm256_permute2x128_si256(prev, input, 0x21);
		let prev1 = _mm256_alignr_epi8(input, joined, 15);
		let prev2 = _mm256_alignr_epi8(input, joined, 14);
		let prev3 = _mm256_alignr_epi8(input, joined, 13);
		let low = _mm256_set1_epi8(0x0f);
		let special = _mm256_and_si256(
			_mm256_and_si256(
				_mm256_shuffle_epi8(tables[0], high_nibbles(prev1)),
				_mm256_shuffle_epi8(tables[1], _mm256_and_si256(prev1, low)),
			),
			_mm256_shuffle_epi8(tables[2], high_nibbles(input)),
		);
		// The third byte of a three- or four-byte character, or the fourth
		// of a four-byte one: two continuations in a row are expected here.
		let third = _mm256_subs_epu8(prev2, _mm256_set1_epi8((0xe0u8 - 0x80) as i8));
		let fourth = _mm256_subs_epu8(prev3, _mm256_set1_epi8((0xf0u8 - 0x80) as i8));
		let expected = _mm256_and_si256(
			_mm256_or_si256(third, fourth),
			_mm256_set1_epi8(0x80u8 as i8),
		);
		_mm256_xor_si256(expected, special)
	}

	/// # Safety
	///
	/// The CPU must support AVX2.
	#[target_feature(enable = "avx2")]
	pub(in crate::vt) unsafe fn is_valid(bytes: &[u8]) -> bool {
		let tables = [table(&PREV_HIGH), table(&PREV_LOW), table(&HIGH)];
		// SAFETY: 32 readable bytes.
		let incomplete_max = unsafe { _mm256_loadu_si256(INCOMPLETE.as_ptr().cast()) };
		let (chunks, rest) = bytes.as_chunks::<32>();
		let mut tail = [0u8; 32];
		tail[..rest.len()].copy_from_slice(rest);
		let (mut prev, mut error, mut incomplete) = (
			_mm256_setzero_si256(),
			_mm256_setzero_si256(),
			_mm256_setzero_si256(),
		);
		// The zero-padded tail is ASCII after the last byte, so a character
		// cut off at the end shows up as an error in it.
		for chunk in chunks.iter().chain([&tail]) {
			// SAFETY: 32 readable bytes.
			let input = unsafe { _mm256_loadu_si256(chunk.as_ptr().cast()) };
			if _mm256_movemask_epi8(input) == 0 {
				// ASCII: valid unless the previous block left a character open.
				// A nonzero `incomplete` is already an error, so it may stay.
				error = _mm256_or_si256(error, incomplete);
			} else {
				error = _mm256_or_si256(error, errors(input, prev, &tables));
				incomplete = _mm256_subs_epu8(input, incomplete_max);
			}
			prev = input;
		}
		_mm256_testz_si256(error, error) == 1
	}
}
