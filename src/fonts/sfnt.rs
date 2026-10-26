//! Reads the metadata of OpenType and TrueType fonts (`.ttf`, `.otf`, `.ttc`):
//! names, weight, slant and spacing, for the font database.
//!
//! Written from the OpenType specification. Only the table directory and the
//! small `name`, `OS/2`, `post`, `head`, `hhea` and `maxp` tables are read,
//! by seeking, plus `cmap` and, for fonts whose flags do not say monospace,
//! `hmtx`, so
//! listing a 20 MB CJK font costs a few kilobytes of I/O. Font files are
//! untrusted: every count and offset is bounds-checked and reads are capped.
use std::io::{self, Read, Seek, SeekFrom};

use super::{
	cmap::Cmap,
	coverage::{Ranges, Shared},
};

/// What the database needs to know about one face of a font file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FaceInfo {
	/// Position in a collection (`.ttc`); 0 for single fonts.
	pub index: u32,
	/// Typographic family when the font has one, so all weights group.
	pub family: String,
	pub style: String,
	pub full_name: String,
	pub postscript: String,
	/// CSS-style weight class, 1–1000 (400 regular, 700 bold).
	pub weight: u16,
	pub italic: bool,
	/// A fixed-pitch text font: every glyph has one width and ASCII letters
	/// and digits are mapped, which rules out emoji and symbol fonts.
	pub monospace: bool,
	/// Has glyphs it can draw: outlines (`glyf`, `CFF `, `CFF2`) or color
	/// bitmaps (`CBDT`).
	pub drawable: bool,
	/// Has color bitmaps (`CBDT`), as emoji fonts do.
	pub color: bool,
	/// The code points its `cmap` maps to a glyph other than 0.
	pub coverage: Shared,
}

/// More faces than any real collection has.
const MAX_FACES: u32 = 256;
const MAX_TABLES: u16 = 512;
/// Name tables of large CJK families reach a few hundred kilobytes.
const MAX_NAME_TABLE: u32 = 1 << 20;
/// `cmap` tables of the largest CJK fonts are under a megabyte.
const MAX_CMAP: u32 = 4 << 20;

/// Lists the faces in a font file. A collection skips faces it cannot read;
/// a single font that cannot be read is an error. Faces without a family
/// name are left out.
pub(crate) fn read_faces<R: Read + Seek>(file: &mut R) -> io::Result<Vec<FaceInfo>> {
	let header = read_at(file, 0, 12)?;
	if &header[..4] != b"ttcf" {
		return Ok(read_face(file, 0, 0)?.into_iter().collect());
	}
	let count = be32(&header, 8);
	if count == 0 || count > MAX_FACES {
		return Err(invalid("bad face count in font collection"));
	}
	let offsets = read_at(file, 12, 4 * count as usize)?;
	Ok(
		(0..count)
			.filter_map(|index| {
				let offset = be32(&offsets, 4 * index as usize);
				read_face(file, u64::from(offset), index).ok().flatten()
			})
			.collect(),
	)
}

