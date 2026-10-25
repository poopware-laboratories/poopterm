//! Color bitmap glyphs: `CBLC` says where each glyph's image is, `CBDT`
//! holds the PNG data, as in Noto Color Emoji. Written from the OpenType
//! specification; index formats 1 and 3 with image formats 17 and 18.
use super::{face::Table, read::Bytes};

/// More strikes or index subtables than any real font has.
const MAX_STRIKES: usize = 64;
const MAX_SUBTABLES: usize = 4096;

/// The color bitmap tables of a face.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cbdt {
	pub cblc: Table,
	pub cbdt: Table,
}

/// A glyph's PNG and its placement, in pixels of a strike of `ppem` pixels
/// per em.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ColorBitmap<'a> {
	pub ppem: u8,
	pub width: u8,
	pub height: u8,
	/// From the pen to the left edge.
	pub left: i8,
	/// From the baseline up to the top edge.
	pub top: i8,
	pub advance: u8,
	pub png: &'a [u8],
}

impl Cbdt {
	/// The bitmap of `glyph` in the largest strike that has it.
	pub(crate) fn glyph<'a>(&self, data: &'a [u8], glyph: u16) -> Option<ColorBitmap<'a>> {
		let (cblc, cbdt) = (self.cblc.bytes(data), self.cbdt.bytes(data));
		let strikes = (cblc.u32_at(4)? as usize).min(MAX_STRIKES);
		(0..strikes)
			.filter_map(|strike| {
				let at = 8 + 48 * strike;
				let (first, last) = (cblc.u16_at(at + 40)?, cblc.u16_at(at + 42)?);
				let ppem = cblc.u8_at(at + 44)?;
				(first..=last).contains(&glyph).then_some((ppem, at))
			})
			.max_by_key(|&(ppem, _)| ppem)
			.and_then(|(ppem, at)| locate(cblc, cbdt, at, glyph, ppem))
	}
}

/// Finds `glyph` through the index subtables of the strike record at `at`.
fn locate<'a>(
	cblc: &[u8],
	cbdt: &'a [u8],
	at: usize,
	glyph: u16,
	ppem: u8,
) -> Option<ColorBitmap<'a>> {
	let array = cblc.u32_at(at)? as usize;
	let subtables = (cblc.u32_at(at + 8)? as usize).min(MAX_SUBTABLES);
	let (first, offset) = (0..subtables).find_map(|i| {
		let record = array + 8 * i;
		let (first, last) = (cblc.u16_at(record)?, cblc.u16_at(record + 2)?);
		let offset = cblc.u32_at(record + 4)? as usize;
		(first..=last)
			.contains(&glyph)
			.then_some((first, array + offset))
	})?;
	let (index_format, image_format) = (cblc.u16_at(offset)?, cblc.u16_at(offset + 2)?);
	let base = cblc.u32_at(offset + 4)? as usize;
	let i = usize::from(glyph - first);
	let (start, end) = match index_format {
		1 => (
			cblc.u32_at(offset + 8 + 4 * i)?,
			cblc.u32_at(offset + 12 + 4 * i)?,
		),
		3 => (
			u32::from(cblc.u16_at(offset + 8 + 2 * i)?),
			u32::from(cblc.u16_at(offset + 10 + 2 * i)?),
		),
		_ => return None,
	};
	// Equal offsets mean the glyph has no bitmap.
	let image = cbdt.slice_at(base + start as usize, end.checked_sub(start)? as usize)?;
	let metrics = match image_format {
		17 => 5,
		18 => 8,
		_ => return None,
	};
	let length = image.u32_at(metrics)? as usize;
	Some(ColorBitmap {
		ppem,
		height: image.u8_at(0)?,
		width: image.u8_at(1)?,
		left: image.u8_at(2)? as i8,
		top: image.u8_at(3)? as i8,
		advance: image.u8_at(4)?,
		png: image.slice_at(metrics + 4, length)?,
	})
}
