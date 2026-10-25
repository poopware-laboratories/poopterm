//! Font fallback resolution and the rasterized-glyph cache.
use std::collections::hash_map::Entry;

use crate::fonts::{
	Font, Metrics,
	fallback::{SystemFallback, has},
};

use super::{boxdraw, fxhash::FxHashMap};

/// Upper bound on cached rasterizations before the cache is reset.
const MAX_CACHED_GLYPHS: usize = 4096;

/// A rasterized glyph: placement metrics plus an 8-bit coverage bitmap, or
/// color pixels for color glyphs (emoji).
pub(crate) struct Glyph {
	pub metrics: Metrics,
	pub coverage: Vec<u8>,
	/// `0xAARRGGBB` with straight alpha, drawn instead of `coverage`.
	pub color: Option<Vec<u32>>,
}

/// The primary font, its fallbacks, and every glyph rasterized so far.
///
/// Lookups are keyed by `(char, pixel size)` and resolve the fallback font
/// only on a cache miss, so drawing a cell costs a single hash lookup.
pub(crate) struct FontSet {
	primary: Font,
	fallbacks: Vec<Font>,
	/// Installed fonts, searched when no font above has a character.
	system: Option<SystemFallback>,
	cache: FxHashMap<(char, u32), Glyph>,
	/// Box drawing and other glyphs drawn in code, for cells of `drawn_cell`.
	drawn: FxHashMap<char, boxdraw::Drawn>,
	drawn_cell: (usize, usize),
}

impl FontSet {
	pub(crate) fn new(primary: Font, fallbacks: Vec<Font>) -> Self {
		Self {
			primary,
			fallbacks,
			system: None,
			cache: FxHashMap::default(),
			drawn: FxHashMap::default(),
			drawn_cell: (0, 0),
		}
	}

	/// The configured fonts, falling back to every installed font.
	pub(crate) fn load() -> Self {
		let (primary, fallbacks) = crate::fonts::load_fonts();
		let system = SystemFallback::new(crate::fonts::system_fonts);
		Self::new(primary, fallbacks).with_system_fallback(system)
	}

	/// Also searches installed fonts for characters no given font has.
	pub(crate) fn with_system_fallback(mut self, system: SystemFallback) -> Self {
		self.system = Some(system);
		self
	}

	pub(crate) fn primary(&self) -> &Font {
		&self.primary
	}

	/// Returns the cached glyph for `ch` at `px`, rasterizing it on first use
	/// with the first font that contains it, an installed one if none does
	/// (the primary font otherwise).
	#[inline]
	pub(crate) fn glyph(&mut self, ch: char, px: f32) -> &Glyph {
		match self.cache.entry((ch, px.to_bits())) {
			Entry::Occupied(entry) => entry.into_mut(),
			Entry::Vacant(entry) => {
				let mut find = |color| {
					(self.fallbacks.iter())
						.position(|font| has(font, ch, color))
						.or_else(|| self.system.as_mut()?.find(ch, color, &mut self.fallbacks))
				};
				let found = (emoji_presentation(ch).then(|| find(true)).flatten())
					.or_else(|| (!has(&self.primary, ch, false)).then(|| find(false))?);
				let font = found.map_or(&self.primary, |index| &self.fallbacks[index]);
				let color = font.rasterize_color(ch, px);
				let (metrics, coverage) = match &color {
					Some((metrics, _)) => (*metrics, Vec::new()),
					None => font.rasterize(ch, px),
				};
				let color = color.map(|(_, pixels)| pixels);
				entry.insert(Glyph {
					metrics,
					coverage,
					color,
				})
			}
		}
	}

	/// Returns `ch` drawn in code to fill a `cell` exactly (box drawing,
	/// blocks, Braille, Powerline), or `None` for glyphs taken from fonts.
	#[inline]
	pub(crate) fn drawn(&mut self, ch: char, cell: (usize, usize)) -> Option<&boxdraw::Drawn> {
		if !boxdraw::covers(ch) {
			return None;
		}
		// One cell size at a time bounds the cache to the covered characters.
		if self.drawn_cell != cell {
			self.drawn.clear();
			self.drawn_cell = cell;
		}
		match self.drawn.entry(ch) {
			Entry::Occupied(entry) => Some(entry.into_mut()),
			Entry::Vacant(entry) => {
				let coverage = boxdraw::draw(ch, cell)?;
				Some(entry.insert(boxdraw::compact(&coverage, cell)))
			}
		}
	}

