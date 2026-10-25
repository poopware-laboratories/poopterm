//! Clusters drawn as one glyph: emoji ZWJ sequences, keycaps and tag
//! sequences, which fonts join through their ligatures.
use super::{FontSet, Glyph, Presentation};
use crate::fonts::{Font, Metrics, fallback::has};

/// A glyph that draws nothing.
static NO_GLYPH: Glyph = Glyph {
	metrics: Metrics {
		xmin: 0,
		ymin: 0,
		width: 0,
		height: 0,
		advance_width: 0.0,
	},
	coverage: Vec::new(),
	color: None,
};

/// Whether a cell's `marks` make its cluster a sequence some font may draw
/// as one glyph: a joiner, the keycap mark, or tag characters.
pub(crate) fn joins(marks: &[char]) -> bool {
	marks
		.iter()
		.any(|&mark| matches!(mark, '\u{200d}' | '\u{20e3}' | '\u{e0020}'..='\u{e007f}'))
}

/// What a cell draws: a character in a presentation, or a cluster that a
/// font joins into one glyph.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Shape<'a> {
	Char(char, Presentation),
	Cluster(&'a [char]),
}

impl FontSet {
	/// The glyph a font joins the cluster `chars` into at `px`, from a
	/// fallback that has its first character (color fonts are found for
	/// it first), else the primary font; `None` when no font joins them.
	pub(crate) fn cluster(&mut self, chars: &[char], px: f32) -> Option<&Glyph> {
		let mut key = std::mem::take(&mut self.cluster_key);
		key.clear();
		key.push(px.to_bits());
		key.extend(chars.iter().map(|&ch| u32::from(ch)));
		if !self.clusters.contains_key(&key[..]) {
			let glyph = self.ligature(chars).map(|(font, glyph)| {
				let font = font.map_or(&self.primary, |index| &self.fallbacks[index]);
				rasterize(font, glyph, px)
			});
			self.clusters.insert(key.clone().into_boxed_slice(), glyph);
		}
		let found = self.clusters.get(&key[..]);
		self.cluster_key = key;
		found?.as_ref()
	}

	/// The glyph for `shape` at `px`; a cluster no font joins draws its
	/// first character as an emoji.
	pub(crate) fn shaped(&mut self, shape: Shape, px: f32) -> &Glyph {
		match shape {
			Shape::Char(ch, presentation) => self.presented(ch, px, presentation),
			Shape::Cluster(chars) => {
				if self.cluster(chars, px).is_none() {
					let first = chars.first().copied().unwrap_or(' ');
					return self.presented(first, px, Presentation::Emoji);
				}
				// Cached by the call above.
				self.cluster(chars, px).unwrap_or(&NO_GLYPH)
			}
		}
	}

	/// The font (`None` for the primary) and glyph that join `chars`.
	fn ligature(&mut self, chars: &[char]) -> Option<(Option<usize>, u16)> {
		let first = *chars.first()?;
		let joined = |(index, font): (usize, &Font)| Some((Some(index), font.ligature(chars)?));
		let fallbacks = || self.fallbacks.iter().enumerate();
		if let Some(found) = fallbacks()
			.filter(|(_, font)| has(font, first, true))
			.find_map(joined)
		{
			return Some(found);
		}
		let system = self.system.as_mut();
		if let Some(index) = system.and_then(|system| system.find(first, true, &mut self.fallbacks))
			&& let Some(glyph) = self.fallbacks[index].ligature(chars)
		{
			return Some((Some(index), glyph));
		}
		if let Some(glyph) = self.primary.ligature(chars) {
			return Some((None, glyph));
		}
		let fallbacks = self.fallbacks.iter().enumerate();
		fallbacks
			.filter(|(_, font)| has(font, first, false))
			.find_map(joined)
	}
}

/// `glyph` of `font` at `px`, in color when the font has it so.
fn rasterize(font: &Font, glyph: u16, px: f32) -> Glyph {
	match font.rasterize_color_glyph(glyph, px) {
		Some((metrics, pixels)) => Glyph {
			metrics,
			coverage: Vec::new(),
			color: Some(pixels),
		},
		None => {
			let (metrics, coverage) = font.rasterize_glyph(glyph, px);
			Glyph {
				metrics,
				coverage,
				color: None,
			}
		}
	}
}

#[cfg(test)]
#[path = "glyph_clusters_tests.rs"]
mod tests;
