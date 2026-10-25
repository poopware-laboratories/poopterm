//! Glyph substitution (`GSUB`): the lookups of the glyph composition and
//! ligature features, applied to a cluster's glyphs, which is how color
//! fonts turn emoji sequences (ZWJ, flags, keycaps, tags) into one glyph.
//! Written from the OpenType specification; contextual lookups are in
//! `gsub_context`.
use super::{face::Table, otl, otl::Gdef, read::Bytes};

/// The features applied, besides the language system's required one.
const FEATURES: [&[u8; 4]; 3] = [b"ccmp", b"rlig", b"liga"];

/// Bounds against hostile fonts: the glyphs a cluster may grow to, the
/// subtables tried while shaping one, and how deep contextual lookups nest.
pub(super) const MAX_GLYPHS: usize = 64;
const MAX_OPERATIONS: usize = 4096;
pub(super) const MAX_NESTING: usize = 8;

/// The lookup types.
const SINGLE: u16 = 1;
const MULTIPLE: u16 = 2;
const ALTERNATE: u16 = 3;
const LIGATURE: u16 = 4;
const CONTEXT: u16 = 5;
const CHAINED_CONTEXT: u16 = 6;
const EXTENSION: u16 = 7;
const REVERSE_CHAINED: u16 = 8;

/// A face's substitutions: the lookups its features use, in the order the
/// lookup list gives them.
#[derive(Clone, Debug)]
pub(crate) struct Gsub {
	table: Table,
	lookup_list: usize,
	lookups: Vec<u16>,
}

impl Gsub {
	/// Reads the lookups of [`FEATURES`] for the default script, or Latin,
	/// or the first one, in its default language system.
	pub(crate) fn parse(data: &[u8], table: Table) -> Option<Self> {
		let bytes = table.bytes(data);
		if bytes.u16_at(0)? != 1 {
			return None;
		}
		let offset = |at| bytes.u16_at(at).map(usize::from);
		let (scripts, features, lookup_list) = (offset(4)?, offset(6)?, offset(8)?);
		let lang = language_system(bytes, scripts)?;
		let feature_count = bytes.u16_at(features)?;
		let required = bytes.u16_at(lang + 2)?;
		let listed = (0..usize::from(bytes.u16_at(lang + 4)?))
			.filter_map(|i| bytes.u16_at(lang + 6 + 2 * i))
			.map(|index| (index, false));
		let mut lookups = Vec::new();
		for (index, required) in std::iter::once((required, true)).chain(listed) {
			if index >= feature_count {
				continue;
			}
			let record = features + 2 + 6 * usize::from(index);
			let tag = bytes.slice_at(record, 4)?;
			if !required && !FEATURES.iter().any(|wanted| wanted[..] == *tag) {
				continue;
			}
			let feature = features + offset(record + 4)?;
			let count = usize::from(bytes.u16_at(feature + 2)?);
			lookups.extend((0..count).filter_map(|i| bytes.u16_at(feature + 4 + 2 * i)));
		}
		lookups.sort_unstable();
		lookups.dedup();
		Some(Self {
			table,
			lookup_list,
			lookups,
		})
	}

	/// Substitutes `glyphs` with each lookup in turn.
	pub(crate) fn apply(&self, data: &[u8], gdef: Option<&Gdef>, glyphs: &mut Vec<u16>) {
		let mut shaper = Shaper {
			bytes: self.table.bytes(data),
			data,
			gdef,
			list: self.lookup_list,
			budget: MAX_OPERATIONS,
		};
		for &index in &self.lookups {
			if let Some(lookup) = shaper.lookup(index) {
				shaper.apply_lookup(&lookup, glyphs);
			}
		}
	}
}

/// The language system to shape with: the default script's, else Latin's,
/// else the first script's; its default one, else its first.
fn language_system(bytes: &[u8], scripts: usize) -> Option<usize> {
	let count = usize::from(bytes.u16_at(scripts)?);
	let script =
		|tag: &[u8; 4]| (0..count).find(|i| bytes.slice_at(scripts + 2 + 6 * i, 4) == Some(&tag[..]));
	let index = script(b"DFLT").or_else(|| script(b"latn")).unwrap_or(0);
	(index < count).then_some(())?;
	let script = scripts + usize::from(bytes.u16_at(scripts + 6 + 6 * index)?);
	let default = usize::from(bytes.u16_at(script)?);
	if default != 0 {
		return Some(script + default);
	}
	(bytes.u16_at(script + 2)? > 0).then_some(())?;
	Some(script + usize::from(bytes.u16_at(script + 8)?))
}

