//! Installed fonts for characters the configured fonts lack: the font
//! database is opened on the first missing character, and only fonts whose
//! pages cover it are loaded, a few at a time.
use std::{
	collections::HashSet,
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

/// The faces that may have `ch`, in color if `color`, best first: fonts
/// whose pages cover it, monospace before proportional, upright regular
/// weights first, then smaller files, which are usually the more specific
/// fonts.
pub(crate) fn candidates(cache: &Cache, ch: char, color: bool) -> Vec<(&Path, u32)> {
	let mut found: Vec<_> = cache
		.files
		.iter()
		.filter(|file| file.stamp.size <= MAX_FILE)
		.flat_map(|file| file.faces.iter().map(move |face| (file, face)))
		.filter(|(_, face)| if color { face.color } else { face.drawable })
		.filter(|(_, face)| covers(&face.pages, ch))
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

/// Loads installed fonts on demand for characters no loaded font has.
pub(crate) struct SystemFallback {
	open: Option<Box<dyn FnOnce() -> Cache>>,
	cache: Cache,
	/// Faces already loaded, or that failed to load.
	tried: HashSet<(PathBuf, u32)>,
	/// Characters, and whether in color, found nowhere.
	missing: HashSet<(char, bool)>,
}

impl SystemFallback {
	/// `open` reads the font database; it runs on the first lookup.
	pub(crate) fn new(open: impl FnOnce() -> Cache + 'static) -> Self {
		Self {
			open: Some(Box::new(open)),
			cache: Cache::default(),
			tried: HashSet::new(),
			missing: HashSet::new(),
		}
	}

	/// Loads untried fonts that may have `ch`, in color if `color`, into
	/// `fonts` until one does, returning its index there.
	pub(crate) fn find(&mut self, ch: char, color: bool, fonts: &mut Vec<Font>) -> Option<usize> {
		if self.missing.contains(&(ch, color)) || self.tried.len() >= MAX_LOADED {
			return None;
		}
		if let Some(open) = self.open.take() {
			self.cache = open();
		}
		let untried: Vec<(PathBuf, u32)> = candidates(&self.cache, ch, color)
			.into_iter()
			.filter(|&(path, index)| !self.tried.contains(&(path.to_owned(), index)))
			.take(TRIES_PER_CHAR.min(MAX_LOADED - self.tried.len()))
			.map(|(path, index)| (path.to_owned(), index))
			.collect();
		for (path, index) in untried {
			let loaded = fs::read(&path)
				.ok()
				.and_then(|bytes| Font::from_bytes(bytes, index).ok());
			self.tried.insert((path, index));
			// Kept even without `ch`: it may have the next character.
			if let Some(font) = loaded {
				fonts.push(font);
				if has(fonts.last()?, ch, color) {
					return Some(fonts.len() - 1);
				}
			}
		}
		if self.missing.len() >= MAX_MISSING {
			self.missing.clear();
		}
		self.missing.insert((ch, color));
		None
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