	/// Bounds memory use; call once per frame, before drawing.
	pub(crate) fn trim(&mut self) {
		if self.cache.len() > MAX_CACHED_GLYPHS {
			self.cache.clear();
		}
	}

	/// Drops every rasterization, e.g. after the font size changes.
	pub(crate) fn clear(&mut self) {
		self.cache.clear();
		self.drawn.clear();
	}

	#[cfg(test)]
	pub(crate) fn cached(&self) -> usize {
		self.cache.len()
	}
}

/// Whether `ch` is shown as an emoji by default (Emoji_Presentation): the
/// wide characters among symbols, which leaves out CJK.
pub(crate) fn emoji_presentation(ch: char) -> bool {
	matches!(u32::from(ch), 0x2300..=0x2bff | 0x1f000..=0x1fbff) && crate::width::width(ch) == 2
}

#[cfg(test)]
mod tests {
	use super::*;

	fn bundled() -> Font {
		Font::from_bytes(crate::fonts::BUNDLED, 0).unwrap()
	}

	#[test]
	fn glyphs_are_cached_separately_at_each_font_size() {
		let mut fonts = FontSet::new(bundled(), Vec::new());
		let small = fonts.glyph('a', 12.0).metrics.height;
		let large = fonts.glyph('a', 24.0).metrics.height;
		assert!(large > small);
		fonts.glyph('a', 12.0);
		assert_eq!(fonts.cached(), 2);
	}

	#[test]
	fn trim_resets_an_oversized_cache() {
		let mut fonts = FontSet::new(bundled(), Vec::new());
		for ch in (0..=MAX_CACHED_GLYPHS as u32).filter_map(char::from_u32) {
			fonts.glyph(ch, 10.0);
		}
		fonts.trim();
		assert_eq!(fonts.cached(), 0);
	}

	/// A system fallback whose only installed font has `ch`, counting how
	/// often the font database is opened.
	fn system_with(ch: char, name: &str) -> (SystemFallback, std::rc::Rc<std::cell::Cell<u32>>) {
		use crate::fonts::{coverage::pages, db, fallback_tests::font_with_only};
		let dir = std::env::temp_dir().join(format!("poopterm-glyphs-{}-{name}", std::process::id()));
		std::fs::create_dir_all(&dir).unwrap();
		std::fs::write(dir.join("only.ttf"), font_with_only(ch)).unwrap();
		let (mut cache, _) = db::refresh(&[dir], None);
		cache.files[0].faces[0].pages = pages([(0, 0x10ffff)]);
		let opened = std::rc::Rc::new(std::cell::Cell::new(0));
		let counter = opened.clone();
		let system = SystemFallback::new(move || {
			counter.set(counter.get() + 1);
			cache
		});
		(system, opened)
	}

	#[test]
	fn characters_no_font_has_are_drawn_from_an_installed_font() {
		let (system, opened) = system_with('漢', "found");
		let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
		let letter_a = bundled().rasterize('A', 16.0);
		let han = fonts.glyph('漢', 16.0);
		assert_eq!((han.metrics, &han.coverage), (letter_a.0, &letter_a.1));
		assert_eq!(opened.get(), 1);
	}

	#[test]
	fn the_installed_fonts_are_not_opened_for_characters_the_fonts_have() {
		let (system, opened) = system_with('漢', "present");
		let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
		for ch in ['a', '─', '\u{e0b0}'] {
			fonts.glyph(ch, 16.0);
		}
		assert_eq!(opened.get(), 0);
	}

	#[test]
	fn a_character_no_font_anywhere_has_falls_back_to_the_primary_font() {
		let (system, _) = system_with('漢', "nowhere");
		let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
		let tofu = bundled().rasterize('字', 16.0);
		let glyph = fonts.glyph('字', 16.0);
		assert_eq!((glyph.metrics, &glyph.coverage), (tofu.0, &tofu.1));
		// The installed font that was loaded still serves its own character.
		let letter_a = bundled().rasterize('A', 16.0);
		assert_eq!(fonts.glyph('漢', 16.0).coverage, letter_a.1);
	}

	#[test]
	fn without_a_system_fallback_missing_characters_use_the_primary_font() {
		let mut fonts = FontSet::new(bundled(), Vec::new());
		let tofu = bundled().rasterize('漢', 16.0);
		assert_eq!(fonts.glyph('漢', 16.0).coverage, tofu.1);
	}