/// A lookup: its type (the extended one for extension lookups), flags,
/// mark filtering set and subtables.
pub(super) struct Lookup {
	at: usize,
	kind: u16,
	flag: u16,
	set: u16,
	count: usize,
}

/// Shapes one cluster: the `GSUB` table, the font for `GDEF`, and the
/// operations left.
pub(super) struct Shaper<'a> {
	pub(super) bytes: &'a [u8],
	data: &'a [u8],
	gdef: Option<&'a Gdef>,
	list: usize,
	budget: usize,
}

impl Shaper<'_> {
	pub(super) fn lookup(&self, index: u16) -> Option<Lookup> {
		let bytes = self.bytes;
		let count = bytes.u16_at(self.list)?;
		(index < count).then_some(())?;
		let at = self.list + usize::from(bytes.u16_at(self.list + 2 + 2 * usize::from(index))?);
		let (kind, flag) = (bytes.u16_at(at)?, bytes.u16_at(at + 2)?);
		let count = usize::from(bytes.u16_at(at + 4)?);
		let set = if flag & otl::USE_MARK_FILTERING_SET != 0 {
			bytes.u16_at(at + 6 + 2 * count)?
		} else {
			0
		};
		let mut lookup = Lookup {
			at,
			kind,
			flag,
			set,
			count,
		};
		if kind == EXTENSION {
			lookup.kind = self.subtable(&lookup, 0)?.0;
		}
		Some(lookup)
	}

	/// The type and offset of `lookup`'s subtable `i`, through extensions.
	fn subtable(&self, lookup: &Lookup, i: usize) -> Option<(u16, usize)> {
		let bytes = self.bytes;
		let at = lookup.at + usize::from(bytes.u16_at(lookup.at + 6 + 2 * i)?);
		if bytes.u16_at(lookup.at)? != EXTENSION {
			return Some((lookup.kind, at));
		}
		let kind = bytes.u16_at(at + 2)?;
		(bytes.u16_at(at)? == 1).then_some(())?;
		Some((kind, at + bytes.u32_at(at + 4)? as usize))
	}

	fn skips(&self, lookup: &Lookup, glyph: u16) -> bool {
		(self.gdef).is_some_and(|gdef| gdef.skips(self.data, lookup.flag, lookup.set, glyph))
	}

	/// The first glyph after `from` that `lookup` does not skip.
	pub(super) fn next(&self, lookup: &Lookup, glyphs: &[u16], from: usize) -> Option<usize> {
		(from + 1..glyphs.len()).find(|&i| !self.skips(lookup, glyphs[i]))
	}

	/// The last glyph before `from` that `lookup` does not skip.
	pub(super) fn previous(&self, lookup: &Lookup, glyphs: &[u16], from: usize) -> Option<usize> {
		(0..from).rev().find(|&i| !self.skips(lookup, glyphs[i]))
	}

	/// Applies `lookup` across `glyphs`, from the end for reverse ones.
	fn apply_lookup(&mut self, lookup: &Lookup, glyphs: &mut Vec<u16>) {
		if lookup.kind == REVERSE_CHAINED {
			for pos in (0..glyphs.len()).rev() {
				if !self.skips(lookup, glyphs[pos]) {
					self.apply_at(lookup, glyphs, pos, 0);
				}
			}
			return;
		}
		let mut pos = 0;
		while pos < glyphs.len() && self.budget > 0 {
			if self.skips(lookup, glyphs[pos]) {
				pos += 1;
				continue;
			}
			let before = glyphs.len();
			pos = match self.apply_at(lookup, glyphs, pos, 0) {
				// A deletion leaves the next glyph at `pos`.
				Some(next) if glyphs.len() < before => next.max(pos),
				Some(next) => next.max(pos + 1),
				None => pos + 1,
			};
		}
	}

	/// Applies the first of `lookup`'s subtables that matches at `pos`;
	/// returns where to go on.
	pub(super) fn apply_at(
		&mut self,
		lookup: &Lookup,
		glyphs: &mut Vec<u16>,
		pos: usize,
		depth: usize,
	) -> Option<usize> {
		for i in 0..lookup.count {
			if self.budget == 0 {
				return None;
			}
			self.budget -= 1;
			let Some((kind, at)) = self.subtable(lookup, i) else {
				continue;
			};
			let applied = match kind {
				SINGLE => single(self.bytes, at, glyphs[pos]).map(|glyph| {
					glyphs[pos] = glyph;
					pos + 1
				}),
				MULTIPLE => multiple(self.bytes, at, glyphs, pos),
				ALTERNATE => alternate(self.bytes, at, glyphs[pos]).map(|glyph| {
					glyphs[pos] = glyph;
					pos + 1
				}),
				LIGATURE => self.ligature(lookup, at, glyphs, pos),
				CONTEXT => self.context(lookup, at, glyphs, pos, depth),
				CHAINED_CONTEXT => self.chained(lookup, at, glyphs, pos, depth),
				REVERSE_CHAINED if depth == 0 => self.reverse(lookup, at, glyphs, pos),
				_ => None,
			};
			if applied.is_some() {
				return applied;
			}
		}
		None
	}

	/// Ligature substitution: the first ligature whose components follow
	/// `pos` replaces them; skipped glyphs between them move after it.
	fn ligature(
		&self,
		lookup: &Lookup,
		at: usize,
		glyphs: &mut Vec<u16>,
		pos: usize,
	) -> Option<usize> {
		let bytes = self.bytes;
		let set = at + indexed(bytes, at, glyphs[pos])?;
		'ligatures: for k in 0..usize::from(bytes.u16_at(set)?) {
			let ligature = set + usize::from(bytes.u16_at(set + 2 + 2 * k)?);
			let (target, count) = (bytes.u16_at(ligature)?, bytes.u16_at(ligature + 2)?);
			let mut matched = vec![pos];
			for c in 1..usize::from(count) {
				let next = self.next(lookup, glyphs, *matched.last()?);
				match next.zip(bytes.u16_at(ligature + 2 + 2 * c)) {
					Some((next, component)) if glyphs[next] == component => matched.push(next),
					_ => continue 'ligatures,
				}
			}
			if count == 0 {
				continue;
			}
			let last = *matched.last()?;
			let kept: Vec<u16> = (pos + 1..last)
				.filter(|i| !matched.contains(i))
				.map(|i| glyphs[i])
				.collect();
			glyphs.splice(pos..=last, std::iter::once(target).chain(kept));
			return Some(pos + 1);
		}
		None
	}
}

