//! Character to glyph mapping (`cmap`): choosing the best Unicode subtable
//! and looking characters up in formats 0, 4, 6 and 12.
use super::{
	coverage::{Ranges, ranges},
	read::Bytes,
};

/// A Unicode subtable inside the font data: its offset and format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cmap {
	offset: usize,
	format: u16,
}

impl Cmap {
	/// The best supported Unicode subtable of the `cmap` table at `table`:
	/// full repertoire (format 12) before the Basic Multilingual Plane.
	pub(crate) fn parse(data: &[u8], table: usize) -> Option<Self> {
		let count = usize::from(data.u16_at(table + 2)?);
		let mut best: Option<(u8, Self)> = None;
		for record in 0..count.min(256) {
			let at = table + 4 + 8 * record;
			let (platform, encoding) = (data.u16_at(at)?, data.u16_at(at + 2)?);
			let rank = match (platform, encoding) {
				(3, 10) | (0, 4 | 6) => 0,
				(3, 1) | (0, 0..=3) => 1,
				(3, 0) => 2,
				_ => continue,
			};
			let offset = table + data.u32_at(at + 4)? as usize;
			let Some(format) = data.u16_at(offset) else {
				continue;
			};
			if matches!(format, 0 | 4 | 6 | 12) && best.is_none_or(|(old, _)| rank < old) {
				best = Some((rank, Self { offset, format }));
			}
		}
		best.map(|(_, cmap)| cmap)
	}

	/// The glyph for `ch`, 0 (`.notdef`) when unmapped or malformed.
	pub(crate) fn glyph(&self, data: &[u8], ch: u32) -> u16 {
		let table = &data[self.offset.min(data.len())..];
		match self.format {
			0 => (ch < 256)
				.then(|| table.u8_at(6 + ch as usize))
				.flatten()
				.map_or(0, u16::from),
			4 => format4_glyph(table, ch).unwrap_or(0),
			6 => format6_glyph(table, ch).unwrap_or(0),
			12 => format12_glyph(table, ch).unwrap_or(0),
			_ => 0,
		}
	}
}

impl Cmap {
	/// The characters this subtable maps to a glyph other than 0 and below
	/// `glyphs` (`maxp.numGlyphs`), so exactly those [`Cmap::glyph`] draws.
	/// Sorted tables are walked segment by segment, which is what the
	/// lookup's binary search finds for each character; others look every
	/// listed character up, merged and clamped to Unicode first, which
	/// bounds the walk for malformed tables.
	pub(crate) fn coverage(&self, data: &[u8], glyphs: u16) -> Ranges {
		let table = &data[self.offset.min(data.len())..];
		let mut runs: Ranges = Vec::new();
		let mut add = |ch: u32, glyph: u16| {
			if glyph == 0 || glyph >= glyphs {
				return;
			}
			match runs.last_mut() {
				Some((_, end)) if *end + 1 == ch => *end = ch,
				_ => runs.push((ch, ch)),
			}
		};
		let segments = (self.format == 4).then(|| sorted_segments(table)).flatten();
		let groups = (self.format == 12).then(|| sorted_groups(table)).flatten();
		if let Some(segments) = segments {
			// Segment i maps the characters after the previous segment's
			// end; the last segment, 0xFFFF alone, only ends the table.
			let mut after = 0;
			for (i, &(start, end)) in segments.iter().enumerate() {
				if start != 0xffff {
					for ch in u32::from(start).max(after)..=u32::from(end) {
						add(
							ch,
							segment_glyph(table, segments.len(), i, ch as u16).unwrap_or(0),
						);
					}
				}
				after = u32::from(end) + 1;
			}
		} else if let Some(groups) = groups {
			for (first, last, glyph) in groups {
				for ch in first..=last.min(LAST) {
					add(
						ch,
						u16::try_from(glyph.wrapping_add(ch - first)).unwrap_or(0),
					);
				}
			}
		} else {
			for (first, last) in ranges(self.listed(table)) {
				for ch in first..=last {
					add(ch, self.glyph(data, ch));
				}
			}
		}
		runs
	}

	/// The ranges of characters the subtable lists; it may map some of
	/// them to glyph 0.
	fn listed(&self, table: &[u8]) -> Vec<(u32, u32)> {
		match self.format {
			0 => vec![(0, 0xff)],
			4 => segments(table)
				.into_iter()
				// The last segment, 0xFFFF alone, only ends the table.
				.filter(|&(first, _)| first != 0xffff)
				.map(|(first, last)| (first.into(), last.into()))
				.collect(),
			6 => match (table.u16_at(6), table.u16_at(8)) {
				(Some(first), Some(count @ 1..)) => {
					vec![(first.into(), u32::from(first) + u32::from(count) - 1)]
				}
				_ => Vec::new(),
			},
			12 => groups(table)
				.into_iter()
				.map(|(first, last, _)| (first, last))
				.collect(),
			_ => Vec::new(),
		}
	}
}

