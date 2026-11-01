//! Tests of the image textures against simple models: runs checked on
//! every order of up to seven quads needing one of three textures or none,
//! and the cache against a plain least recently used list over random
//! frames of drawing, dropping and removing bitmaps.
use super::textures::{Textures, runs, texture_bytes};
use crate::graphics::Bitmap;
use std::sync::Arc;

/// Every sequence of `length` needs over none and three textures.
fn every_order(length: u32) -> impl Iterator<Item = Vec<Option<u8>>> {
	(0..4u32.pow(length)).map(move |mut n| {
		(0..length)
			.map(|_| {
				let need = (n % 4) as u8;
				n /= 4;
				need.checked_sub(1)
			})
			.collect()
	})
}

#[test]
fn runs_cover_every_quad_in_order_in_as_few_runs_as_textures_allow() {
	for length in 0..=7 {
		for needs in every_order(length) {
			let found = runs(needs.iter().copied());
			// Contiguous from the first quad to the last.
			let mut next = 0;
			for (range, _) in &found {
				assert_eq!(range.start, next, "{needs:?}: {found:?}");
				assert!(range.end > range.start, "{needs:?}: {found:?}");
				next = range.end;
			}
			assert_eq!(next, length, "{needs:?}: {found:?}");
			// Each quad drawn with the texture it needs bound.
			for (range, bound) in &found {
				for need in &needs[range.start as usize..range.end as usize] {
					assert!(need.is_none() || need == bound, "{needs:?}: {found:?}");
				}
				// Bound only where a quad needs it.
				let needed = needs[range.start as usize..range.end as usize].contains(bound);
				assert!(bound.is_none() || needed, "{needs:?}: {found:?}");
			}
			// A new run only where the texture needed changes.
			let mut textures = needs.iter().flatten();
			let mut changes = textures.next().map_or(0, |_| 1);
			let mut last = needs.iter().flatten().next();
			for need in textures {
				changes += usize::from(Some(need) != last);
				last = Some(need);
			}
			assert_eq!(
				found.len(),
				changes.max(usize::from(length > 0)),
				"{needs:?}"
			);
		}
	}
}

/// A tiny random number generator: xorshift, from a fixed seed.
struct Random(u64);

impl Random {
	fn below(&mut self, n: usize) -> usize {
		self.0 ^= self.0 << 13;
		self.0 ^= self.0 >> 7;
		self.0 ^= self.0 << 17;
		(self.0 % n as u64) as usize
	}
}

/// The cache as a list, least recently used first: what each texture
/// takes, and its bitmap.
struct Model {
	kept: Vec<(usize, usize)>,
	budget: usize,
}

impl Model {
	fn bytes(&self) -> usize {
		self.kept.iter().map(|&(_, bytes)| bytes).sum()
	}

	/// Draws bitmap `id` of `bytes` as the only one this frame: a hit
	/// moves it last; a miss evicts from the front until it fits, or is
	/// refused, evicting nothing, if it cannot.
	fn draw(&mut self, id: usize, bytes: usize) -> bool {
		if let Some(at) = self.kept.iter().position(|&(kept, _)| kept == id) {
			let hit = self.kept.remove(at);
			self.kept.push(hit);
			return true;
		}
		if bytes > self.budget {
			return false;
		}
		while self.bytes() + bytes > self.budget {
			self.kept.remove(0);
		}
		self.kept.push((id, bytes));
		true
	}
}

#[test]
fn the_cache_keeps_what_a_least_recently_used_list_keeps() {
	for seed in 1..=20u64 {
		let mut random = Random(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
		let budget = 4 * (8 + random.below(40));
		let mut textures = Textures::new(budget);
		let mut model = Model {
			kept: Vec::new(),
			budget,
		};
		// Eight bitmaps of up to 4×4, each replaced by a new one when
		// dropped.
		let new = |random: &mut Random| {
			let (width, height) = (1 + random.below(4), 1 + random.below(4));
			Arc::new(Bitmap::new(width, height, vec![0; width * height]))
		};
		let mut bitmaps: Vec<_> = (0..8).map(|_| new(&mut random)).collect();
		let mut ids: Vec<usize> = (0..8).collect();
		let mut fresh = 8;
		for step in 0..300 {
			let at = random.below(8);
			match random.below(5) {
				// Dropped: gone from both once the next frame starts.
				0 => {
					let held = textures.len();
					bitmaps[at] = new(&mut random);
					assert_eq!(textures.len(), held, "held until the frame starts");
					textures.begin_frame();
					model.kept.retain(|&(id, _)| id != ids[at]);
					ids[at] = fresh;
					fresh += 1;
				}
				// Forgotten, as a failed frame forgets its new textures.
				1 => {
					model.kept.retain(|&(id, _)| id != ids[at]);
					textures.remove(&bitmaps[at]);
				}
				_ => {
					textures.begin_frame();
					let bitmap = &bitmaps[at];
					let bytes = bitmap.pixels.len() * 4;
					let hit = textures.get(bitmap).is_some();
					let drawn = hit
						|| textures.make_room(bytes) && {
							textures.insert(bitmap, ids[at], bytes);
							true
						};
					assert_eq!(drawn, model.draw(ids[at], bytes), "seed {seed} step {step}");
				}
			}
			assert_eq!(textures.len(), model.kept.len(), "seed {seed} step {step}");
			assert!(textures.bytes() <= budget, "seed {seed} step {step}");
		}
		textures.begin_frame();
		assert_eq!(textures.bytes(), model.bytes(), "seed {seed}");
		for (bitmap, id) in bitmaps.iter().zip(&ids) {
			let kept = model.kept.iter().any(|&(kept, _)| kept == *id);
			assert_eq!(
				textures.get(bitmap).copied(),
				kept.then_some(*id),
				"seed {seed}"
			);
		}
	}
}

#[test]
fn a_bitmap_whose_size_overflows_never_gets_room() {
	let mut textures: Textures<()> = Textures::new(usize::MAX - 1);
	textures.begin_frame();
	for (width, height) in [(usize::MAX, 2), (2, usize::MAX), (usize::MAX / 2, 3)] {
		let huge = Bitmap {
			width,
			height,
			opaque: true,
			pixels: Vec::new(),
		};
		assert!(
			!textures.make_room(texture_bytes(&huge)),
			"{width}×{height}"
		);
	}
	assert_eq!(textures.bytes(), 0);
}
