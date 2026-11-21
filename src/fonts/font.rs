//! A loaded font at the interface the renderer uses: glyph lookup, line
//! and glyph metrics in pixels, and rasterized coverage bitmaps.
use super::{
	colr::{self, Layer},
	data::Data,
	face::{Error, Face},
	quads::{Quad, Quads},
	raster::{Path, Raster},
};

/// Where a rasterized glyph sits relative to the pen on the baseline, y up.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Metrics {
	pub xmin: i32,
	pub ymin: i32,
	pub width: usize,
	pub height: usize,
	pub advance_width: f32,
}

/// Vertical metrics in pixels; `descent` is negative below the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineMetrics {
	pub ascent: f32,
	pub descent: f32,
	pub line_gap: f32,
	pub new_line_size: f32,
}

/// The color of `COLR` layers drawn in the text color: glyphs are cached
/// without the cell's color, so the light text of the default themes.
const LAYER_TEXT_COLOR: u32 = 0xffff_ffff;

pub(crate) struct Font {
	face: Face,
}

impl Font {
	/// Loads face `index` of a font file or collection.
	pub(crate) fn from_bytes(data: impl Into<Data>, index: u32) -> Result<Self, Error> {
		Face::parse(data.into(), index).map(|face| Self { face })
	}

	#[cfg(test)]
	pub(crate) fn is_mapped(&self) -> bool {
		matches!(self.face.data, Data::Mapped(_))
	}

	#[cfg(test)]
	pub(crate) fn is_static(&self) -> bool {
		matches!(self.face.data, Data::Static(_))
	}

	#[cfg(test)]
	pub(crate) fn is_cff(&self) -> bool {
		self.face.is_cff()
	}

	#[cfg(test)]
	pub(crate) fn is_cff2(&self) -> bool {
		self.face.is_cff2()
	}

	/// The glyph for `ch`, 0 when the font lacks it.
	pub(crate) fn lookup_glyph_index(&self, ch: char) -> u16 {
		self.face.glyph_index(ch)
	}

	fn scale(&self, px: f32) -> f32 {
		px / f32::from(self.face.units_per_em)
	}

	pub(crate) fn horizontal_line_metrics(&self, px: f32) -> Option<LineMetrics> {
		let scale = self.scale(px);
		let (ascent, descent, line_gap) = (
			f32::from(self.face.ascender) * scale,
			f32::from(self.face.descender) * scale,
			f32::from(self.face.line_gap) * scale,
		);
		Some(LineMetrics {
			ascent,
			descent,
			line_gap,
			new_line_size: ascent - descent + line_gap,
		})
	}

	pub(crate) fn metrics(&self, ch: char, px: f32) -> Metrics {
		self.outline(self.face.glyph_index(ch), px).0
	}

	/// `ch` as color pixels (`0xAARRGGBB`, straight alpha), for fonts that
	/// store it as a color bitmap.
	pub(crate) fn rasterize_color(&self, ch: char, px: f32) -> Option<(Metrics, Vec<u32>)> {
		self.rasterize_color_glyph(self.face.glyph_index(ch), px)
	}

	/// [`Self::rasterize_color`] for a glyph by its index: its color bitmap,
	/// or else its `COLR` layers.
	pub(crate) fn rasterize_color_glyph(&self, glyph: u16, px: f32) -> Option<(Metrics, Vec<u32>)> {
		match self.face.color_bitmap(glyph) {
			Some(bitmap) => super::color::render(&bitmap, px),
			None => Some(self.rasterize_layers(glyph, &self.face.color_layers(glyph)?, px)),
		}
	}

	/// `layers` of `glyph` painted bottom first over one box that holds
	/// them all, advancing as `glyph` does.
	fn rasterize_layers(&self, glyph: u16, layers: &[Layer], px: f32) -> (Metrics, Vec<u32>) {
		let outlines: Vec<_> = layers
			.iter()
			.map(|layer| (self.outline(layer.glyph, px), layer.color))
			.filter(|((metrics, _), _)| metrics.width > 0 && metrics.height > 0)
			.collect();
		let advance_width = f32::from(self.face.advance(glyph)) * self.scale(px);
		let boxes = outlines.iter().map(|((m, _), _)| m);
		let (Some(left), Some(bottom), Some(right), Some(top)) = (
			boxes.clone().map(|m| m.xmin).min(),
			boxes.clone().map(|m| m.ymin).min(),
			boxes.clone().map(|m| m.xmin + m.width as i32).max(),
			boxes.map(|m| m.ymin + m.height as i32).max(),
		) else {
			let metrics = Metrics {
				advance_width,
				..Metrics::default()
			};
			return (metrics, Vec::new());
		};
		let metrics = Metrics {
			xmin: left,
			ymin: bottom,
			// Each layer's box is bounded, but not where it lies.
			width: (right - left).clamp(0, 4096) as usize,
			height: (top - bottom).clamp(0, 4096) as usize,
			advance_width,
		};
		let mut pixels = vec![0; metrics.width * metrics.height];
		for ((_, path), color) in &outlines {
			let mut raster = Raster::new(metrics.width, metrics.height);
			raster.draw(path, self.scale(px), left as f32, top as f32);
			colr::paint(
				&mut pixels,
				&raster.coverage(),
				color.unwrap_or(LAYER_TEXT_COLOR),
			);
		}
		(metrics, pixels)
	}