fn read_face<R: Read + Seek>(file: &mut R, start: u64, index: u32) -> io::Result<Option<FaceInfo>> {
	let header = read_at(file, start, 12)?;
	if !matches!(&header[..4], [0, 1, 0, 0] | b"OTTO" | b"true") {
		return Err(invalid("not an OpenType font"));
	}
	let tables = be16(&header, 4);
	if tables > MAX_TABLES {
		return Err(invalid("too many tables"));
	}
	let records = read_at(file, start + 12, 16 * usize::from(tables))?;
	let find = |tag: &[u8; 4]| {
		records
			.as_chunks::<16>()
			.0
			.iter()
			.find(|record| &record[..4] == tag)
			.map(|record| (u64::from(be32(record, 8)), be32(record, 12)))
	};
	let mut table = |tag: &[u8; 4], cap: u32| -> io::Result<Vec<u8>> {
		match find(tag) {
			Some((offset, length)) => read_at(file, offset, length.min(cap) as usize),
			None => Ok(Vec::new()),
		}
	};
	let name = table(b"name", MAX_NAME_TABLE)?;
	let os2 = table(b"OS/2", 96)?;
	let post = table(b"post", 16)?;
	let head = table(b"head", 54)?;
	let hhea = table(b"hhea", 36)?;

	let names = Names::parse(&name);
	let Some(family) = names.get(16).or_else(|| names.get(1)) else {
		return Ok(None);
	};
	let style = names
		.get(17)
		.or_else(|| names.get(2))
		.unwrap_or_else(|| "Regular".to_owned());
	let full_name = names.get(4).unwrap_or_else(|| format!("{family} {style}"));
	let mac_style = if head.len() >= 46 { be16(&head, 44) } else { 0 };
	let weight = if os2.len() >= 6 {
		match be16(&os2, 4) {
			// Some old fonts use 1–9 instead of 100–900.
			weight @ 1..=9 => weight * 100,
			weight => weight.clamp(1, 1000),
		}
	} else if mac_style & 1 != 0 {
		700
	} else {
		400
	};
	let fs_selection = if os2.len() >= 64 { be16(&os2, 62) } else { 0 };
	let italic = fs_selection & (1 | 1 << 9) != 0 || mac_style & 2 != 0;
	// post.isFixedPitch. PANOSE is not trusted: patched proportional fonts
	// keep their source's "monospaced" byte.
	let fixed_pitch = post.len() >= 16 && be32(&post, 12) != 0;
	// Fonts like Roboto Mono do not set it; measure their glyphs instead.
	let fixed_width = fixed_pitch
		|| match (find(b"hmtx"), hhea.len() >= 36) {
			(Some((offset, length)), true) => {
				let metrics = u64::from(be16(&hhea, 34)) * 4;
				// A truncated table only costs the monospace guess.
				equal_advances(file, offset, metrics.min(u64::from(length))).unwrap_or(false)
			}
			_ => false,
		};
	// Emoji and symbol fonts are fixed pitch too; a terminal font needs text.
	// Glyphs at or past maxp.numGlyphs are drawn as .notdef; without maxp
	// the font does not load.
	let glyphs = find(b"maxp")
		.and_then(|(offset, length)| (length >= 6).then(|| read_at(file, offset, 6).ok())?)
		.map_or(0, |maxp| be16(&maxp, 4));
	let (latin, coverage) = find(b"cmap").map_or_else(Default::default, |(offset, length)| {
		read_cmap(file, offset, length, glyphs).unwrap_or_default()
	});
	let monospace = fixed_width && latin;
	let drawable = [b"glyf", b"CFF ", b"CFF2", b"CBDT"]
		.into_iter()
		.any(|tag| find(tag).is_some());
	Ok(Some(FaceInfo {
		index,
		drawable,
		color: find(b"CBDT").is_some(),
		coverage: coverage.into(),
		postscript: names.get(6).unwrap_or_default(),
		family,
		style,
		full_name,
		weight,
		italic,
		monospace,
	}))
}

/// The best string for each name ID in a `name` table.
struct Names<'a> {
	table: &'a [u8],
	/// Per name ID up to 17: (rank, offset, length, platform), lowest rank wins.
	best: [Option<(u8, usize, usize, u16)>; 18],
}

impl<'a> Names<'a> {
	fn parse(table: &'a [u8]) -> Self {
		let mut names = Self {
			table,
			best: [None; 18],
		};
		if table.len() < 6 {
			return names;
		}
		let (count, storage) = (usize::from(be16(table, 2)), usize::from(be16(table, 4)));
		for record in table[6..].as_chunks::<12>().0.iter().take(count) {
			let [platform, encoding, language, id, length, offset] =
				std::array::from_fn(|i| be16(record, 2 * i));
			let Some(rank) = rank(platform, encoding, language) else {
				continue;
			};
			let (start, length) = (storage + usize::from(offset), usize::from(length));
			let slot = names.best.get_mut(usize::from(id));
			if let Some(slot) = slot
				&& start + length <= table.len()
				&& slot.is_none_or(|(best, ..)| rank < best)
			{
				*slot = Some((rank, start, length, platform));
			}
		}
		names
	}

	/// Name `id` as text; `None` when missing or empty.
	fn get(&self, id: usize) -> Option<String> {
		let (_, start, length, platform) = self.best.get(id).copied().flatten()?;
		let bytes = &self.table[start..start + length];
		let text: String = if platform == 1 {
			// Mac Roman; font names are ASCII in practice, so read it as Latin-1.
			bytes.iter().map(|&byte| char::from(byte)).collect()
		} else {
			char::decode_utf16(
				bytes
					.as_chunks::<2>()
					.0
					.iter()
					.map(|&pair| u16::from_be_bytes(pair)),
			)
			.map(|ch| ch.unwrap_or(char::REPLACEMENT_CHARACTER))
			.collect()
		};
		let text = text.trim_matches(|ch: char| ch == '\0' || ch.is_whitespace());
		(!text.is_empty()).then(|| text.to_owned())
	}
}

