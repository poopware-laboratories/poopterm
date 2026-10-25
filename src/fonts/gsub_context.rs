//! Contextual substitution in `GSUB`: rules that match a sequence of
//! glyphs, classes or coverages, with chained rules also checking the
//! glyphs before and after it, then apply other lookups inside the match;
//! and reverse chained single substitution. Written from the OpenType
//! specification, formats 1 to 3 of each.
use super::{
	gsub::{Lookup, MAX_NESTING, Shaper, covered},
	otl,
	read::Bytes,
};

/// How a rule's values name glyphs.
#[derive(Clone, Copy)]
enum Kind {
	Glyphs,
	/// Classes in the class definition at this offset, if there is one.
	Classes(Option<usize>),
	/// Offsets from the subtable at this offset to coverage tables.
	Coverages(usize),
}

/// `count` values from `at`, naming glyphs as `kind` says.
#[derive(Clone, Copy)]
struct Sequence {
	at: usize,
	count: usize,
	kind: Kind,
}

impl Sequence {
	fn new(bytes: &[u8], at: usize, kind: Kind) -> Option<Self> {
		let count = usize::from(bytes.u16_at(at)?);
		Some(Self {
			at: at + 2,
			count,
			kind,
		})
	}

	/// The offset just past the values.
	fn end(&self) -> usize {
		self.at + 2 * self.count
	}

	fn matches(&self, bytes: &[u8], i: usize, glyph: u16) -> bool {
		let Some(value) = bytes.u16_at(self.at + 2 * i) else {
			return false;
		};
		match self.kind {
			Kind::Glyphs => value == glyph,
			Kind::Classes(at) => at.map_or(0, |at| otl::class(bytes, at, glyph)) == value,
			Kind::Coverages(base) => otl::coverage(bytes, base + usize::from(value), glyph).is_some(),
		}
	}
}

/// A rule: the glyphs before, the input after its first glyph, the glyphs
/// after, and its `count` lookup records at `records`.
struct Rule {
	backtrack: Option<Sequence>,
	input: Sequence,
	lookahead: Option<Sequence>,
	records: usize,
	count: usize,
}