	#[test]
	fn a_found_font_serves_every_size_and_survives_clearing_the_cache() {
		let (system, opened) = system_with('漢', "sizes");
		let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
		let small = fonts.glyph('漢', 12.0).metrics;
		let large = fonts.glyph('漢', 24.0).metrics;
		assert!(large.height > small.height);
		fonts.clear();
		let again = fonts.glyph('漢', 12.0).metrics;
		assert_eq!(again, small);
		assert_eq!(opened.get(), 1, "the database is read once");
	}

	#[test]
	fn the_primary_font_wins_over_installed_fonts_that_also_have_a_character() {
		// The installed font draws 'a' as an 'A'; the primary has its own 'a'.
		let (system, opened) = system_with('a', "primary");
		let mut fonts = FontSet::new(bundled(), Vec::new()).with_system_fallback(system);
		assert_eq!(
			fonts.glyph('a', 16.0).coverage,
			bundled().rasterize('a', 16.0).1
		);
		assert_eq!(opened.get(), 0);
	}

	/// A color font with a red 4×4 bitmap for each of `chars`.
	fn color(chars: &[char]) -> Font {
		use crate::fonts::cbdt_tests::{Image, Strike, color_font, png};
		let image = Image {
			metrics: [4, 4, 0, 4, 4],
			png: png(4, 4, &[0xffff0000; 16]),
		};
		let strike = Strike {
			ppem: 16,
			first: 1,
			index_format: 1,
			image_format: 17,
			images: vec![Some(image); chars.len()],
		};
		Font::from_bytes(color_font(chars, &[strike]), 0).unwrap()
	}

	fn outline(ch: char) -> Font {
		Font::from_bytes(crate::fonts::fallback_tests::font_with_only(ch), 0).unwrap()
	}

	#[test]
	fn emoji_presentation_covers_wide_symbols_but_not_text_or_cjk() {
		for ch in ['😀', '👍', '🦀', '⚡', '⌚', '☕', '♈', '⬛', '🀄'] {
			assert!(emoji_presentation(ch), "{ch:?}");
		}
		// Text-style symbols, CJK and Hangul are wide or symbolic, not emoji.
		for ch in [
			'a',
			'☃',
			'♞',
			'❤',
			'✔',
			'⌘',
			'漢',
			'한',
			'＃',
			'\u{2bff}',
			'\u{1fc00}',
		] {
			assert!(!emoji_presentation(ch), "{ch:?}");
		}
	}

	#[test]
	fn emoji_prefer_a_color_font_over_earlier_outline_fonts() {
		// The outline font comes first, as symbol fonts loaded at startup do.
		let mut fonts = FontSet::new(bundled(), vec![outline('👍'), color(&['👍'])]);
		assert_eq!(fonts.glyph('👍', 16.0).color, Some(vec![0xffff0000; 16]));
	}

	#[test]
	fn emoji_without_a_color_font_use_the_outline_one() {
		let mut fonts = FontSet::new(bundled(), vec![outline('👍')]);
		let glyph = fonts.glyph('👍', 16.0);
		assert!(glyph.color.is_none() && glyph.coverage.iter().any(|&c| c > 0));
	}

	#[test]
	fn emoji_in_color_win_over_the_primary_font_too() {
		// The bundled font has a black-and-white ⚡; the emoji is in color.
		assert_ne!(bundled().lookup_glyph_index('⚡'), 0);
		let mut fonts = FontSet::new(bundled(), vec![color(&['⚡'])]);
		assert!(fonts.glyph('⚡', 16.0).color.is_some());
	}

	#[test]
	fn text_symbols_keep_the_first_font_that_has_them() {
		// ☃ defaults to text: the primary font's outline wins over color.
		let mut fonts = FontSet::new(bundled(), vec![color(&['☃', '漢'])]);
		assert!(fonts.glyph('☃', 16.0).color.is_none());
		// A character only a color font has is still drawn, in color.
		assert!(fonts.glyph('漢', 16.0).color.is_some());
	}

	#[test]
	fn color_glyphs_are_cached_per_size() {
		let mut fonts = FontSet::new(bundled(), vec![color(&['😀'])]);
		let small = fonts.glyph('😀', 8.0).metrics;
		let large = fonts.glyph('😀', 32.0).metrics;
		assert_eq!((small.width, large.width), (2, 8));
		assert_eq!(fonts.cached(), 2);
	}
}
