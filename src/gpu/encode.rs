//! A quad as the GPU's shaders read it: twelve words of its rectangle,
//! its clip, where its pixels come from, its kind and its color, the same
//! for every backend.
use super::{
	atlas::Slot,
	quads::{Fill, Quad},
};

/// Quad kinds, as the shaders know them.
const SOLID: u32 = 0;
const COVERAGE: u32 = 1;
const COLOR: u32 = 2;
const IMAGE: u32 = 3;
/// Words a quad.
pub(crate) const QUAD: usize = 12;

/// `quad` as the shaders read it: its rectangle, its clip, and its atlas
/// position and page, or its image's source corner and size, then its kind
/// and color; `None` for an image source past what 16 bits hold.
pub(crate) fn encode(quad: &Quad) -> Option<[u32; QUAD]> {
	let (kind, from) = match quad.fill {
		Fill::Solid => (SOLID, [0, 0]),
		Fill::Coverage(slot) => (COVERAGE | u32::from(quad.italic) << 8, place(slot)),
		Fill::Color(slot) => (COLOR, place(slot)),
		Fill::Image(_, source) => {
			let [x, y, width, height] =
				[source.x, source.y, source.width, source.height].map(u16::try_from);
			let pair = |low: u16, high: u16| u32::from(low) | u32::from(high) << 16;
			(
				IMAGE,
				[pair(x.ok()?, y.ok()?), pair(width.ok()?, height.ok()?)],
			)
		}
	};
	let (x, y) = quad.at;
	let (left, top, right, bottom) = quad.clip;
	let place = [
		x,
		y,
		quad.size.0 as i32,
		quad.size.1 as i32,
		left,
		top,
		right,
		bottom,
	];
	let mut words = [0; QUAD];
	for (word, value) in words.iter_mut().zip(place) {
		*word = value as u32;
	}
	words[8..].copy_from_slice(&[from[0], from[1], kind, quad.color]);
	Some(words)
}

/// A slot's atlas position x | y << 16 and its page.
fn place(slot: Slot) -> [u32; 2] {
	[(slot.x | slot.y << 16) as u32, slot.page as u32]
}

#[cfg(test)]
#[path = "encode_tests.rs"]
mod tests;
