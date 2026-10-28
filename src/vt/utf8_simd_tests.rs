//! Boundary, truncation and adversarial-input checks for the 16-byte validator.
use crate::vt::utf8::neon::is_valid;

fn agrees(bytes: &[u8]) {
	assert_eq!(
		is_valid(bytes),
		std::str::from_utf8(bytes).is_ok(),
		"{bytes:02x?}"
	);
}

#[test]
fn every_pair_at_every_lane_matches_the_standard_library() {
	for offset in 0..16 {
		let mut data = [b'x'; 48];
		for first in 0..=255u8 {
			for second in 0..=255u8 {
				data[offset] = first;
				data[offset + 1] = second;
				agrees(&data);
			}
		}
	}
}

#[test]
fn valid_scalar_boundaries_cross_every_vector_lane() {
	let scalars = [
		'\0',
		'\u{7f}',
		'\u{80}',
		'\u{7ff}',
		'\u{800}',
		'\u{d7ff}',
		'\u{e000}',
		'\u{ffff}',
		'\u{10000}',
		'\u{10ffff}',
	];
	for offset in 0..48 {
		for c in scalars {
			let mut data = vec![b'a'; offset];
			let mut encoded = [0; 4];
			data.extend_from_slice(c.encode_utf8(&mut encoded).as_bytes());
			data.extend_from_slice(&[b'z'; 33]);
			assert!(is_valid(&data), "{c:?} at {offset}");
		}
	}
}

#[test]
fn truncated_characters_at_every_tail_length_are_rejected() {
	for offset in 0..65 {
		for c in ['é', '猫', '🦀'] {
			let mut encoded = [0; 4];
			let encoded = c.encode_utf8(&mut encoded).as_bytes();
			for cut in 1..encoded.len() {
				let mut data = vec![b' '; offset];
				data.extend_from_slice(&encoded[..cut]);
				assert!(!is_valid(&data), "{c:?} cut {cut} offset {offset}");
				data.extend_from_slice(&encoded[cut..]);
				assert!(is_valid(&data), "{c:?} completed at {offset}");
			}
		}
	}
}

#[test]
fn overlong_surrogate_and_out_of_range_sequences_cross_blocks() {
	let invalid: &[&[u8]] = &[
		&[0xc0, 0x80],
		&[0xc1, 0xbf],
		&[0xe0, 0x80, 0x80],
		&[0xe0, 0x9f, 0xbf],
		&[0xed, 0xa0, 0x80],
		&[0xed, 0xbf, 0xbf],
		&[0xf0, 0x80, 0x80, 0x80],
		&[0xf0, 0x8f, 0xbf, 0xbf],
		&[0xf4, 0x90, 0x80, 0x80],
		&[0xf5, 0x80, 0x80, 0x80],
		&[0xff],
	];
	for offset in 0..48 {
		for bytes in invalid {
			let mut data = vec![b' '; offset];
			data.extend_from_slice(bytes);
			data.extend_from_slice(&[b' '; 32]);
			assert!(!is_valid(&data), "{bytes:02x?} at {offset}");
		}
	}
}

#[test]
fn continuation_obligations_survive_ascii_blocks() {
	for offset in 0..64 {
		for lead in [0xc2, 0xdf, 0xe0, 0xed, 0xef, 0xf0, 0xf4] {
			let mut data = vec![b' '; offset];
			data.push(lead);
			data.extend_from_slice(&[b'a'; 64]);
			assert!(!is_valid(&data), "lead {lead:02x} at {offset}");
		}
		for continuation in [0x80, 0x8f, 0x90, 0x9f, 0xa0, 0xbf] {
			let mut data = vec![b'a'; offset];
			data.push(continuation);
			data.extend_from_slice(&[b'a'; 64]);
			assert!(!is_valid(&data), "stray {continuation:02x} at {offset}");
		}
	}
}

#[test]
fn empty_ascii_and_subslice_alignment_have_no_special_cases() {
	assert!(is_valid(&[]));
	for length in 0..129 {
		let data = vec![b'a'; length + 17];
		for start in 0..17 {
			assert!(is_valid(&data[start..start + length]));
		}
	}
	let data = "é猫🦀 hello ".repeat(64).into_bytes();
	for start in 0..33 {
		for end in start..data.len() {
			agrees(&data[start..end]);
		}
	}
}

#[test]
fn single_byte_mutations_of_mixed_text_match_the_oracle() {
	let text = "abc é Ελληνικά Кириллица 猫 🦀 e\u{301} 👩\u{200d}💻";
	let original = text.repeat(3).into_bytes();
	for index in 0..original.len() {
		let mut data = original.clone();
		for byte in 0..=255u8 {
			data[index] = byte;
			agrees(&data);
		}
	}
}

#[test]
fn deterministic_random_inputs_and_their_prefixes_match_the_oracle() {
	let mut seed = 0x51f0_a983_72c4_0debu64;
	for length in [1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 255, 1024] {
		for _ in 0..64 {
			let mut data = vec![0; length];
			for byte in &mut data {
				seed ^= seed << 13;
				seed ^= seed >> 7;
				seed ^= seed << 17;
				*byte = seed as u8;
			}
			agrees(&data);
			for end in 0..data.len().min(65) {
				agrees(&data[..end]);
			}
		}
	}
}

#[test]
fn errors_are_not_forgotten_when_later_blocks_are_valid() {
	let invalid: &[&[u8]] = &[&[0xff], &[0xc0, 0xaf], &[0xed, 0xa0, 0x80]];
	for bytes in invalid {
		for prefix in [0, 15, 16, 31, 32, 127] {
			let mut data = vec![b'x'; prefix];
			data.extend_from_slice(bytes);
			for suffix in [16, 32, 256, 4096] {
				let mut extended = data.clone();
				extended.extend_from_slice(&vec![b'y'; suffix]);
				assert!(!is_valid(&extended));
				extended.extend_from_slice("é猫🦀".as_bytes());
				assert!(!is_valid(&extended));
			}
		}
	}
}

#[test]
fn invalid_bytes_outside_a_subslice_are_not_examined() {
	for offset in 1..33 {
		for length in 0..65 {
			let mut data = vec![0xff; offset + length + 33];
			data[offset..offset + length].fill(b'a');
			assert!(is_valid(&data[offset..offset + length]));
			assert!(!is_valid(&data[..offset + length]));
			assert!(!is_valid(&data[offset..]));
		}
	}
}

#[test]
fn independent_errors_in_the_same_lane_cannot_cancel() {
	for lane in 0..16 {
		for gap in [16, 32, 48, 64] {
			let mut data = vec![b'a'; lane + gap + 65];
			data[lane] = 0xff;
			data[lane + gap] = 0xff;
			assert!(!is_valid(&data));
			data[lane] = b'a';
			assert!(!is_valid(&data));
			data[lane + gap] = b'a';
			assert!(is_valid(&data));
		}
	}
}

#[test]
fn dispatch_boundaries_preserve_validation_and_truncation() {
	for length in [0, 1, 7, 8, 9, 15, 16, 31, 32, 63, 64, 65, 127, 128, 129] {
		let mut data = vec![b'a'; length];
		assert!(crate::vt::utf8::is_valid(&data));
		if let Some(last) = data.last_mut() {
			*last = 0xc2;
			assert!(!crate::vt::utf8::is_valid(&data));
			data.push(0x80);
			assert!(crate::vt::utf8::is_valid(&data));
		}
	}
}
