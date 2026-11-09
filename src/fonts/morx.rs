//! Extended glyph metamorphosis (`morx`): the AAT substitutions of a face's
//! default features, applied to a cluster's glyphs. Apple Color Emoji has no
//! `GSUB`; it turns flags, skin tones, keycaps and ZWJ sequences into one
//! glyph here. Written from Apple's TrueType Reference Manual: noncontextual,
//! contextual and ligature subtables run; rearrangement and insertion
//! subtables, and subtables for vertical text only, are skipped.
use super::{face::Table, morx_lookup::lookup, read::Bytes};

/// Bounds against hostile fonts: chains and subtables read, components held
/// for one ligature, and state transitions per glyph.
const MAX_CHAINS: usize = 16;
const MAX_SUBTABLES: usize = 256;
const MAX_STACK: usize = 32;
const STEPS_PER_GLYPH: usize = 16;

/// A glyph a subtable removed; dropped once every chain has run.
const DELETED: u16 = 0xffff;
/// The subtable types, in the low byte of its coverage, and its vertical flag.
const CONTEXTUAL: u32 = 1;
const LIGATURE: u32 = 2;
const NONCONTEXTUAL: u32 = 4;
const TYPE_MASK: u32 = 0xff;
const VERTICAL: u32 = 0x8000_0000;
/// Entry flags: contextual `SET_MARK` and ligature `SET_COMPONENT` share a
/// bit, as do `DONT_ADVANCE`, and the ligature `PERFORM_ACTION`.
const SET_MARK: u16 = 0x8000;
const SET_COMPONENT: u16 = 0x8000;
const DONT_ADVANCE: u16 = 0x4000;
const PERFORM_ACTION: u16 = 0x2000;
/// Ligature action bits, the rest being a signed 30-bit component offset.
const LAST: u32 = 0x8000_0000;
const STORE: u32 = 0x4000_0000;
const OFFSET_SIGN: i32 = 0x2000_0000;

/// A face's `morx` table.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Morx {
	table: Table,
}

impl Morx {
	/// The table, if it is version 2 or 3.
	pub(crate) fn parse(data: &[u8], table: Table) -> Option<Self> {
		matches!(table.bytes(data).u16_at(0)?, 2 | 3).then_some(Self { table })
	}

	/// Substitutes `glyphs` with each enabled subtable of each chain.
	pub(crate) fn apply(&self, data: &[u8], glyphs: &mut Vec<u16>) {
		let bytes = self.table.bytes(data);
		let chains = bytes
			.u32_at(4)
			.map_or(0, |count| (count as usize).min(MAX_CHAINS));
		let mut at = 8;
		for _ in 0..chains {
			match chain(bytes, at, glyphs) {
				Some(next) => at = next,
				None => break,
			}
		}
		glyphs.retain(|&glyph| glyph != DELETED);
	}
}

/// Runs the subtables of the chain at `at` that its default features
/// enable; the offset of the next chain.
fn chain(bytes: &[u8], at: usize, glyphs: &mut [u16]) -> Option<usize> {
	let defaults = bytes.u32_at(at)?;
	let end = at.checked_add(bytes.u32_at(at + 4)? as usize)?;
	let features = bytes.u32_at(at + 8)? as usize;
	let count = (bytes.u32_at(at + 12)? as usize).min(MAX_SUBTABLES);
	let mut start = at.checked_add(16)?.checked_add(features.checked_mul(12)?)?;
	for _ in 0..count {
		let size = bytes.u32_at(start)? as usize;
		let next = start.checked_add(size.max(12))?;
		let (coverage, enabled) = (bytes.u32_at(start + 4)?, bytes.u32_at(start + 8)?);
		if enabled & defaults != 0 && coverage & VERTICAL == 0 {
			let body = bytes.get(start + 12..next)?;
			match coverage & TYPE_MASK {
				NONCONTEXTUAL => replace_each(body, glyphs),
				LIGATURE => ligatures(body, glyphs),
				CONTEXTUAL => contextual(body, glyphs),
				_ => {}
			}
		}
		start = next;
	}
	Some(end).filter(|&end| end > at)
}

/// Noncontextual: each glyph the lookup table lists becomes its value.
fn replace_each(body: &[u8], glyphs: &mut [u16]) {
	for glyph in glyphs.iter_mut().filter(|glyph| **glyph != DELETED) {
		if let Some(value) = lookup(body, 0, *glyph) {
			*glyph = value;
		}
	}
}

/// An extended state table: its glyph classes, state array and entries.
struct Machine<'a> {
	body: &'a [u8],
	classes: usize,
	class_table: usize,
	states: usize,
	entries: usize,
}

impl<'a> Machine<'a> {
	fn new(body: &'a [u8]) -> Option<Self> {
		let word = |at| body.u32_at(at).map(|value| value as usize);
		Some(Self {
			body,
			classes: word(0)?,
			class_table: word(4)?,
			states: word(8)?,
			entries: word(12)?,
		})
	}

	/// End of text is class 0, a deleted glyph 2, one the table lacks 1.
	fn class(&self, glyph: Option<u16>) -> usize {
		match glyph {
			None => 0,
			Some(DELETED) => 2,
			Some(glyph) => lookup(self.body, self.class_table, glyph).map_or(1, usize::from),
		}
	}

