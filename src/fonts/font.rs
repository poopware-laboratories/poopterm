//! A loaded font at the interface the renderer uses: glyph lookup, line
//! and glyph metrics in pixels, and rasterized coverage bitmaps.
use std::borrow::Cow;

use super::{
	face::{Error, Face},
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

pub(crate) struct Font {
	face: Face,
}

impl Font {
	/// Loads face `index` of a font file or collection.
	pub(crate) fn from_bytes(data: impl Into<Cow<'static, [u8]>>, index: u32) -> Result<Self, Error> {
		Face::parse(data.into(), index).map(|face| Self { face })
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
		self.outline(ch, px).0
	}

	/// `ch` as color pixels (`0xAARRGGBB`, straight alpha), for fonts that
	/// store it as a color bitmap.
	pub(crate) fn rasterize_color(&self, ch: char, px: f32) -> Option<(Metrics, Vec<u32>)> {
		let bitmap = self.face.color_bitmap(self.face.glyph_index(ch))?;
		super::color::render(&bitmap, px)
	}

	#[cfg(test)]
	pub(crate) fn color_bitmap(&self, ch: char) -> Option<super::cbdt::ColorBitmap<'_>> {
		self.face.color_bitmap(self.face.glyph_index(ch))
	}

	/// Whether `ch` is a color bitmap in this font.
	pub(crate) fn has_color(&self, ch: char) -> bool {
		self.face.color_bitmap(self.face.glyph_index(ch)).is_some()
	}

	pub(crate) fn rasterize(&self, ch: char, px: f32) -> (Metrics, Vec<u8>) {
		let (metrics, path) = self.outline(ch, px);
		let mut raster = Raster::new(metrics.width, metrics.height);
		let top = (metrics.ymin + metrics.height as i32) as f32;
		raster.draw(&path, self.scale(px), metrics.xmin as f32, top);
		(metrics, raster.coverage())
	}

	/// The glyph's outline and its pixel box: the bounds scaled and
	/// rounded outward.
	fn outline(&self, ch: char, px: f32) -> (Metrics, Path) {
		let (glyph, scale) = (self.face.glyph_index(ch), self.scale(px));
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