/// Preference for a name record: Windows US English first, then other
/// English, Unicode, Mac Roman English, and any other Windows language.
fn rank(platform: u16, encoding: u16, language: u16) -> Option<u8> {
	match (platform, encoding, language) {
		(3, 0 | 1 | 10, 0x409) => Some(0),
		(3, 0 | 1 | 10, language) if language & 0xff == 0x09 => Some(1),
		(0, ..) => Some(2),
		(1, 0, 0) => Some(3),
		(3, 0 | 1 | 10, _) => Some(4),
		_ => None,
	}
}

/// Characters a terminal font must map: ASCII letters and digits.
fn latin() -> impl Iterator<Item = u32> {
	(u32::from(b'0')..=u32::from(b'9'))
		.chain(u32::from(b'A')..=u32::from(b'Z'))
		.chain(u32::from(b'a')..=u32::from(b'z'))
}

/// Whether the `cmap` table at `offset` maps every [`latin`] character to
/// a glyph, and the characters it maps to glyphs below `glyphs`, read from
/// the subtable the font engine uses, so the two agree exactly.
fn read_cmap<R: Read + Seek>(
	file: &mut R,
	offset: u64,
	length: u32,
	glyphs: u16,
) -> io::Result<(bool, Ranges)> {
	let table = read_at(file, offset, length.min(MAX_CMAP) as usize)?;
	let Some(cmap) = Cmap::parse(&table, 0) else {
		return Ok((false, Vec::new()));
	};
	let latin = latin().all(|ch| cmap.glyph(&table, ch) != 0);
	Ok((latin, cmap.coverage(&table, glyphs)))
}

/// Whether every non-zero advance width among the `len` bytes of `hmtx`
/// records at `offset` is the same, with at least one non-zero. Streams the
/// table in chunks, the first one small, so a proportional font is rejected
/// after reading about a kilobyte.
fn equal_advances<R: Read + Seek>(file: &mut R, offset: u64, len: u64) -> io::Result<bool> {
	let len = len - len % 4;
	let mut width = None;
	let (mut at, mut chunk) = (0, 1024);
	while at < len {
		let records = read_at(file, offset + at, chunk.min(len - at) as usize)?;
		width = width.or_else(|| {
			records
				.as_chunks::<4>()
				.0
				.iter()
				.map(|record| [record[0], record[1]])
				.find(|advance| *advance != [0, 0])
		});
		if let Some(width) = width
			&& !advances_match(&records, width)
		{
			return Ok(false);
		}
		at += chunk.min(len - at);
		chunk = 64 * 1024;
	}
	Ok(width.is_some())
}

/// SIMD check that each 4-byte `hmtx` record in `records` has advance 0 or
/// `width` (big-endian bytes), sixteen records per step.
fn advances_match(records: &[u8], width: [u8; 2]) -> bool {
	use std::simd::prelude::*;
	// The advance is a record's first two bytes, whatever the host order.
	let mask = u32x16::splat(u32::from_ne_bytes([0xff, 0xff, 0, 0]));
	let target = u32x16::splat(u32::from_ne_bytes([width[0], width[1], 0, 0]));
	let (chunks, rest) = records.as_chunks::<64>();
	let mut all = mask32x16::splat(true);
	for chunk in chunks {
		let lanes = u32x16::from_array(std::array::from_fn(|i| {
			u32::from_ne_bytes([
				chunk[4 * i],
				chunk[4 * i + 1],
				chunk[4 * i + 2],
				chunk[4 * i + 3],
			])
		}));
		let advance = lanes & mask;
		all &= advance.simd_eq(u32x16::splat(0)) | advance.simd_eq(target);
		if !all.all() {
			return false;
		}
	}
	rest
		.as_chunks::<4>()
		.0
		.iter()
		.all(|&[high, low, ..]| [high, low] == [0, 0] || [high, low] == width)
}

fn read_at<R: Read + Seek>(file: &mut R, offset: u64, len: usize) -> io::Result<Vec<u8>> {
	file.seek(SeekFrom::Start(offset))?;
	let mut buffer = vec![0; len];
	file.read_exact(&mut buffer)?;
	Ok(buffer)
}

fn be16(bytes: &[u8], at: usize) -> u16 {
	u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

fn be32(bytes: &[u8], at: usize) -> u32 {
	u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn invalid(message: &str) -> io::Error {
	io::Error::new(io::ErrorKind::InvalidData, message)
}
