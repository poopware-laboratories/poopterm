//! One font face loaded in memory: its table directory, global metrics,
//! character map and horizontal metrics, and its glyph outlines (`glyf` or
//! `CFF `). Written from the OpenType specification.
use std::borrow::Cow;

use super::{
	cbdt::{Cbdt, ColorBitmap},
	cff::Cff,
	cmap::Cmap,
	glyf,
	gsub::{Gsub, MAX_GLYPHS},
	otl::Gdef,
	read::Bytes,
};

/// Receives a glyph outline in font units, y up.
pub(crate) trait Sink {
	fn move_to(&mut self, x: f32, y: f32);
	fn line_to(&mut self, x: f32, y: f32);
	fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32);
	fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32);
	fn close(&mut self);
}

/// A glyph's bounding box in font units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Bounds {
	pub x_min: f32,
	pub y_min: f32,
	pub x_max: f32,
	pub y_max: f32,
}

impl Bounds {
	/// Grows `bounds` to contain the point.
	pub(crate) fn include(bounds: &mut Option<Self>, x: f32, y: f32) {
		let b = bounds.get_or_insert(Self {
			x_min: x,
			y_min: y,
			x_max: x,
			y_max: y,
		});
		b.x_min = b.x_min.min(x);
		b.y_min = b.y_min.min(y);
		b.x_max = b.x_max.max(x);
		b.y_max = b.y_max.max(y);
	}
}

/// Where a table lies in the font data.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Table {
	pub offset: usize,
	pub len: usize,
}

impl Table {
	pub(crate) fn bytes<'a>(&self, data: &'a [u8]) -> &'a [u8] {
		data
			.get(self.offset..self.offset + self.len)
			.unwrap_or_default()
	}
}

pub(crate) struct Face {
	pub(crate) data: Cow<'static, [u8]>,
	pub(crate) units_per_em: u16,
	pub(crate) ascender: i16,
	pub(crate) descender: i16,
	pub(crate) line_gap: i16,
	pub(crate) glyphs: u16,
	h_metrics: u16,
	hmtx: Table,
	cmap: Option<Cmap>,
	pub(crate) loca: Table,
	pub(crate) glyf: Table,
	pub(crate) long_loca: bool,
	cff: Option<Cff>,
	cbdt: Option<Cbdt>,
	gsub: Option<Gsub>,
	gdef: Option<Gdef>,
}

/// Why a font could not be loaded.
pub(crate) type Error = &'static str;

