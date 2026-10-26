//! Installed fonts for characters the configured fonts lack: the font
//! database is opened on the first missing character, and only fonts that
//! map it are loaded, a few at a time.
use std::{
	collections::{HashMap, HashSet},
	fs,
	path::{Path, PathBuf},
};

use super::{Font, coverage::covers, db::Cache};

/// Fonts loaded for one character before giving up on it.
const TRIES_PER_CHAR: usize = 4;
/// Fonts loaded in all; each stays in memory once loaded.
const MAX_LOADED: usize = 32;
/// Larger files are skipped, like the symbol fonts loaded at startup.
const MAX_FILE: u64 = 64 << 20;
/// Characters remembered as found nowhere before the list is reset.
const MAX_MISSING: usize = 4096;
/// Characters whose ranked candidates are remembered before the list is
/// reset.
const MAX_RANKED: usize = 512;

/// The faces that may have `ch`, in color if `color`, best first: fonts
/// whose `cmap` maps it, monospace before proportional, upright regular
/// weights first, then smaller files, which are usually the more specific
/// fonts.
pub(crate) fn candidates(cache: &Cache, ch: char, color: bool) -> Vec<(&Path, u32)> {
	let mut found: Vec<_> = cache
		.files
		.iter()
		.filter(|file| file.stamp.size <= MAX_FILE)
		.flat_map(|file| file.faces.iter().map(move |face| (file, face)))
		.filter(|(_, face)| if color { face.color } else { face.drawable })
		.filter(|(_, face)| covers(&face.coverage, ch))
		.collect();
	found.sort_by_key(|(file, face)| {
		(
			!face.monospace,
			face.italic,
			face.weight.abs_diff(400),
			file.stamp.size,
			&file.path,
			face.index,
		)
	});
	found
		.into_iter()
		.map(|(file, face)| (file.path.as_path(), face.index))
		.collect()
}

/// Picks installed fonts for characters the configured fonts lack, loading
/// them on demand. The choice depends only on the character: the best
/// ranked candidate that has it, whatever was loaded before.
pub(crate) struct SystemFallback {
	open: Option<Box<dyn FnOnce() -> Cache>>,
	cache: Cache,
	/// Candidates by character and color: they depend on nothing else.
	ranked: HashMap<(char, bool), Vec<(PathBuf, u32)>>,
	/// Faces already loaded, or that failed to load.
	tried: HashSet<(PathBuf, u32)>,
	/// Where each loaded face is in the caller's list.
	loaded: HashMap<(PathBuf, u32), usize>,
	/// Characters, and whether in color, found nowhere.
	missing: HashSet<(char, bool)>,
}

impl SystemFallback {
	/// `open` reads the font database; it runs on the first lookup.
	pub(crate) fn new(open: impl FnOnce() -> Cache + 'static) -> Self {
		Self {
			open: Some(Box::new(open)),
			cache: Cache::default(),
			ranked: HashMap::new(),
			tried: HashSet::new(),
			loaded: HashMap::new(),
			missing: HashSet::new(),
		}
	}

	/// The index in `fonts` of the best ranked installed font with `ch`,
	/// in color if `color`, loading untried candidates into `fonts` on the
	/// way: at most a few per character, and a bounded number in all.
	pub(crate) fn find(&mut self, ch: char, color: bool, fonts: &mut Vec<Font>) -> Option<usize> {
		if self.missing.contains(&(ch, color)) {
			return None;
		}
		if let Some(open) = self.open.take() {
			self.cache = open();
		}
		if self.ranked.len() >= MAX_RANKED {
			self.ranked.clear();
		}
		let cache = &self.cache;
		let ranked = self.ranked.entry((ch, color)).or_insert_with(|| {
			candidates(cache, ch, color)
				.into_iter()
				.map(|(path, index)| (path.to_owned(), index))
				.collect()
		});
		let mut loads = 0;
		for face in ranked.iter() {
			let at = match self.loaded.get(face) {
				Some(&at) => at,
				None if self.tried.contains(face) => continue,
				None if loads == TRIES_PER_CHAR || self.tried.len() >= MAX_LOADED => continue,
				None => {
					loads += 1;
					self.tried.insert(face.clone());
					let Some(font) = fs::read(&face.0)
						.ok()
						.and_then(|bytes| Font::from_bytes(bytes, face.1).ok())
					else {
						continue;
					};
					// Kept even without `ch`: it may have the next character.
					fonts.push(font);
					self.loaded.insert(face.clone(), fonts.len() - 1);
					fonts.len() - 1
				}
			};
			if has(&fonts[at], ch, color) {
				return Some(at);
			}
		}
		if self.missing.len() >= MAX_MISSING {
			self.missing.clear();
		}
		self.missing.insert((ch, color));
		None
	}

	#[cfg(test)]
	pub(crate) fn ranked_chars(&self) -> usize {
		self.ranked.len()
	}
}

/// Whether `font` has `ch`, as a color bitmap if `color`.
pub(crate) fn has(font: &Font, ch: char, color: bool) -> bool {
	if color {
		font.has_color(ch)
	} else {
		font.lookup_glyph_index(ch) != 0
	}
}
