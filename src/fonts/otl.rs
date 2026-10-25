//! OpenType Layout common tables: coverage and class definitions, and the
//! glyph classes in `GDEF` that lookup flags skip glyphs by. Written from
//! the OpenType specification. Offsets are into the table that holds them.
use super::{face::Table, read::Bytes};

/// The coverage index of `glyph` in the coverage table at `at`.
pub(crate) fn coverage(table: &[u8], at: usize, glyph: u16) -> Option<u16> {
	let count = usize::from(table.u16_at(at + 2)?);
	match table.u16_at(at)? {
		1 => {
			let index = search(count, glyph, |i| {
				let id = table.u16_at(at + 4 + 2 * i)?;
				Some((id, id))
			})?;
			u16::try_from(index).ok()
		}
		2 => {
			let index = search(count, glyph, |i| {
				Some((table.u16_at(at + 4 + 6 * i)?, table.u16_at(at + 6 + 6 * i)?))
			})?;
			let (first, start) = (
				table.u16_at(at + 4 + 6 * index)?,
				table.u16_at(at + 8 + 6 * index)?,
			);
			start.checked_add(glyph - first)
		}
		_ => None,
	}
}

/// The class of `glyph` in the class definition table at `at`; 0 for
/// glyphs it does not list.
pub(crate) fn class(table: &[u8], at: usize, glyph: u16) -> u16 {
	let class = || match table.u16_at(at)? {
		1 => {
			let (start, count) = (table.u16_at(at + 2)?, table.u16_at(at + 4)?);
			let i = glyph.checked_sub(start).filter(|&i| i < count)?;
			table.u16_at(at + 6 + 2 * usize::from(i))
		}
		2 => {
			let count = usize::from(table.u16_at(at + 2)?);
			let index = search(count, glyph, |i| {
				Some((table.u16_at(at + 4 + 6 * i)?, table.u16_at(at + 6 + 6 * i)?))
			})?;
			table.u16_at(at + 8 + 6 * index)
		}
		_ => None,
	};
	class().unwrap_or(0)
}

/// Binary search over `count` sorted, disjoint glyph ranges, of which
/// `range` reads the `i`th; a font that breaks the order only misses.
fn search(count: usize, glyph: u16, range: impl Fn(usize) -> Option<(u16, u16)>) -> Option<usize> {
	let (mut low, mut high) = (0, count);
	while low < high {
		let middle = low + (high - low) / 2;
		let (first, last) = range(middle)?;
		if glyph < first {
			high = middle;
		} else if glyph > last {
			low = middle + 1;
		} else {
			return Some(middle);
		}
	}
	None
}

/// Lookup flags (`LookupFlag`) that make a lookup skip glyphs.
pub(crate) const IGNORE_BASE_GLYPHS: u16 = 0x0002;
pub(crate) const IGNORE_LIGATURES: u16 = 0x0004;
pub(crate) const IGNORE_MARKS: u16 = 0x0008;
pub(crate) const USE_MARK_FILTERING_SET: u16 = 0x0010;
pub(crate) const MARK_ATTACHMENT_TYPE: u16 = 0xff00;

/// Glyph classes in `GlyphClassDef`.
const BASE: u16 = 1;
const LIGATURE: u16 = 2;
const MARK: u16 = 3;

/// The glyph definition table: which glyphs are bases, ligatures and
/// marks, and the mark classes and sets lookups may filter by.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Gdef {
	table: Table,
	/// Offsets of the subtables, 0 for those the font leaves out.
	glyph_classes: usize,
	mark_classes: usize,
	mark_sets: usize,
}

impl Gdef {
	pub(crate) fn parse(data: &[u8], table: Table) -> Option<Self> {
		let bytes = table.bytes(data);
		let (major, minor) = (bytes.u16_at(0)?, bytes.u16_at(2)?);
		if major != 1 {
			return None;
		}
		let offset = |at| bytes.u16_at(at).map_or(0, usize::from);
		Some(Self {
			table,
			glyph_classes: offset(4),
			mark_classes: offset(10),
			mark_sets: if minor >= 2 { offset(12) } else { 0 },
		})
	}

	/// Whether a lookup with `flag` (and mark filtering set `set`) skips
	/// `glyph`.
	pub(crate) fn skips(&self, data: &[u8], flag: u16, set: u16, glyph: u16) -> bool {
		let bytes = self.table.bytes(data);
		let glyph_class = match self.glyph_classes {
			0 => 0,
			at => class(bytes, at, glyph),
		};
		match glyph_class {
			BASE => flag & IGNORE_BASE_GLYPHS != 0,
			LIGATURE => flag & IGNORE_LIGATURES != 0,
			MARK if flag & IGNORE_MARKS != 0 => true,
			MARK if flag & USE_MARK_FILTERING_SET != 0 => !self.in_set(bytes, set, glyph),
			MARK if flag & MARK_ATTACHMENT_TYPE != 0 => {
				let wanted = flag >> 8;
				self.mark_classes == 0 || class(bytes, self.mark_classes, glyph) != wanted
			}
			_ => false,
		}
	}

	/// Whether `glyph` is in mark glyph set `set`.
	fn in_set(&self, bytes: &[u8], set: u16, glyph: u16) -> bool {
		let at = self.mark_sets;
		let found = || {
			let count = bytes.u16_at(at + 2)?;
			(at != 0 && bytes.u16_at(at)? == 1 && set < count).then_some(())?;
			let offset = bytes.u32_at(at + 4 + 4 * usize::from(set))? as usize;
			coverage(bytes, at + offset, glyph)
		};
		found().is_some()
	}
}

#[cfg(test)]
#[path = "otl_tests.rs"]
pub(crate) mod tests;
