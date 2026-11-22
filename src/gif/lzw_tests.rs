//! GIF's LZW decoding against an encoder written here from the
//! specification: every minimum code size, streams that fill the table
//! and clear it or keep using it full, and broken streams.
use super::lzw::decode_into;

/// [`decode_into`] kept in a vector.
pub(super) fn decode(data: &[u8], min: u8, limit: usize) -> Option<Vec<u8>> {
	let mut out = Vec::new();
	decode_into(data, min, limit, |index| out.push(index))?;
	Some(out)
}
use crate::bench::corpus::Rng;
use std::collections::HashMap;

/// Packs codes least significant bit first, as GIF does.
struct Bits {
	out: Vec<u8>,
	value: u32,
	held: u32,
}

impl Bits {
	fn push(&mut self, code: usize, size: u32) {
		self.value |= (code as u32) << self.held;
		self.held += size;
		while self.held >= 8 {
			self.out.push(self.value as u8);
			(self.value, self.held) = (self.value >> 8, self.held - 8);
		}
	}

	fn finish(mut self) -> Vec<u8> {
		if self.held > 0 {
			self.out.push(self.value as u8);
		}
		self.out
	}
}

/// `indices` encoded with minimum code size `min`. With `clear_when_full`
/// the encoder starts over once 4096 codes are defined; without it, it
/// keeps sending 12-bit codes from the full table, which decoders must
/// accept.
pub(super) fn encode(indices: &[u8], min: u8, clear_when_full: bool) -> Vec<u8> {
	let (clear, end) = (1usize << min, (1usize << min) + 1);
	let mut bits = Bits {
		out: Vec::new(),
		value: 0,
		held: 0,
	};
	let mut size = u32::from(min) + 1;
	let mut table: HashMap<(usize, u8), usize> = HashMap::new();
	let mut next = end + 1;
	bits.push(clear, size);
	let Some((&first, rest)) = indices.split_first() else {
		bits.push(end, size);
		return bits.finish();
	};
	let mut string = usize::from(first);
	for &index in rest {
		if let Some(&code) = table.get(&(string, index)) {
			string = code;
			continue;
		}
		bits.push(string, size);
		if next < 4096 {
			table.insert((string, index), next);
			next += 1;
			if next > 1 << size && size < 12 {
				size += 1;
			}
		} else if clear_when_full {
			bits.push(clear, size);
			(table, next, size) = (HashMap::new(), end + 1, u32::from(min) + 1);
		}
		string = usize::from(index);
	}
	bits.push(string, size);
	bits.push(end, size);
	bits.finish()
}

#[test]
fn random_indices_survive_every_minimum_code_size() {
	let mut rng = Rng::new(17);
	for min in 2..=8u8 {
		for _ in 0..40 {
			let colors = 1usize << min;
			// Few distinct runs compress well and grow long strings.
			let palette = 1 + rng.below(colors);
			let len = rng.below(3000);
			let indices: Vec<u8> = (0..len).map(|_| rng.below(palette) as u8).collect();
			for clear in [true, false] {
				let stream = encode(&indices, min, clear);
				assert_eq!(
					decode(&stream, min, usize::MAX),
					Some(indices.clone()),
					"min {min}, {len}"
				);
			}
		}
	}
}

#[test]
fn streams_past_a_full_table_clear_or_keep_using_it() {
	// Noise defines a code for nearly every index, filling 4096 codes
	// several times over.
	let mut rng = Rng::new(18);
	let indices: Vec<u8> = (0..60_000).map(|_| rng.below(256) as u8).collect();
	for clear in [true, false] {
		let stream = encode(&indices, 8, clear);
		assert_eq!(
			decode(&stream, 8, usize::MAX).as_deref(),
			Some(&indices[..]),
			"clear {clear}"
		);
	}
	// A single color makes ever longer runs: the KwKwK case throughout.
	let same = vec![3u8; 100_000];
	assert_eq!(decode(&encode(&same, 2, true), 2, usize::MAX), Some(same));
}

#[test]
fn output_stops_at_the_limit() {
	let indices: Vec<u8> = (0..500).map(|i| (i % 7) as u8).collect();
	let stream = encode(&indices, 3, true);
	for limit in [0, 1, 2, 3, 99, 499, 500, 501] {
		let decoded = decode(&stream, 3, limit).unwrap();
		assert_eq!(decoded, indices[..limit.min(500)], "limit {limit}");
	}
}

#[test]
fn a_stream_cut_short_keeps_what_it_decoded() {
	let indices: Vec<u8> = (0..300).map(|i| (i * 5 % 4) as u8).collect();
	let stream = encode(&indices, 2, true);
	for cut in 0..stream.len() {
		let decoded = decode(&stream[..cut], 2, usize::MAX).unwrap();
		assert!(indices.starts_with(&decoded), "cut at {cut}");
	}
	assert_eq!(decode(&[], 2, 100), Some(Vec::new()));
}

#[test]
fn codes_not_yet_defined_and_bad_code_sizes_are_refused() {
	// Clear (4), then code 7 when only 6 could be next.
	let mut bits = Bits {
		out: Vec::new(),
		value: 0,
		held: 0,
	};
	bits.push(4, 3);
	bits.push(7, 3);
	assert_eq!(decode(&bits.finish(), 2, 100), None);
	// The first code after a clear must be a root.
	let mut bits = Bits {
		out: Vec::new(),
		value: 0,
		held: 0,
	};
	bits.push(4, 3);
	bits.push(6, 3);
	assert_eq!(decode(&bits.finish(), 2, 100), None);
	for min in [0, 1, 9, 12] {
		assert_eq!(decode(&[0x44], min, 100), None, "minimum code size {min}");
	}
}

#[test]
fn an_end_code_stops_decoding_whatever_follows() {
	let mut stream = encode(&[1, 2, 3], 2, true);
	stream.extend([0xff; 8]);
	assert_eq!(decode(&stream, 2, 100), Some(vec![1, 2, 3]));
	// A stream with only clear and end decodes to nothing.
	assert_eq!(decode(&encode(&[], 4, true), 4, 100), Some(Vec::new()));
}

#[test]
fn reaching_the_limit_ends_decoding_before_what_follows_is_read() {
	// Three good codes, then one never defined: past a limit of three the
	// bad code is never looked at. After the third, codes 6 and 7 are
	// defined, 8 is next, and codes take four bits.
	let mut bits = Bits {
		out: Vec::new(),
		value: 0,
		held: 0,
	};
	for (code, size) in [(4, 3), (1, 3), (2, 3), (3, 3), (9, 4)] {
		bits.push(code, size);
	}
	let stream = bits.finish();
	assert_eq!(decode(&stream, 2, 3), Some(vec![1, 2, 3]));
	assert_eq!(decode(&stream, 2, 4), None, "the bad code is reached");
}

#[test]
fn the_longest_strings_a_full_table_can_hold_decode_whole() {
	// One color, over and over, defines strings one longer each code: the
	// last of 4096 codes stands for over 3800 indices.
	let run = vec![1u8; 3838 * 3839 / 2];
	for clear in [true, false] {
		let stream = encode(&run, 2, clear);
		assert_eq!(
			decode(&stream, 2, usize::MAX).as_deref(),
			Some(&run[..]),
			"clear {clear}"
		);
	}
}
