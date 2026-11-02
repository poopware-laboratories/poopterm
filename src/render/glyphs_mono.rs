//! Monospace UI text, such as the Diagnostics page's log: a font set's
//! mono face and the glyphs rasterized from it.
use super::{FontSet, Glyph};
use crate::fonts::{Font, fallback::has};

impl FontSet {
	/// Draws monospace text with `font`.
	pub(crate) fn with_mono(mut self, font: Font) -> Self {
		self.mono = Some(font);
		self
	}

	/// The face monospace text is drawn with, if the set has one.
	#[cfg(test)]
	pub(crate) fn mono(&self) -> Option<&Font> {
		self.mono.as_ref()
	}

	/// The glyph for `ch` at `px` in the mono face, or as [`Self::glyph`]
	/// finds it when the set has no mono face or the face lacks `ch`.
	pub(crate) fn mono_glyph(&mut self, ch: char, px: f32) -> &Glyph {
		let key = (ch, px.to_bits());
		if !self.mono_cache.contains_key(&key) {
			let Some(font) = self.mono.as_ref().filter(|font| has(font, ch, false)) else {
				return self.glyph(ch, px);
			};
			let (metrics, coverage) = font.rasterize(ch, px);
			let glyph = Glyph {
				metrics,
				coverage,
				color: None,
			};
			self.mono_cache.insert(key, glyph);
		}
		&self.mono_cache[&key]
	}

	/// How far monospace text moves past `ch` at `px`, in whole pixels.
	pub(crate) fn mono_advance(&mut self, ch: char, px: f32) -> usize {
		self.mono_glyph(ch, px).metrics.advance_width.ceil() as usize
	}

	#[cfg(test)]
	pub(crate) fn mono_cached(&self) -> usize {
		self.mono_cache.len()
	}
}
