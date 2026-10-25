//! Tests of glyph records: the words a shader reads hold exactly the bands,
//! indices and curves of the glyph, at the offsets the header implies.
use super::{
	bands::{Banded, MAX_CURVES},
	record::encode,
};
use crate::fonts::{BUNDLED, Font, quads::Quad};

fn quad(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2]) -> Quad {
	Quad { p0, p1, p2 }
}

/// Reads a record back the way a shader would, checking it holds nothing
/// past its end.
fn decode(words: &[u32]) -> Banded {
	let (bands, count) = ((words[0] & 0xffff) as usize, (words[0] >> 16) as usize);
	let y_range = (f32::from_bits(words[1]), f32::from_bits(words[2]));
	let starts = words[3..4 + bands].to_vec();
	let listed = *starts.last().unwrap() as usize;
	let at = 4 + bands;
	let pairs = &words[at..at + listed.div_ceil(2)];
	let indices = (0..listed)
		.map(|i| (pairs[i / 2] >> (16 * (i % 2))) as u16)
		.collect();
	let at = at + listed.div_ceil(2);
	let point = |i: usize| [f32::from_bits(words[i]), f32::from_bits(words[i + 1])];
	let curves = (0..count)
		.map(|c| {
			quad(
				point(at + 6 * c),
				point(at + 6 * c + 2),
				point(at + 6 * c + 4),
			)
		})
		.collect();
	assert_eq!(words.len(), at + 6 * count, "words past the record");
	Banded {
		curves,
		y_range,
		starts,
		indices,
	}
}

#[test]
fn a_triangle_is_laid_out_word_by_word() {
	let curves = vec![
		quad([0.0, 0.0], [1.0, 2.0], [2.0, 4.0]),
		quad([2.0, 4.0], [3.0, 2.0], [4.0, 0.0]),
		quad([4.0, 0.0], [2.0, 0.0], [0.0, 0.0]),
	];
	let words = encode(&Banded::new(curves).unwrap());
	let f = f32::to_bits;
	#[rustfmt::skip]
	let expected = [
		// One band, three curves; y from 0 to 4.
		1 | 3 << 16, f(0.0), f(4.0),
		// The band lists curves 0 and 1: the flat base crosses no ray.
		0, 2,
		// Indices 0 and 1, two to a word, the first in the low half.
		1 << 16,
		f(0.0), f(0.0), f(1.0), f(2.0), f(2.0), f(4.0),
		f(2.0), f(4.0), f(3.0), f(2.0), f(4.0), f(0.0),
		f(4.0), f(0.0), f(2.0), f(0.0), f(0.0), f(0.0),
	];
	assert_eq!(words, expected);
}

#[test]
fn an_odd_index_count_leaves_the_last_high_half_zero() {
	// A square's sides 1 and 3 cross rays; a lone slanted line makes three.
	let mut curves = vec![quad([9.0, 0.0], [9.5, 5.0], [10.0, 10.0])];
	curves.extend([
		quad([0.0, 0.0], [2.0, 0.0], [4.0, 0.0]),
		quad([4.0, 0.0], [4.0, 5.0], [4.0, 10.0]),
		quad([4.0, 10.0], [2.0, 10.0], [0.0, 10.0]),
		quad([0.0, 10.0], [0.0, 5.0], [0.0, 0.0]),
	]);
	let banded = Banded::new(curves).unwrap();
	assert_eq!(banded.indices, [0, 2, 4]);
	let words = encode(&banded);
	assert_eq!(&words[5..7], [2 << 16, 4]);
	assert_eq!(decode(&words), banded);
}

#[test]
fn bundled_glyphs_read_back_exactly() {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	for ch in ('!'..='~').chain("@&%§éßøΩЖ→─│┼█▀🐱".chars()) {
		let curves = font.quads(font.lookup_glyph_index(ch));
		let Some(banded) = Banded::new(curves) else {
			continue;
		};
		let words = encode(&banded);
		assert_eq!(decode(&words), banded, "{ch}");
	}
}

#[test]
fn the_largest_glyph_keeps_its_counts_and_indices() {
	// Curve 4095 and a count of 4096 both need more than twelve bits.
	let line = quad([0.0, 0.0], [0.0, 1.0], [0.0, 2.0]);
	let mut curves = vec![line; MAX_CURVES - 1];
	curves.push(quad([1.0, 0.0], [1.0, 9.0], [1.0, 18.0]));
	let banded = Banded::new(curves).unwrap();
	let words = encode(&banded);
	assert_eq!(words[0] >> 16, MAX_CURVES as u32);
	let decoded = decode(&words);
	assert!(decoded.indices.contains(&(MAX_CURVES as u16 - 1)));
	assert_eq!(decoded, banded);
}

#[test]
fn band_starts_beyond_sixteen_bits_survive() {
	// Lines spanning the whole glyph sit in all sixteen bands: 65536 indices,
	// one past what 16 bits can count.
	let line = quad([0.0, 0.0], [0.0, 1.0], [0.0, 2.0]);
	let banded = Banded::new(vec![line; MAX_CURVES]).unwrap();
	assert_eq!(banded.indices.len(), 1 << 16);
	let words = encode(&banded);
	assert_eq!(words[3 + 16], 1 << 16);
	assert_eq!(decode(&words), banded);
}