	/// Where the entry for `state` and `class` starts, entries being `size`
	/// bytes.
	fn entry(&self, state: u16, class: usize, size: usize) -> Option<usize> {
		if class >= self.classes {
			return None;
		}
		let cell = usize::from(state)
			.checked_mul(self.classes)?
			.checked_add(class)?;
		let entry = self
			.body
			.u16_at(self.states.checked_add(cell.checked_mul(2)?)?)?;
		self
			.entries
			.checked_add(usize::from(entry).checked_mul(size)?)
	}
}

/// Walks `glyphs` and then the end of text through the state table in
/// `body`, handing each entry's start and flags to `step`, which is given
/// the position. Stops after a bounded number of transitions.
fn drive(
	body: &[u8],
	glyphs: &mut [u16],
	entry_size: usize,
	mut step: impl FnMut(&mut [u16], usize, u16, usize),
) {
	let Some(machine) = Machine::new(body) else {
		return;
	};
	let (mut state, mut at) = (0u16, 0);
	let mut budget = STEPS_PER_GLYPH * (glyphs.len() + 1);
	while at <= glyphs.len() && budget > 0 {
		budget -= 1;
		let class = machine.class(glyphs.get(at).copied());
		let Some(entry) = machine.entry(state, class, entry_size) else {
			return;
		};
		let (Some(next), Some(flags)) = (body.u16_at(entry), body.u16_at(entry + 2)) else {
			return;
		};
		step(glyphs, at, flags, entry);
		state = next;
		if flags & DONT_ADVANCE == 0 {
			at += 1;
		}
	}
}

/// Ligature: components are pushed as the machine reads them and, on the
/// performing entry, popped through the action list into one glyph.
fn ligatures(body: &[u8], glyphs: &mut [u16]) {
	let word = |at| body.u32_at(at).map(|value| value as usize);
	let (Some(actions), Some(components), Some(table)) = (word(16), word(20), word(24)) else {
		return;
	};
	let mut stack = Vec::new();
	drive(body, glyphs, 6, |glyphs, at, flags, entry| {
		if flags & SET_COMPONENT != 0 && at < glyphs.len() && stack.len() < MAX_STACK {
			stack.push(at);
		}
		if flags & PERFORM_ACTION != 0
			&& let Some(first) = body.u16_at(entry + 4)
		{
			let at = (actions, components, table);
			join(body, at, usize::from(first), glyphs, &mut stack);
		}
	});
}

/// Runs the actions from `first`: each pops a component, adds the value its
/// glyph and offset give to a running sum, and on its last or store bit
/// writes the ligature that sum picks; a store keeps the glyph for the next
/// action, a plain action deletes it.
fn join(
	body: &[u8],
	(actions, components, table): (usize, usize, usize),
	first: usize,
	glyphs: &mut [u16],
	stack: &mut Vec<usize>,
) {
	let mut sum = 0usize;
	for n in 0..2 * MAX_STACK {
		let Some(action) = body.u32_at(actions.saturating_add(4 * (first + n))) else {
			return;
		};
		let Some(place) = stack.pop() else {
			return;
		};
		let raw = (action & 0x3fff_ffff) as i32;
		let offset = if raw & OFFSET_SIGN != 0 {
			raw - 2 * OFFSET_SIGN
		} else {
			raw
		};
		let index = i64::from(glyphs[place]) + i64::from(offset);
		let Some(part) = usize::try_from(index)
			.ok()
			.and_then(|index| body.u16_at(components.saturating_add(2 * index)))
		else {
			return;
		};
		sum = sum.saturating_add(usize::from(part));
		if action & (LAST | STORE) == 0 {
			glyphs[place] = DELETED;
		} else {
			let Some(ligature) = body.u16_at(table.saturating_add(2 * sum)) else {
				return;
			};
			glyphs[place] = ligature;
			if action & LAST == 0 {
				stack.push(place);
			}
		}
		if action & LAST != 0 {
			return;
		}
	}
}

/// Contextual: an entry may replace the marked glyph and the current one
/// from its substitution tables, and mark the current glyph.
fn contextual(body: &[u8], glyphs: &mut [u16]) {
	let Some(substitutions) = body.u32_at(16).map(|offset| offset as usize) else {
		return;
	};
	let mut mark = None;
	drive(body, glyphs, 8, |glyphs, at, flags, entry| {
		let replace = |glyph: &mut u16, index: Option<u16>| {
			let table = body.u32_at(substitutions.saturating_add(4 * usize::from(index?)))?;
			let lookup_at = substitutions.saturating_add(table as usize);
			*glyph = lookup(body, lookup_at, *glyph).filter(|_| *glyph != DELETED)?;
			Some(())
		};
		let (marked, current) = (body.u16_at(entry + 4), body.u16_at(entry + 6));
		let none = |index: Option<u16>| index.filter(|&index| index != 0xffff);
		if let Some(place) = mark {
			replace(&mut glyphs[place], none(marked));
		}
		if at < glyphs.len() {
			replace(&mut glyphs[at], none(current));
			if flags & SET_MARK != 0 {
				mark = Some(at);
			}
		}
	});
}