impl Face {
	/// Parses face `index` of `data` (0 unless it is a collection).
	pub(crate) fn parse(data: Cow<'static, [u8]>, index: u32) -> Result<Self, Error> {
		let bytes: &[u8] = &data;
		let start = if bytes.slice_at(0, 4) == Some(b"ttcf") {
			let count = bytes.u32_at(8).ok_or("truncated collection")?;
			if index >= count {
				return Err("no such face in the collection");
			}
			bytes
				.u32_at(12 + 4 * index as usize)
				.ok_or("truncated collection")? as usize
		} else {
			0
		};
		let tag = bytes.slice_at(start, 4).ok_or("not a font")?;
		if !matches!(tag, [0, 1, 0, 0] | b"OTTO" | b"true") {
			return Err("not a TrueType or OpenType font");
		}
		let count = usize::from(bytes.u16_at(start + 4).ok_or("truncated")?);
		let find = |name: &[u8; 4]| {
			(0..count).find_map(|i| {
				let at = start + 12 + 16 * i;
				(bytes.slice_at(at, 4)? == name).then_some(())?;
				let table = Table {
					offset: bytes.u32_at(at + 8)? as usize,
					len: bytes.u32_at(at + 12)? as usize,
				};
				bytes.slice_at(table.offset, table.len).map(|_| table)
			})
		};
		let head = find(b"head").ok_or("missing head table")?.bytes(bytes);
		let hhea = find(b"hhea").ok_or("missing hhea table")?.bytes(bytes);
		let maxp = find(b"maxp").ok_or("missing maxp table")?.bytes(bytes);
		let units_per_em = head
			.u16_at(18)
			.filter(|&units| units >= 16)
			.ok_or("bad unitsPerEm")?;
		let (mut ascender, mut descender, mut line_gap) = (
			hhea.i16_at(4).unwrap_or(0),
			hhea.i16_at(6).unwrap_or(0),
			hhea.i16_at(8).unwrap_or(0),
		);
		if let Some(os2) = find(b"OS/2").map(|table| table.bytes(bytes)) {
			// USE_TYPO_METRICS, or a font without hhea metrics.
			let use_typo = os2
				.u16_at(62)
				.is_some_and(|selection| selection & 1 << 7 != 0);
			if (use_typo || (ascender == 0 && descender == 0))
				&& let (Some(a), Some(d), Some(g)) = (os2.i16_at(68), os2.i16_at(70), os2.i16_at(72))
			{
				(ascender, descender, line_gap) = (a, d, g);
			}
		}
		let cff = match (find(b"CFF "), find(b"CFF2")) {
			(Some(table), _) => Some(Cff::parse(bytes, table, false)?),
			(None, Some(table)) => Some(Cff::parse(bytes, table, true)?),
			(None, None) => None,
		};
		let glyf = find(b"glyf").unwrap_or_default();
		let cbdt = match (find(b"CBLC"), find(b"CBDT")) {
			(Some(cblc), Some(cbdt)) => Some(Cbdt { cblc, cbdt }),
			_ => None,
		};
		if cff.is_none() && glyf.len == 0 && cbdt.is_none() {
			return Err("no glyf or CFF outlines or color bitmaps");
		}
		let face = Self {
			units_per_em,
			ascender,
			descender,
			line_gap,
			glyphs: maxp.u16_at(4).unwrap_or(0),
			h_metrics: hhea.u16_at(34).unwrap_or(0),
			hmtx: find(b"hmtx").unwrap_or_default(),
			cmap: find(b"cmap").and_then(|table| Cmap::parse(bytes, table.offset)),
			loca: find(b"loca").unwrap_or_default(),
			glyf,
			long_loca: head.i16_at(50) == Some(1),
			cff,
			cbdt,
			gsub: find(b"GSUB").and_then(|table| Gsub::parse(bytes, table)),
			gdef: find(b"GDEF").and_then(|table| Gdef::parse(bytes, table)),
			data: Cow::Borrowed(&[]),
		};
		Ok(Self { data, ..face })
	}

	#[cfg(test)]
	pub(crate) fn is_cff(&self) -> bool {
		self.cff.is_some()
	}

	#[cfg(test)]
	pub(crate) fn is_cff2(&self) -> bool {
		self.cff.as_ref().is_some_and(|cff| cff.version2)
	}

	/// The glyph for `ch`; 0 (`.notdef`) when the font lacks it.
	pub(crate) fn glyph_index(&self, ch: char) -> u16 {
		let glyph = self
			.cmap
			.map_or(0, |cmap| cmap.glyph(&self.data, ch as u32));
		if glyph < self.glyphs { glyph } else { 0 }
	}

	/// The glyphs for a cluster of `chars`, after the substitutions of the
	/// glyph composition and ligature features; at most [`MAX_GLYPHS`].
	pub(crate) fn shape(&self, chars: &[char]) -> Vec<u16> {
		let mut glyphs: Vec<u16> = chars
			.iter()
			.take(MAX_GLYPHS)
			.map(|&ch| self.glyph_index(ch))
			.collect();
		if let Some(gsub) = &self.gsub {
			gsub.apply(&self.data, self.gdef.as_ref(), &mut glyphs);
		}
		glyphs
	}

	/// Advance width of `glyph` in font units. Glyphs past the last metric
	/// share its advance (monospace fonts store only one).
	pub(crate) fn advance(&self, glyph: u16) -> u16 {
		let hmtx = self.hmtx.bytes(&self.data);
		let index = glyph.min(self.h_metrics.saturating_sub(1));
		hmtx.u16_at(4 * usize::from(index)).unwrap_or(0)
	}

	/// `glyph`'s color bitmap, if the face has one for it.
	pub(crate) fn color_bitmap(&self, glyph: u16) -> Option<ColorBitmap<'_>> {
		self.cbdt?.glyph(&self.data, glyph)
	}

	/// Sends `glyph`'s outline to `sink` and returns its bounds, or `None`
	/// for empty or malformed glyphs.
	pub(crate) fn outline(&self, glyph: u16, sink: &mut dyn Sink) -> Option<Bounds> {
		if glyph >= self.glyphs {
			return None;
		}
		match &self.cff {
			Some(cff) => cff.outline(&self.data, glyph, sink),
			None => glyf::outline(self, glyph, sink),
		}
	}
}
