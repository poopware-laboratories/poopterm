//! Layered color glyphs from `COLR` version 0, colored from palette 0 of
//! `CPAL`, and the source-over painting that stacks their layers. Written
//! from the OpenType specification; the version 1 paint graph is not read,
//! only the version 0 records a version 1 table still carries.
use super::{face::Table, read::Bytes};

/// More layers than any real glyph has: each one is rasterized.
pub(crate) const MAX_LAYERS: usize = 64;
/// The palette index of layers drawn in the text color.
const TEXT_COLOR: u16 = 0xffff;

/// The `COLR` table of a face and its `CPAL`, if it has one.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Colr {
	pub colr: Table,
	pub cpal: Option<Table>,
}

/// One layer of a color glyph, bottom first: an outline glyph and its color
/// as `0xAARRGGBB`, or `None` for the text color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Layer {
	pub glyph: u16,
	pub color: Option<u32>,
}

impl Colr {
	/// The layers of `glyph`, one of `glyphs`; `None` when it has no base
	/// glyph record, or its record, layers or colors are malformed.
	pub(crate) fn layers(&self, data: &[u8], glyph: u16, glyphs: u16) -> Option<Vec<Layer>> {
		let colr = self.colr.bytes(data);
		if colr.u16_at(0)? > 1 || glyph >= glyphs {
			return None;
		}
		let (first, count) = base_record(colr, glyph)?;
		let (layers_at, layer_count) = (colr.u32_at(8)? as usize, usize::from(colr.u16_at(12)?));
		if count == 0 || count > MAX_LAYERS || first + count > layer_count {
			return None;
		}
		let cpal = self.cpal.map(|table| table.bytes(data));
		(first..first + count)
			.map(|i| {
				let at = layers_at.checked_add(4 * i)?;
				let (glyph, index) = (colr.u16_at(at)?, colr.u16_at(at + 2)?);
				let color = match index {
					TEXT_COLOR => None,
					_ => Some(palette_color(cpal?, index)?),
				};
				(glyph < glyphs).then_some(Layer { glyph, color })
			})
			.collect()
	}
}

/// The first layer and layer count of `glyph`'s base record, found by
/// binary search: the records are sorted by glyph.
fn base_record(colr: &[u8], glyph: u16) -> Option<(usize, usize)> {
	let (records, at) = (usize::from(colr.u16_at(2)?), colr.u32_at(4)? as usize);
	let (mut low, mut high) = (0, records);
	while low < high {
		let middle = (low + high) / 2;
		let record = at.checked_add(6 * middle)?;
		match colr.u16_at(record)?.cmp(&glyph) {
			std::cmp::Ordering::Less => low = middle + 1,
			std::cmp::Ordering::Greater => high = middle,
			std::cmp::Ordering::Equal => {
				let first = usize::from(colr.u16_at(record + 2)?);
				return Some((first, usize::from(colr.u16_at(record + 4)?)));
			}
		}
	}
	None
}

/// Entry `index` of palette 0 as `0xAARRGGBB`; `None` past the palette or
/// the color records.
fn palette_color(cpal: &[u8], index: u16) -> Option<u32> {
	if cpal.u16_at(0)? > 1 || index >= cpal.u16_at(2)? || cpal.u16_at(4)? == 0 {
		return None;
	}
	let records = cpal.u16_at(6)?;
	let record = cpal
		.u16_at(12)?
		.checked_add(index)
		.filter(|&r| r < records)?;
	let at = (cpal.u32_at(8)? as usize).checked_add(4 * usize::from(record))?;
	let [blue, green, red, alpha] = cpal.slice_at(at, 4)?.try_into().ok()?;
	Some(u32::from_be_bytes([alpha, red, green, blue]))
}

/// Paints `color` (`0xAARRGGBB`) through `coverage` over `pixels`, with
/// straight alpha on both sides.
pub(crate) fn paint(pixels: &mut [u32], coverage: &[u8], color: u32) {
	let channel = |value: u32, shift: u32| (value >> shift & 0xff) as f32 / 255.0;
	let source_alpha = channel(color, 24);
	for (pixel, &cover) in pixels.iter_mut().zip(coverage) {
		let over = source_alpha * f32::from(cover) / 255.0;
		if over == 0.0 {
			continue;
		}
		let under = channel(*pixel, 24) * (1.0 - over);
		let alpha = over + under;
		let mix = |shift| {
			let value = (channel(color, shift) * over + channel(*pixel, shift) * under) / alpha;
			((value * 255.0).round() as u32) << shift
		};
		*pixel = ((alpha * 255.0).round() as u32) << 24 | mix(16) | mix(8) | mix(0);
	}
}
