//! Glyph records in one buffer of fixed-size pages, appended in turn,
//! evicted a page at a time by when they were last used, with the words
//! each page needs uploaded.
use super::atlas::Refused;
use crate::render::fxhash::FxHashMap;
use std::{hash::Hash, ops::Range};

/// Where a record lies in the buffer, in words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Record {
	pub start: usize,
	pub len: usize,
}

#[derive(Clone, Default)]
struct Page {
	/// Words used from the page's start.
	end: usize,
	used: u64,
	/// What changed since the last upload, from the page's start.
	dirty: Option<Range<usize>>,
}

/// Records keyed by `K` on up to `max_pages` pages of `page` words, with
/// the buffer's contents kept here for uploading.
pub(crate) struct Store<K> {
	page: usize,
	max_pages: usize,
	pages: Vec<Page>,
	records: FxHashMap<K, (usize, Record)>,
	data: Vec<u32>,
	frame: u64,
}

impl<K: Hash + Eq> Store<K> {
	pub(crate) fn new(page: usize, max_pages: usize) -> Self {
		Self {
			page,
			max_pages,
			pages: Vec::new(),
			records: FxHashMap::default(),
			data: Vec::new(),
			frame: 0,
		}
	}

	/// Starts a frame: pages used only before it may be evicted.
	pub(crate) fn begin_frame(&mut self) {
		self.frame += 1;
	}

	/// The buffer's contents, one page after another.
	pub(crate) fn data(&self) -> &[u32] {
		&self.data
	}

	/// Every record, leaving pages unmarked.
	#[cfg(test)]
	pub(crate) fn records(&self) -> impl Iterator<Item = (&K, &Record)> {
		self.records.iter().map(|(key, (_, record))| (key, record))
	}

	/// The record of `key`, marking its page used this frame.
	pub(crate) fn get(&mut self, key: &K) -> Option<Record> {
		let (page, record) = *self.records.get(key)?;
		self.pages[page].used = self.frame;
		Some(record)
	}

	/// The record of `key`, writing `words` for it when it has none. When
	/// every page is full, empties the one used longest ago, unless it was
	/// used this frame.
	pub(crate) fn insert(&mut self, key: K, words: &[u32]) -> Result<Record, Refused> {
		if let Some(record) = self.get(&key) {
			return Ok(record);
		}
		if words.len() > self.page {
			return Err(Refused::TooBig);
		}
		let fits = |page: &Page| page.end + words.len() <= self.page;
		let page = match self.pages.iter().position(fits) {
			Some(page) => page,
			None => self.free_page()?,
		};
		let data = &mut self.pages[page];
		let (at, end) = (data.end, data.end + words.len());
		data.end = end;
		data.used = self.frame;
		// A GPU copy of no words is invalid.
		if !words.is_empty() {
			let dirty = data.dirty.get_or_insert(at..end);
			dirty.end = end;
		}
		let start = page * self.page + at;
		self.data[start..start + words.len()].copy_from_slice(words);
		let record = Record {
			start,
			len: words.len(),
		};
		self.records.insert(key, (page, record));
		Ok(record)
	}

	/// A new page, or the emptied page used longest ago.
	fn free_page(&mut self) -> Result<usize, Refused> {
		if self.pages.len() < self.max_pages {
			self.pages.push(Page::default());
			self.data.resize(self.pages.len() * self.page, 0);
			return Ok(self.pages.len() - 1);
		}
		let pages = self.pages.iter().enumerate();
		let (oldest, _) = (pages.filter(|(_, page)| page.used < self.frame))
			.min_by_key(|(_, page)| page.used)
			.ok_or(Refused::Full)?;
		self.records.retain(|_, (page, _)| *page != oldest);
		// Its old words are never read again, so only new ones are dirty.
		self.pages[oldest] = Page::default();
		Ok(oldest)
	}

	/// The words changed since the last call, page by page, for the GPU to
	/// upload.
	pub(crate) fn take_dirty(&mut self) -> Vec<Range<usize>> {
		let page = self.page;
		let pages = self.pages.iter_mut().enumerate();
		let dirty = pages.filter_map(|(index, data)| Some((index * page, data.dirty.take()?)));
		dirty
			.map(|(base, range)| base + range.start..base + range.end)
			.collect()
	}
}
