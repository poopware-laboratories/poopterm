//! AAT lookup tables, the glyph-to-value maps `morx` uses for classes and
//! substitutions. Written from Apple's TrueType Reference Manual: formats
//! 0, 2, 4, 6, 8 and 10; other formats find nothing.
use super::read::Bytes;

/// The value for `glyph` in the lookup table at `at` in `data`, or `None`
/// when the table does not cover it or is malformed.
pub(super) fn lookup(data: &[u8], at: usize, glyph: u16) -> Option<u16> {
	let table = data.get(at..)?;
	match table.u16_at(0)? {
		0 => table.u16_at(2 + 2 * usize::from(glyph)),
		2 => segment(table, glyph, |unit, _| table.u16_at(unit + 4)),
		4 => segment(table, glyph, |unit, first| {
			let values = usize::from(table.u16_at(unit + 4)?);
			table.u16_at(values + 2 * usize::from(glyph - first))
		}),
		6 => single(table, glyph),
		8 => trimmed(table, glyph, (2, 2), |at| table.u16_at(at)),
		10 => {
			let size = usize::from(table.u16_at(2)?);
			trimmed(table, glyph, (4, size), |at| {
				// The low 16 bits, as a glyph or class needs.
				let value = table.slice_at(at, size)?;
				Some(value.iter().fold(0u16, |n, &byte| n << 8 | u16::from(byte)))
			})
		}
		_ => None,
	}
}

/// Formats 2 and 4: units of `last`, `first`, then data, sorted by `last`;
/// `value` reads the match from its unit's position and first glyph.
fn segment(table: &[u8], glyph: u16, value: impl Fn(usize, u16) -> Option<u16>) -> Option<u16> {
	let size = usize::from(table.u16_at(2)?);
	let count = usize::from(table.u16_at(4)?);
	let (mut low, mut high) = (0, count);
	while low < high {
		let middle = (low + high) / 2;
		let unit = 12 + size * middle;
		let (last, first) = (table.u16_at(unit)?, table.u16_at(unit + 2)?);
		if glyph > last {
			low = middle + 1;
		} else if glyph < first {
			high = middle;
		} else {
			return value(unit, first);
		}
	}
	None
}

/// Format 6: units of glyph and value, sorted by glyph.
fn single(table: &[u8], glyph: u16) -> Option<u16> {
	let size = usize::from(table.u16_at(2)?);
	let (mut low, mut high) = (0, usize::from(table.u16_at(4)?));
	while low < high {
		let middle = (low + high) / 2;
		let unit = 12 + size * middle;
		match table.u16_at(unit)?.cmp(&glyph) {
			std::cmp::Ordering::Less => low = middle + 1,
			std::cmp::Ordering::Greater => high = middle,
			std::cmp::Ordering::Equal => return table.u16_at(unit + 2),
		}
	}
	None
}

/// Formats 8 and 10: the first glyph and a count at `header`, then values
/// of `size` bytes, read by `read` at their offset.
fn trimmed(
	table: &[u8],
	glyph: u16,
	(header, size): (usize, usize),
	read: impl Fn(usize) -> Option<u16>,
) -> Option<u16> {
	let at = header;
	let (first, count) = (table.u16_at(at)?, table.u16_at(at + 2)?);
	let index = glyph.checked_sub(first).filter(|&i| i < count)?;
	read(at + 4 + size * usize::from(index))
}