impl Shaper<'_> {
	/// Contextual substitution (type 5).
	pub(super) fn context(
		&mut self,
		lookup: &Lookup,
		at: usize,
		glyphs: &mut Vec<u16>,
		pos: usize,
		depth: usize,
	) -> Option<usize> {
		let bytes = self.bytes;
		let rule = |at: usize, kind| {
			let count = usize::from(bytes.u16_at(at)?).checked_sub(1)?;
			let input = Sequence {
				at: at + 4,
				count,
				kind,
			};
			Some(Rule {
				backtrack: None,
				records: input.end(),
				count: usize::from(bytes.u16_at(at + 2)?),
				input,
				lookahead: None,
			})
		};
		match bytes.u16_at(at)? {
			1 => {
				let set = self.rule_set(at, 4, usize::from(covered(bytes, at, glyphs[pos])?))?;
				self.first_rule(lookup, glyphs, pos, depth, set, |at| rule(at, Kind::Glyphs))
			}
			2 => {
				covered(bytes, at, glyphs[pos])?;
				let classes = offset(bytes, at, at + 4);
				let class = classes.map_or(0, |classes| otl::class(bytes, classes, glyphs[pos]));
				let set = self.rule_set(at, 6, usize::from(class))?;
				self.first_rule(lookup, glyphs, pos, depth, set, |at| {
					rule(at, Kind::Classes(classes))
				})
			}
			3 => {
				let count = usize::from(bytes.u16_at(at + 2)?);
				let first = usize::from(bytes.u16_at(at + 6)?);
				otl::coverage(bytes, at + first, glyphs[pos])?;
				let input = Sequence {
					at: at + 8,
					count: count.checked_sub(1)?,
					kind: Kind::Coverages(at),
				};
				let rule = Rule {
					backtrack: None,
					records: input.end(),
					count: usize::from(bytes.u16_at(at + 4)?),
					input,
					lookahead: None,
				};
				self.apply_rule(lookup, glyphs, pos, depth, &rule)
			}
			_ => None,
		}
	}

	/// Chained contextual substitution (type 6).
	pub(super) fn chained(
		&mut self,
		lookup: &Lookup,
		at: usize,
		glyphs: &mut Vec<u16>,
		pos: usize,
		depth: usize,
	) -> Option<usize> {
		let bytes = self.bytes;
		match bytes.u16_at(at)? {
			1 => {
				let set = self.rule_set(at, 4, usize::from(covered(bytes, at, glyphs[pos])?))?;
				let kinds = [Kind::Glyphs; 3];
				self.first_rule(lookup, glyphs, pos, depth, set, |at| {
					chain_rule(bytes, at, kinds)
				})
			}
			2 => {
				covered(bytes, at, glyphs[pos])?;
				let classes = [4, 6, 8].map(|field| Kind::Classes(offset(bytes, at, at + field)));
				let Kind::Classes(input) = classes[1] else {
					return None;
				};
				let class = input.map_or(0, |input| otl::class(bytes, input, glyphs[pos]));
				let set = self.rule_set(at, 10, usize::from(class))?;
				self.first_rule(lookup, glyphs, pos, depth, set, |at| {
					chain_rule(bytes, at, classes)
				})
			}
			3 => {
				let coverages = Kind::Coverages(at);
				let backtrack = Sequence::new(bytes, at + 2, coverages)?;
				let input = Sequence::new(bytes, backtrack.end(), coverages)?;
				let lookahead = Sequence::new(bytes, input.end(), coverages)?;
				(input.count > 0 && input.matches(bytes, 0, glyphs[pos])).then_some(())?;
				let rule = Rule {
					backtrack: Some(backtrack),
					input: Sequence {
						at: input.at + 2,
						count: input.count - 1,
						..input
					},
					lookahead: Some(lookahead),
					records: lookahead.end() + 2,
					count: usize::from(bytes.u16_at(lookahead.end())?),
				};
				self.apply_rule(lookup, glyphs, pos, depth, &rule)
			}
			_ => None,
		}
	}

	/// Reverse chained single substitution (type 8), applied from the end.
	pub(super) fn reverse(
		&mut self,
		lookup: &Lookup,
		at: usize,
		glyphs: &mut [u16],
		pos: usize,
	) -> Option<usize> {
		let bytes = self.bytes;
		let index = usize::from(covered(bytes, at, glyphs[pos])?);
		(bytes.u16_at(at)? == 1).then_some(())?;
		let backtrack = Sequence::new(bytes, at + 4, Kind::Coverages(at))?;
		let lookahead = Sequence::new(bytes, backtrack.end(), Kind::Coverages(at))?;
		self.context_matches(lookup, glyphs, (pos, pos), Some(backtrack), Some(lookahead))?;
		let substitutes = Sequence::new(bytes, lookahead.end(), Kind::Glyphs)?;
		(index < substitutes.count).then_some(())?;
		glyphs[pos] = bytes.u16_at(substitutes.at + 2 * index)?;
		Some(pos + 1)
	}

	/// The offset of rule set `index` of the subtable at `at`, whose count
	/// of sets is at `at + field`.
	fn rule_set(&self, at: usize, field: usize, index: usize) -> Option<usize> {
		let count = usize::from(self.bytes.u16_at(at + field)?);
		(index < count).then_some(())?;
		offset(self.bytes, at, at + field + 2 + 2 * index)
	}

	/// Applies the first rule of the set at `set` that matches at `pos`.
	fn first_rule(
		&mut self,
		lookup: &Lookup,
		glyphs: &mut Vec<u16>,
		pos: usize,
		depth: usize,
		set: usize,
		rule: impl Fn(usize) -> Option<Rule>,
	) -> Option<usize> {
		let bytes = self.bytes;
		let count = usize::from(bytes.u16_at(set)?);
		(0..count).find_map(|i| {
			let rule = rule(offset(bytes, set, set + 2 + 2 * i)?)?;
			self.apply_rule(lookup, glyphs, pos, depth, &rule)
		})
	}

	/// Matches `rule` at `pos`, then applies its lookups to the matched
	/// glyphs; returns the position after the match.
	fn apply_rule(
		&mut self,
		lookup: &Lookup,
		glyphs: &mut Vec<u16>,
		pos: usize,
		depth: usize,
		rule: &Rule,
	) -> Option<usize> {
		let bytes = self.bytes;
		let mut matched = vec![pos];
		for i in 0..rule.input.count {
			let next = self.next(lookup, glyphs, *matched.last()?)?;
			rule.input.matches(bytes, i, glyphs[next]).then_some(())?;
			matched.push(next);
		}
		let span = (pos, *matched.last()?);
		self.context_matches(lookup, glyphs, span, rule.backtrack, rule.lookahead)?;
		let mut end = span.1 as isize + 1;
		for r in 0..rule.count {
			let record = rule.records + 4 * r;
			let (Some(index), Some(nested)) = (bytes.u16_at(record), bytes.u16_at(record + 2)) else {
				break;
			};
			let at = matched.get(usize::from(index)).copied();
			let Some(at) = at.filter(|&at| at < glyphs.len() && depth < MAX_NESTING) else {
				continue;
			};
			let Some(nested) = self.lookup(nested) else {
				continue;
			};
			let before = glyphs.len() as isize;
			self.apply_at(&nested, glyphs, at, depth + 1);
			let delta = glyphs.len() as isize - before;
			for later in matched.iter_mut().filter(|later| **later > at) {
				*later = (*later as isize + delta).max(at as isize) as usize;
			}
			end += delta;
		}
		Some((end.max(pos as isize) as usize).min(glyphs.len()))
	}

	/// Whether the glyphs before `span` match `backtrack`, nearest first,
	/// and those after it match `lookahead`.
	fn context_matches(
		&self,
		lookup: &Lookup,
		glyphs: &[u16],
		(first, last): (usize, usize),
		backtrack: Option<Sequence>,
		lookahead: Option<Sequence>,
	) -> Option<()> {
		let bytes = self.bytes;
		if let Some(backtrack) = backtrack {
			let mut at = first;
			for i in 0..backtrack.count {
				at = self.previous(lookup, glyphs, at)?;
				backtrack.matches(bytes, i, glyphs[at]).then_some(())?;
			}
		}
		if let Some(lookahead) = lookahead {
			let mut at = last;
			for i in 0..lookahead.count {
				at = self.next(lookup, glyphs, at)?;
				lookahead.matches(bytes, i, glyphs[at]).then_some(())?;
			}
		}
		Some(())
	}
}

/// A chained rule at `at`, its sequences named by `kinds` (backtrack,
/// input, lookahead).
fn chain_rule(bytes: &[u8], at: usize, [back, input, ahead]: [Kind; 3]) -> Option<Rule> {
	let backtrack = Sequence::new(bytes, at, back)?;
	let input_count = usize::from(bytes.u16_at(backtrack.end())?).checked_sub(1)?;
	let input = Sequence {
		at: backtrack.end() + 2,
		count: input_count,
		kind: input,
	};
	let lookahead = Sequence::new(bytes, input.end(), ahead)?;
	Some(Rule {
		backtrack: Some(backtrack),
		input,
		lookahead: Some(lookahead),
		records: lookahead.end() + 2,
		count: usize::from(bytes.u16_at(lookahead.end())?),
	})
}

/// The offset at `field` from the table at `base`, `None` when it is 0.
fn offset(bytes: &[u8], base: usize, field: usize) -> Option<usize> {
	let offset = usize::from(bytes.u16_at(field)?);
	(offset != 0).then_some(base + offset)
}

#[cfg(test)]
#[path = "gsub_context_tests.rs"]
mod tests;
