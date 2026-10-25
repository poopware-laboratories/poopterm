//! Square pages of bitmaps (color emoji, images of glyphs) packed on
//! shelves, evicted a page at a time by when they were last used, with the
//! rectangles each page needs uploaded.
use crate::render::fxhash::FxHashMap;
use std::{borrow::Borrow, hash::Hash};

/// A rectangle of a page, in pixels: where a bitmap lies, or what the GPU
/// has not been sent yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slot {
	pub page: usize,
	pub x: usize,
	pub y: usize,
	pub w: usize,
	pub h: usize,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Refused {
	/// Larger than a page.
	TooBig,
	/// Every page holds a bitmap drawn this frame.
	Full,
}

/// A row of bitmaps as tall as `h`, filled from the left up to `x`.
struct Shelf {
	y: usize,
	h: usize,
	x: usize,
}

#[derive(Default)]
struct Page {
	shelves: Vec<Shelf>,
	/// The top of the free space below the shelves.
	bottom: usize,
	used: u64,
	/// Left, top, right and bottom of what changed since the last upload.
	dirty: Option<[usize; 4]>,
}

impl Page {
	/// Places a `w` × `h` bitmap on the tightest shelf no more than twice
	/// its height, or on a new shelf, leaving a pixel between neighbours
	/// so filtering never blends them.
	fn place(&mut self, side: usize, (w, h): (usize, usize)) -> Option<(usize, usize)> {
		let fits = |shelf: &&mut Shelf| shelf.h >= h && shelf.h <= 2 * h && shelf.x + w <= side;
		let shelf = match self
			.shelves
			.iter_mut()
			.filter(fits)
			.min_by_key(|shelf| shelf.h)
		{
			Some(shelf) => shelf,
			None if self.bottom + h <= side => {
				self.shelves.push(Shelf {
					y: self.bottom,
					h,
					x: 0,
				});
				self.bottom += h + 1;
				self.shelves.last_mut()?
			}
			None => return None,
		};
		let at = (shelf.x, shelf.y);
		shelf.x += w + 1;
		// Empty bitmaps have nothing to upload, and a GPU copy of no pixels
		// is invalid.
		if w > 0 && h > 0 {
			let [left, top, right, bottom] = self.dirty.unwrap_or([at.0, at.1, 0, 0]);
			self.dirty = Some([
				left.min(at.0),
				top.min(at.1),
				right.max(at.0 + w),
				bottom.max(at.1 + h),
			]);
		}
		Some(at)
	}
}

/// Bitmaps keyed by `K` on up to `max_pages` pages `side` pixels square.
pub(crate) struct Atlas<K> {
	side: usize,
	max_pages: usize,
	pages: Vec<Page>,
	slots: FxHashMap<K, Slot>,
	frame: u64,
}

impl<K: Hash + Eq> Atlas<K> {
	pub(crate) fn new(side: usize, max_pages: usize) -> Self {
		Self {
			side,
			max_pages,
			pages: Vec::new(),
			slots: FxHashMap::default(),
			frame: 0,
		}
	}

	/// Starts a frame: pages used only before it may be evicted.
	pub(crate) fn begin_frame(&mut self) {
		self.frame += 1;
	}

	/// Every bitmap's slot, leaving pages unmarked.
	#[cfg(test)]
	pub(crate) fn slots(&self) -> impl Iterator<Item = (&K, &Slot)> {
		self.slots.iter()
	}

	/// The slot of `key`, marking its page used this frame.
	pub(crate) fn get<Q: Hash + Eq + ?Sized>(&mut self, key: &Q) -> Option<Slot>
	where
		K: Borrow<Q>,
	{
		let slot = *self.slots.get(key)?;
		self.pages[slot.page].used = self.frame;
		Some(slot)
	}

	/// The slot of `key`, placing a `w` × `h` bitmap for it when it has none.
	/// When every page is full, empties the one used longest ago, unless it
	/// was used this frame.
	pub(crate) fn insert(&mut self, key: K, (w, h): (usize, usize)) -> Result<Slot, Refused> {
		if let Some(slot) = self.get(&key) {
			return Ok(slot);
		}
		if w > self.side || h > self.side {
			return Err(Refused::TooBig);
		}
		let side = self.side;
		let mut pages = self.pages.iter_mut().enumerate();
		let found = pages.find_map(|(page, data)| Some((page, data.place(side, (w, h))?)));
		let (page, (x, y)) = match found {
			Some(found) => found,
			None => {
				let page = self.free_page()?;
				// An empty page holds anything no larger than itself.
				let at = self.pages[page].place(side, (w, h));
				(page, at.ok_or(Refused::TooBig)?)
			}
		};
		self.pages[page].used = self.frame;
		let slot = Slot { page, x, y, w, h };
		self.slots.insert(key, slot);
		Ok(slot)
	}

	/// A new page, or the emptied page used longest ago.
	fn free_page(&mut self) -> Result<usize, Refused> {
		if self.pages.len() < self.max_pages {
			self.pages.push(Page::default());
			return Ok(self.pages.len() - 1);
		}
		let pages = self.pages.iter().enumerate();
		let (oldest, _) = (pages.filter(|(_, page)| page.used < self.frame))
			.min_by_key(|(_, page)| page.used)
			.ok_or(Refused::Full)?;
		self.slots.retain(|_, slot| slot.page != oldest);
		// Its old pixels are never sampled again, so only new ones are dirty.
		self.pages[oldest] = Page::default();
		Ok(oldest)
	}

	/// What changed since the last call, page by page, for the GPU to upload.
	pub(crate) fn take_dirty(&mut self) -> Vec<Slot> {
		let pages = self.pages.iter_mut().enumerate();
		let dirty = pages.filter_map(|(page, data)| Some((page, data.dirty.take()?)));
		(dirty.map(|(page, [left, top, right, bottom])| Slot {
			page,
			x: left,
			y: top,
			w: right - left,
			h: bottom - top,
		}))
		.collect()
	}
}
