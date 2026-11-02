//! Color glyphs from the `sbix` table, as in Apple Color Emoji: strikes of
//! PNG images placed by their origin offsets. Written from the OpenType
//! specification; `png ` and `dupe` records, other graphic types skipped.
use super::{cbdt::ColorBitmap, face::Table, read::Bytes};

/// More strikes than any real font has.
const MAX_STRIKES: usize = 64;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The `sbix` table of a face.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sbix {
	pub table: Table,
}

/// A glyph record: origin offsets, graphic type and data.
struct Record<'a> {
	x: i16,
	y: i16,
	kind: &'a [u8],
	data: &'a [u8],
}

impl Sbix {
	/// The PNG of `glyph`, one of `glyphs`, in the largest strike that has
	/// one; it advances `advance` units of an em of `units_per_em`.
	pub(crate) fn glyph<'a>(
		&self,
		data: &'a [u8],
		glyph: u16,
		glyphs: u16,
		(advance, units_per_em): (u16, u16),
	) -> Option<ColorBitmap<'a>> {
		let table = self.table.bytes(data);
		if table.u16_at(0)? != 1 || glyph >= glyphs {
			return None;
		}
		let strikes = (table.u32_at(4)? as usize).min(MAX_STRIKES);
		let found = (0..strikes)
			.filter_map(|i| {
				let strike = table.get(table.u32_at(8 + 4 * i)? as usize..)?;
				in_strike(strike, glyph, glyphs)
			})
			.max_by_key(|bitmap| bitmap.ppem)?;
		let scaled = u32::from(advance) * u32::from(found.ppem);
		let units = u32::from(units_per_em.max(1));
		let advance = u16::try_from((scaled + units / 2) / units).ok()?;
		Some(ColorBitmap { advance, ..found })
	}
}

/// `glyph`'s PNG in `strike`, following one `dupe`; its advance is left 0.
fn in_strike(strike: &[u8], glyph: u16, glyphs: u16) -> Option<ColorBitmap<'_>> {
	let mut found = record(strike, glyph)?;
	if found.kind == b"dupe" {
		let target = found.data.u16_at(0)?;
		if target >= glyphs {
			return None;
		}
		found = record(strike, target)?;
	}
	if found.kind != b"png " {
		return None;
	}
	let (width, height) = png_size(found.data)?;
	Some(ColorBitmap {
		ppem: strike.u16_at(0)?,
		width,
		height,
		left: found.x,
		// The origin is the bottom left corner.
		top: found.y.checked_add(i16::try_from(height).ok()?)?,
		advance: 0,
		png: found.data,
	})
}

/// The record of `glyph`: between its offset and the next, at least the
/// eight bytes before the data.
fn record(strike: &[u8], glyph: u16) -> Option<Record<'_>> {
	let at = 4 + 4 * usize::from(glyph);
	let (start, end) = (strike.u32_at(at)?, strike.u32_at(at + 4)?);
	let length = end.checked_sub(start)? as usize;
	let bytes = strike
		.slice_at(start as usize, length)
		.filter(|b| b.len() >= 8)?;
	Some(Record {
		x: bytes.i16_at(0)?,
		y: bytes.i16_at(2)?,
		kind: &bytes[4..8],
		data: &bytes[8..],
	})
}

/// Width and height from a PNG's `IHDR`, which sbix does not repeat.
fn png_size(png: &[u8]) -> Option<(u16, u16)> {
	if png.slice_at(0, 8)? != PNG_SIGNATURE || png.slice_at(12, 4)? != b"IHDR" {
		return None;
	}
	let side = |at| u16::try_from(png.u32_at(at)?).ok();
	Some((side(16)?, side(20)?))
}