/// The last code point.
const LAST: u32 = 0x10_ffff;

/// The `(start, end)` of each format 4 segment, up to the first that cannot
/// be read.
fn segments(table: &[u8]) -> Vec<(u16, u16)> {
	let count = table.u16_at(6).map_or(0, |count| usize::from(count / 2));
	let bounds = |i| {
		Some((
			table.u16_at(16 + 2 * count + 2 * i)?,
			table.u16_at(14 + 2 * i)?,
		))
	};
	(0..count).map_while(bounds).collect()
}

/// [`segments`] when all can be read and their ends never decrease, so the
/// lookup's binary search finds segment i for exactly the characters after
/// the previous end up to its own.
fn sorted_segments(table: &[u8]) -> Option<Vec<(u16, u16)>> {
	let all = segments(table);
	let count = table.u16_at(6).map_or(0, |count| usize::from(count / 2));
	let sorted = all.len() == count && all.is_sorted_by_key(|&(_, end)| end);
	sorted.then_some(all)
}

/// The `(first, last, first glyph)` of each format 12 group present; reads
/// are bounds-checked, and the limit only keeps a lying count from looping
/// billions of times.
fn groups(table: &[u8]) -> Vec<(u32, u32, u32)> {
	let count = table.u32_at(12).unwrap_or(0) as usize;
	let count = count.min(table.len().saturating_sub(16) / 12);
	let group = |i| {
		let at = 16 + 12 * i;
		Some((
			table.u32_at(at)?,
			table.u32_at(at + 4)?,
			table.u32_at(at + 8)?,
		))
	};
	(0..count).filter_map(group).collect()
}

/// [`groups`] when each is in order and after the one before, so the
/// lookup finds each group for exactly its own characters.
fn sorted_groups(table: &[u8]) -> Option<Vec<(u32, u32, u32)>> {
	let all = groups(table);
	let mut after = 0;
	for &(first, last, _) in &all {
		if first < after || first > last {
			return None;
		}
		after = last.saturating_add(1);
	}
	Some(all)
}

/// The glyph a format 4 subtable maps `ch` to; `None` if malformed.
pub(crate) fn format4_glyph(table: &[u8], ch: u32) -> Option<u16> {
	let ch = u16::try_from(ch).ok()?;
	let segments = usize::from(table.u16_at(6)? / 2);
	let (ends, starts) = (14, 16 + 2 * segments);
	// endCode is sorted: binary search for the first segment ending at or
	// after `ch`.
	let (mut low, mut high) = (0, segments);
	while low < high {
		let mid = (low + high) / 2;
		if table.u16_at(ends + 2 * mid)? < ch {
			low = mid + 1;
		} else {
			high = mid;
		}
	}
	let segment = low;
	if segment == segments {
		return Some(0);
	}
	let start = table.u16_at(starts + 2 * segment)?;
	if ch < start {
		return Some(0);
	}
	segment_glyph(table, segments, segment, ch)
}

/// The glyph `segment` of a format 4 subtable with `segments` segments maps
/// `ch` to, for `ch` from the segment's start on; `None` if malformed.
fn segment_glyph(table: &[u8], segments: usize, segment: usize, ch: u16) -> Option<u16> {
	let starts = 16 + 2 * segments;
	let (deltas, ranges) = (starts + 2 * segments, starts + 4 * segments);
	let start = table.u16_at(starts + 2 * segment)?;
	let delta = table.u16_at(deltas + 2 * segment)?;
	let range = table.u16_at(ranges + 2 * segment)?;
	if range == 0 {
		return Some(ch.wrapping_add(delta));
	}
	let at = ranges + 2 * segment + usize::from(range) + 2 * usize::from(ch.checked_sub(start)?);
	Some(match table.u16_at(at)? {
		0 => 0,
		glyph => glyph.wrapping_add(delta),
	})
}

fn format6_glyph(table: &[u8], ch: u32) -> Option<u16> {
	let (first, count) = (u32::from(table.u16_at(6)?), u32::from(table.u16_at(8)?));
	let index = ch.checked_sub(first).filter(|&index| index < count)?;
	table.u16_at(10 + 2 * index as usize)
}

/// The glyph a format 12 subtable maps `ch` to; groups are sorted, so a
/// binary search finds the one containing `ch`.
pub(crate) fn format12_glyph(table: &[u8], ch: u32) -> Option<u16> {
	let groups = table.u32_at(12)? as usize;
	let groups = groups.min(table.len().saturating_sub(16) / 12);
	let (mut low, mut high) = (0, groups);
	while low < high {
		let mid = (low + high) / 2;
		let at = 16 + 12 * mid;
		let (first, last) = (table.u32_at(at)?, table.u32_at(at + 4)?);
		if ch < first {
			high = mid;
		} else if ch > last {
			low = mid + 1;
		} else {
			let glyph = table.u32_at(at + 8)?.wrapping_add(ch - first);
			return Some(u16::try_from(glyph).unwrap_or(0));
		}
	}
	Some(0)
}
