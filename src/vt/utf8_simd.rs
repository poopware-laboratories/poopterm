//! Safe 16-byte UTF-8 lookup validator; aarch64 lowers dynamic swizzles to NEON tables.
use std::simd::{Simd, cmp::SimdPartialEq, num::SimdUint, simd_swizzle};

use super::{HIGH, PREV_HIGH, PREV_LOW};

#[inline]
pub(in crate::vt) fn is_valid(bytes: &[u8]) -> bool {
	let high = Simd::from_array(HIGH);
	let prev_high = Simd::from_array(PREV_HIGH);
	let prev_low = Simd::from_array(PREV_LOW);
	let low = Simd::splat(0x0f);
	let (chunks, rest) = bytes.as_chunks::<16>();
	let mut tail = [0; 16];
	tail[..rest.len()].copy_from_slice(rest);
	let mut prev = Simd::splat(0);
	let mut error = Simd::splat(0);
	// The final zero-padded block rejects an unfinished character, including
	// when the input ends exactly on a vector boundary.
	for chunk in chunks.iter().chain([&tail]) {
		let input = Simd::from_array(*chunk);
		let prev1 = simd_swizzle!(
			prev,
			input,
			[
				15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30
			]
		);
		let prev2 = simd_swizzle!(
			prev,
			input,
			[
				14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29
			]
		);
		let prev3 = simd_swizzle!(
			prev,
			input,
			[
				13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28
			]
		);
		let special = prev_high.swizzle_dyn(prev1 >> 4)
			& prev_low.swizzle_dyn(prev1 & low)
			& high.swizzle_dyn(input >> 4);
		// Third and fourth bytes require two consecutive continuations.
		let third = prev2.saturating_sub(Simd::splat(0x60));
		let fourth = prev3.saturating_sub(Simd::splat(0x70));
		let expected = (third | fourth) & Simd::splat(0x80);
		error |= special ^ expected;
		prev = input;
	}
	error.simd_eq(Simd::splat(0)).all()
}