	/// The one glyph this font forms from a cluster of several `chars`, such
	/// as an emoji ZWJ sequence or a keycap, through its ligatures; tried
	/// again without emoji variation selectors, which sequences may leave
	/// out. `None` when the font draws them as separate glyphs.
	pub(crate) fn ligature(&self, chars: &[char]) -> Option<u16> {
		let one = |chars: &[char]| match self.face.shape(chars)[..] {
			[glyph] if glyph != 0 && chars.len() > 1 => Some(glyph),
			_ => None,
		};
		one(chars).or_else(|| {
			let plain: Vec<char> = chars.iter().copied().filter(|&c| c != '\u{fe0f}').collect();
			(plain.len() < chars.len()).then(|| one(&plain)).flatten()
		})
	}

	#[cfg(test)]
	pub(crate) fn color_bitmap(&self, ch: char) -> Option<super::cbdt::ColorBitmap<'_>> {
		self.face.color_bitmap(self.face.glyph_index(ch))
	}

	/// Whether `ch` is a color bitmap or `COLR` glyph in this font.
	pub(crate) fn has_color(&self, ch: char) -> bool {
		let glyph = self.face.glyph_index(ch);
		self.face.color_bitmap(glyph).is_some() || self.face.color_layers(glyph).is_some()
	}

	/// The glyph's outline as quadratic curves in font units, cubics split
	/// within 1/4096 em.
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "the GPU outline store reads glyphs next (#21)")
	)]
	pub(crate) fn quads(&self, glyph: u16) -> Vec<Quad> {
		let mut quads = Quads::new(f32::from(self.face.units_per_em) / 4096.0);
		self.face.outline(glyph, &mut quads);
		quads.finish()
	}

	#[cfg(test)]
	pub(crate) fn outline_into(&self, glyph: u16, sink: &mut dyn super::face::Sink) {
		self.face.outline(glyph, sink);
	}

	#[cfg(test)]
	pub(crate) fn units_per_em(&self) -> f32 {
		f32::from(self.face.units_per_em)
	}

	pub(crate) fn rasterize(&self, ch: char, px: f32) -> (Metrics, Vec<u8>) {
		self.rasterize_glyph(self.face.glyph_index(ch), px)
	}

	/// [`Self::rasterize`] for a glyph by its index.
	pub(crate) fn rasterize_glyph(&self, glyph: u16, px: f32) -> (Metrics, Vec<u8>) {
		let (metrics, path) = self.outline(glyph, px);
		let mut raster = Raster::new(metrics.width, metrics.height);
		let top = (metrics.ymin + metrics.height as i32) as f32;
		raster.draw(&path, self.scale(px), metrics.xmin as f32, top);
		(metrics, raster.coverage())
	}

	/// The glyph's outline and its pixel box: the bounds scaled and
	/// rounded outward.
	fn outline(&self, glyph: u16, px: f32) -> (Metrics, Path) {
		let scale = self.scale(px);
		let mut path = Path::default();
		let advance_width = f32::from(self.face.advance(glyph)) * scale;
		let outlined = self.face.outline(glyph, &mut path);
		let Some(bounds) = outlined.and_then(|_| path.bounds()) else {
			let metrics = Metrics {
				advance_width,
				..Metrics::default()
			};
			return (metrics, path);
		};
		let (left, bottom) = (
			(bounds.x_min * scale).floor(),
			(bounds.y_min * scale).floor(),
		);
		let (right, top) = ((bounds.x_max * scale).ceil(), (bounds.y_max * scale).ceil());
		let metrics = Metrics {
			xmin: left as i32,
			ymin: bottom as i32,
			width: (right - left).clamp(0.0, 4096.0) as usize,
			height: (top - bottom).clamp(0.0, 4096.0) as usize,
			advance_width,
		};
		(metrics, path)
	}
}
