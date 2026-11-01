//! Textures of the images frames draw, kept on the GPU between frames while
//! their bitmaps live and the budget allows: found by the bitmap itself,
//! evicted least recently used first, never one the frame being drawn uses.
use crate::{graphics::Bitmap, render::fxhash::FxHashMap};
use std::{
	ops::Range,
	sync::{Arc, Weak},
};

struct Entry<T> {
	/// Keeps the bitmap's allocation, so its address names no other while
	/// the entry lives, and tells when the bitmap is dropped.
	bitmap: Weak<Bitmap>,
	bytes: usize,
	/// The frame that last drew it.
	used: u64,
	texture: T,
}

pub(crate) struct Textures<T> {
	/// By the address of the bitmap.
	entries: FxHashMap<usize, Entry<T>>,
	/// Bytes the textures may take, and take.
	budget: usize,
	bytes: usize,
	frame: u64,
}

/// The bytes a texture of `bitmap` takes, four a pixel.
fn bytes(bitmap: &Bitmap) -> usize {
	bitmap.width.saturating_mul(bitmap.height).saturating_mul(4)
}

fn key(bitmap: &Arc<Bitmap>) -> usize {
	Arc::as_ptr(bitmap) as usize
}

impl<T> Textures<T> {
	/// No textures, which may take up to `budget` bytes.
	pub(crate) fn new(budget: usize) -> Self {
		Self {
			entries: FxHashMap::default(),
			budget,
			bytes: 0,
			frame: 0,
		}
	}

	/// Starts a frame, dropping the textures whose bitmaps are gone.
	pub(crate) fn begin_frame(&mut self) {
		self.frame += 1;
		let bytes = &mut self.bytes;
		self.entries.retain(|_, entry| {
			let live = entry.bitmap.strong_count() > 0;
			if !live {
				*bytes -= entry.bytes;
			}
			live
		});
	}

	/// The texture of `bitmap`, marked used by this frame.
	pub(crate) fn get(&mut self, bitmap: &Arc<Bitmap>) -> Option<&T> {
		let entry = self.entries.get_mut(&key(bitmap))?;
		entry.used = self.frame;
		Some(&entry.texture)
	}

	/// Makes room for a texture of `bitmap`, evicting the least recently
	/// used textures this frame does not; false, evicting none, when even
	/// that leaves too little.
	pub(crate) fn make_room(&mut self, bitmap: &Bitmap) -> bool {
		let needed = bytes(bitmap);
		let evictable: usize = (self.entries.values())
			.filter(|entry| entry.used < self.frame)
			.map(|entry| entry.bytes)
			.sum();
		if needed > self.budget.saturating_sub(self.bytes - evictable) {
			return false;
		}
		while self.bytes + needed > self.budget {
			let Some((&oldest, _)) = (self.entries.iter())
				.filter(|(_, entry)| entry.used < self.frame)
				.min_by_key(|(_, entry)| entry.used)
			else {
				break;
			};
			if let Some(entry) = self.entries.remove(&oldest) {
				self.bytes -= entry.bytes;
			}
		}
		true
	}

	/// Keeps `texture` as `bitmap`'s, used by this frame, in place of any
	/// it had; room should be made first.
	pub(crate) fn insert(&mut self, bitmap: &Arc<Bitmap>, texture: T) -> &T {
		let entry = Entry {
			bitmap: Arc::downgrade(bitmap),
			bytes: bytes(bitmap),
			used: self.frame,
			texture,
		};
		self.bytes += entry.bytes;
		if let Some(old) = self.entries.insert(key(bitmap), entry) {
			self.bytes -= old.bytes;
		}
		&self.entries[&key(bitmap)].texture
	}

	/// Forgets `bitmap`'s texture: for one that never reached the GPU.
	pub(crate) fn remove(&mut self, bitmap: &Arc<Bitmap>) {
		if let Some(entry) = self.entries.remove(&key(bitmap)) {
			self.bytes -= entry.bytes;
		}
	}

	/// Forgets every texture: for when the GPU's are gone.
	pub(crate) fn clear(&mut self) {
		self.entries.clear();
		self.bytes = 0;
	}

	pub(crate) fn len(&self) -> usize {
		self.entries.len()
	}

	/// The bytes the textures take.
	pub(crate) fn bytes(&self) -> usize {
		self.bytes
	}
}

/// Quads drawn in order, each needing the texture it names or none, split
/// into runs drawn with one texture bound: a quad needing none joins the
/// run it is in, and a run ends only where another texture is needed.
pub(crate) fn runs<K: PartialEq>(
	needs: impl IntoIterator<Item = Option<K>>,
) -> Vec<(Range<u32>, Option<K>)> {
	let mut runs: Vec<(Range<u32>, Option<K>)> = Vec::new();
	for (index, need) in (0u32..).zip(needs) {
		match (runs.last_mut(), need) {
			(Some((range, bound)), need) if need.is_none() || *bound == need || bound.is_none() => {
				range.end = index + 1;
				if need.is_some() {
					*bound = need;
				}
			}
			(_, need) => runs.push((index..index + 1, need)),
		}
	}
	runs
}
