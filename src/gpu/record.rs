//! A banded glyph as the words a coverage shader reads from its buffer: a
//! header, the bands, then the curves.
use super::bands::Banded;

/// The record of `banded`, in order:
///
/// - the band count, with the curve count in the high 16 bits;
/// - the bottom and top of the bands, as `f32` bits;
/// - one more band start than there are bands, into the indices;
/// - the indices, two to a word, the first in the low half;
/// - each curve's start, control and end points, as `f32` bits.
pub(crate) fn encode(banded: &Banded) -> Vec<u32> {
	let bands = banded.starts.len() - 1;
	let mut words = vec![bands as u32 | (banded.curves.len() as u32) << 16];
	words.extend([banded.y_range.0.to_bits(), banded.y_range.1.to_bits()]);
	words.extend(&banded.starts);
	let pairs = banded.indices.chunks(2);
	words.extend(pairs.map(|pair| u32::from(pair[0]) | u32::from(*pair.get(1).unwrap_or(&0)) << 16));
	for q in &banded.curves {
		words.extend([q.p0, q.p1, q.p2].iter().flatten().map(|v| v.to_bits()));
	}
	words
}
