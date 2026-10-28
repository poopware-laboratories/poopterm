//! Tests for the UTF-8 validator: its lookup tables against a scalar model
//! of the encoding, and every variant against `std::str::from_utf8`.
use super::utf8::{self, HIGH, INCOMPLETE, PREV_HIGH, PREV_LOW, TWO_CONTS};
use crate::bench::corpus::Rng;

type Validator = (&'static str, fn(&[u8]) -> bool);

/// Every validator compiled for this target that this CPU can run.
fn validators() -> Vec<Validator> {
	let all: Vec<Validator> = vec![("dispatched", utf8::is_valid), ("portable", utf8::portable)];
	#[cfg(target_arch = "x86_64")]
	if std::arch::is_x86_feature_detected!("avx2") {
		// SAFETY: the CPU supports AVX2.
		let avx2: Validator = ("avx2", |bytes| unsafe { utf8::x86::is_valid(bytes) });
		return [all, vec![avx2]].concat();
	}
	all
}

#[test]
fn simd_variants_are_tested_where_the_cpu_has_them() {
	let names: Vec<_> = validators().iter().map(|(name, _)| *name).collect();
	#[cfg(target_arch = "x86_64")]
	assert_eq!(
		names.contains(&"avx2"),
		std::arch::is_x86_feature_detected!("avx2")
	);
	assert!(names.starts_with(&["dispatched", "portable"]));
}

fn is_continuation(byte: u8) -> bool {
	byte & 0xc0 == 0x80
}

/// The continuation bytes allowed right after `lead`, from the encoding's
/// table of well-formed sequences (Unicode 15, table 3-7); None if `lead`
/// starts no multi-byte character.
fn second_bytes(lead: u8) -> Option<std::ops::RangeInclusive<u8>> {
	Some(match lead {
		0xc2..=0xdf | 0xe1..=0xec | 0xee..=0xef | 0xf1..=0xf3 => 0x80..=0xbf,
		0xe0 => 0xa0..=0xbf,
		0xed => 0x80..=0x9f,
		0xf0 => 0x90..=0xbf,
		0xf4 => 0x80..=0x8f,
		_ => return None,
	})
}

/// Whether `second` may follow `first`, judged by these two bytes alone.
fn pair_is_valid(first: u8, second: u8) -> bool {
	match second_bytes(first) {
		Some(allowed) => allowed.contains(&second),
		// After ASCII or a continuation, a new character must start.
		None if first < 0x80 || is_continuation(first) => !is_continuation(second),
		// C0, C1 and F5..FF never start a character.
		None => false,
	}
}

#[test]
fn the_lookup_tables_flag_exactly_the_invalid_byte_pairs() {
	for first in 0..=255u8 {
		for second in 0..=255u8 {
			let flags = PREV_HIGH[usize::from(first >> 4)]
				& PREV_LOW[usize::from(first & 0x0f)]
				& HIGH[usize::from(second >> 4)];
			if is_continuation(first) && is_continuation(second) {
				// Left to the check of three- and four-byte characters.
				assert_eq!(flags, TWO_CONTS, "{first:02x} {second:02x}");
			} else {
				let valid = pair_is_valid(first, second);
				assert_eq!(
					flags == 0,
					valid,
					"{first:02x} {second:02x}: flags {flags:08b}"
				);
			}
		}
	}
}

#[test]
fn only_lead_bytes_too_close_to_a_block_end_count_as_incomplete() {
	for byte in 0..=255u8 {
		for (position, needs) in [(29, 4), (30, 3), (31, 2)] {
			let length = match byte {
				0xf0.. => 4,
				0xe0.. => 3,
				0xc0.. => 2,
				_ => 1,
			};
			let open = byte.saturating_sub(INCOMPLETE[position]) != 0;
			assert_eq!(open, length >= needs, "{byte:02x} at {position}");
		}
		for max in &INCOMPLETE[..29] {
			assert_eq!(byte.saturating_sub(*max), 0);
		}
	}
}

fn agree(bytes: &[u8]) {
	let expected = std::str::from_utf8(bytes).is_ok();
	for (name, valid) in validators() {
		assert_eq!(valid(bytes), expected, "{name} on {bytes:x?}");
	}
}

/// Byte values on either side of every boundary the encoding cares about.
const EDGES: [u8; 14] = [
	0x00, 0x7f, 0x80, 0x8f, 0x90, 0x9f, 0xa0, 0xbf, 0xc0, 0xc2, 0xe0, 0xed, 0xf4, 0xff,
];

#[test]
fn every_two_byte_pair_and_edge_triple_agrees_at_every_lane_position() {
	// 61 bytes put a sequence across the 16-, 32- and 64-byte block edges.
	let mut buffer = [b'a'; 61];
	let validators = validators();
	for first in 0..=255u8 {
		for second in 0..=255u8 {
			for &third in &EDGES {
				let sequence = [first, second, third];
				let expected = std::str::from_utf8(&sequence).is_ok();
				for at in [0, 13, 14, 15, 29, 30, 31, 58] {
					buffer.fill(b'a');
					buffer[at..at + 3].copy_from_slice(&sequence);
					let expected = expected && std::str::from_utf8(&buffer).is_ok();
					for (name, valid) in &validators {
						assert_eq!(valid(&buffer), expected, "{name}: {sequence:x?} at {at}");
					}
				}
			}
		}
	}
}

#[test]
fn every_four_byte_edge_sequence_agrees() {
	for first in 0xe0..=0xff {
		for second in 0..=255 {
			for third in EDGES {
				for fourth in EDGES {
					agree(&[b'x', first, second, third, fourth, b'y']);
				}
			}
		}
	}
}

#[test]
fn truncated_characters_at_the_end_are_invalid() {
	for text in ["é", "€", "😀"] {
		let mut bytes = "a".repeat(40).into_bytes();
		bytes.extend_from_slice(text.as_bytes());
		for cut in 0..bytes.len() {
			// Cuts at every length, so the tail lands in every lane.
			agree(&bytes[..cut]);
			agree(&bytes[cut..]);
		}
	}
}

#[test]
fn the_largest_scalar_and_surrogates_are_told_apart() {
	agree("\u{10ffff}".as_bytes());
	agree(&[0xf4, 0x90, 0x80, 0x80]);
	agree("\u{d7ff}\u{e000}".as_bytes());
	agree(&[0xed, 0xa0, 0x80]);
	agree(&[0xc0, 0x80]);
	agree(&[0xe0, 0x9f, 0xbf]);
	agree(&[0xf0, 0x8f, 0xbf, 0xbf]);
	agree(&[]);
}

#[test]
fn random_mixes_of_text_and_noise_agree() {
	let mut rng = Rng::new(7);
	let pieces = [
		"a",
		"é",
		"€",
		"😀",
		"日本",
		"\u{10ffff}",
		"\u{80}",
		" ",
		"\u{7f}",
	];
	for _ in 0..20_000 {
		let mut bytes = Vec::new();
		for _ in 0..rng.below(80) {
			if rng.below(40) == 0 {
				bytes.push(rng.next() as u8);
			} else {
				bytes.extend_from_slice(rng.pick(&pieces).as_bytes());
			}
		}
		agree(&bytes);
	}
}

#[test]
fn long_valid_text_is_valid_across_many_blocks() {
	let text = "Größe ελληνικά кириллица 日本語 😀 ".repeat(40);
	agree(text.as_bytes());
	let mut bytes = text.into_bytes();
	let last = bytes.len() - 1;
	bytes[last] = 0xe2;
	agree(&bytes);
}

/// `bytes` placed after `shift` ASCII bytes, so that it starts in every lane.
fn shifted(bytes: &[u8], shift: usize) -> Vec<u8> {
	let mut out = vec![b'.'; shift];
	out.extend_from_slice(bytes);
	out
}

#[test]
fn every_scalar_value_is_valid_at_every_alignment() {
	let text: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
	for shift in 0..4 {
		for (name, valid) in validators() {
			assert!(
				valid(&shifted(text.as_bytes(), shift)),
				"{name}, shift {shift}"
			);
		}
	}
}

#[test]
fn every_surrogate_is_invalid_at_every_block_offset() {
	let validators = validators();
	for surrogate in 0xd800u32..=0xdfff {
		let encoded = [
			0xe0 | (surrogate >> 12) as u8,
			0x80 | (surrogate >> 6 & 0x3f) as u8,
			0x80 | (surrogate & 0x3f) as u8,
		];
		for shift in [0, 15, 29, 30, 31, 32] {
			let bytes = shifted(&encoded, shift);
			for (name, valid) in &validators {
				assert!(!valid(&bytes), "{name}: U+{surrogate:04X} after {shift}");
			}
		}
	}
}

#[test]
fn overlong_encodings_are_invalid_in_every_length() {
	let validators = validators();
	// Every scalar below each length's minimum, encoded in that length.
	for value in 0u32..0x10000 {
		let mut forms = vec![
			[
				0xf0,
				0x80 | (value >> 12) as u8,
				0x80 | (value >> 6 & 0x3f) as u8,
				0x80 | (value & 0x3f) as u8,
			]
			.to_vec(),
		];
		if value < 0x800 {
			forms.push(vec![
				0xe0,
				0x80 | (value >> 6) as u8,
				0x80 | (value & 0x3f) as u8,
			]);
		}
		if value < 0x80 {
			forms.push(vec![0xc0 | (value >> 6) as u8, 0x80 | (value & 0x3f) as u8]);
		}
		for form in forms {
			let bytes = shifted(&form, (value % 33) as usize);
			for (name, valid) in &validators {
				assert!(!valid(&bytes), "{name}: {form:x?}");
			}
		}
	}
}

#[test]
fn values_past_the_largest_scalar_are_invalid() {
	let validators = validators();
	// The four-byte pattern holds 21 bits; everything above U+10FFFF is out.
	for value in (0x11_0000u32..0x20_0000).step_by(0x3f1) {
		let encoded = [
			0xf0 | (value >> 18) as u8,
			0x80 | (value >> 12 & 0x3f) as u8,
			0x80 | (value >> 6 & 0x3f) as u8,
			0x80 | (value & 0x3f) as u8,
		];
		for shift in [0, 28, 29, 30, 31] {
			let bytes = shifted(&encoded, shift);
			for (name, valid) in &validators {
				assert!(!valid(&bytes), "{name}: {value:x} after {shift}");
			}
		}
	}
}

#[test]
fn noncharacters_and_private_use_are_valid() {
	for text in [
		"\u{fffe}\u{ffff}",
		"\u{fdd0}\u{fdef}",
		"\u{e000}\u{f8ff}",
		"\u{10fffe}\u{f0000}",
	] {
		for shift in 0..33 {
			agree(&shifted(text.as_bytes(), shift));
		}
	}
}

#[test]
fn a_character_left_open_before_an_ascii_block_is_invalid() {
	for text in ["é", "€", "😀"] {
		let bytes = text.as_bytes();
		for cut in 1..bytes.len() {
			// The cut character ends block one; block two is all ASCII.
			let mut stream = vec![b'.'; 32 - cut];
			stream.extend_from_slice(&bytes[..cut]);
			stream.extend_from_slice(&[b'.'; 32]);
			agree(&stream);
			// Further non-ASCII blocks must not wash the error out.
			stream.extend_from_slice("日本語".repeat(20).as_bytes());
			agree(&stream);
		}
		// Whole, the same character before an ASCII block is fine.
		let mut stream = vec![b'.'; 32 - bytes.len()];
		stream.extend_from_slice(bytes);
		stream.extend_from_slice(&[b'.'; 64]);
		agree(&stream);
	}
}

#[test]
fn an_early_error_survives_many_ascii_blocks() {
	for defect in [0x80u8, 0xc0, 0xff, 0xed] {
		let mut stream = vec![defect];
		stream.extend_from_slice(&[b'a'; 32 * 8]);
		agree(&stream);
		stream.extend_from_slice("ελληνικά".as_bytes());
		agree(&stream);
	}
}

#[test]
fn defects_inside_non_ascii_text_agree_at_every_position() {
	// No block of this text is ASCII, so every check sees non-ASCII neighbors.
	let text = "Größe日本語😀кириллица€".repeat(4).into_bytes();
	let validators = validators();
	for at in 0..text.len() {
		for defect in EDGES {
			let mut bytes = text.clone();
			bytes[at] = defect;
			let expected = std::str::from_utf8(&bytes).is_ok();
			for (name, valid) in &validators {
				assert_eq!(valid(&bytes), expected, "{name}: {defect:02x} at {at}");
			}
		}
	}
}

#[path = "utf8_simd_tests.rs"]
mod simd_tests;