/// The offset a format 1 subtable at `at` keeps for `glyph`, from its
/// coverage and the array of offsets at `at + 6`.
fn indexed(bytes: &[u8], at: usize, glyph: u16) -> Option<usize> {
	let index = usize::from(covered(bytes, at, glyph)?);
	(bytes.u16_at(at)? == 1 && index < usize::from(bytes.u16_at(at + 4)?)).then_some(())?;
	let offset = usize::from(bytes.u16_at(at + 6 + 2 * index)?);
	(offset != 0).then_some(offset)
}

/// `glyph`'s index in the coverage table of the subtable at `at`.
pub(super) fn covered(bytes: &[u8], at: usize, glyph: u16) -> Option<u16> {
	otl::coverage(bytes, at + usize::from(bytes.u16_at(at + 2)?), glyph)
}

/// Single substitution: a delta, or a substitute per covered glyph.
fn single(bytes: &[u8], at: usize, glyph: u16) -> Option<u16> {
	let index = usize::from(covered(bytes, at, glyph)?);
	match bytes.u16_at(at)? {
		1 => Some(glyph.wrapping_add(bytes.u16_at(at + 4)?)),
		2 if index < usize::from(bytes.u16_at(at + 4)?) => bytes.u16_at(at + 6 + 2 * index),
		_ => None,
	}
}

/// Multiple substitution: the glyph becomes a sequence, empty to delete it.
fn multiple(bytes: &[u8], at: usize, glyphs: &mut Vec<u16>, pos: usize) -> Option<usize> {
	let sequence = at + indexed(bytes, at, glyphs[pos])?;
	let count = usize::from(bytes.u16_at(sequence)?);
	(glyphs.len() - 1 + count <= MAX_GLYPHS).then_some(())?;
	let substitutes = (0..count)
		.map(|i| bytes.u16_at(sequence + 2 + 2 * i))
		.collect::<Option<Vec<u16>>>()?;
	glyphs.splice(pos..=pos, substitutes);
	Some(pos + count)
}

/// Alternate substitution: with no way to choose, the first alternate.
fn alternate(bytes: &[u8], at: usize, glyph: u16) -> Option<u16> {
	let set = at + indexed(bytes, at, glyph)?;
	(bytes.u16_at(set)? > 0).then_some(())?;
	bytes.u16_at(set + 2)
}

#[cfg(test)]
#[path = "gsub_tests.rs"]
pub(crate) mod tests;
