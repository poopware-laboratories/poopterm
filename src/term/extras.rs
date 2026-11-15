//! Rarely used per-cell data (combining marks, OSC 8 hyperlinks, SGR 58
//! underline colors, parts of scaled text) kept out of [`Cell`](super::Cell) so cells stay 16 bytes.
use std::{
	collections::{HashMap, hash_map::Entry},
	hash::{Hash, Hasher},
};

use super::multicell::Multicell;

/// The `extra` id of a cell with none of these.
pub(crate) const NONE: u16 = 0;
/// Combining marks kept per cell; further marks are dropped.
pub(crate) const MAX_MARKS: usize = 8;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Extra {
	hyperlink: u16,
	marks: Box<[char]>,
	multicell: Option<Box<Multicell>>,
	underline: Option<u32>,
	/// DECSCA: selective erase leaves the character alone.
	protected: bool,
}

// Entries are hashed for every combining mark printed. SipHash costs
// mostly per write, so the common fields go in one write; SipHash itself
// stays, since programs choose the content.
impl Hash for Extra {
	fn hash<H: Hasher>(&self, state: &mut H) {
		let marks = &self.marks[..self.marks.len().min(MAX_MARKS)];
		let mut bytes = [0u8; 8 + 4 * MAX_MARKS];
		bytes[..2].copy_from_slice(&self.hyperlink.to_ne_bytes());
		bytes[2] = marks.len() as u8;
		bytes[3] = u8::from(self.multicell.is_some()) | u8::from(self.protected) << 1;
		// Colors are 24-bit; bit 24 tells black from none.
		let underline = self.underline.map_or(0, |color| color | 1 << 24);
		bytes[4..8].copy_from_slice(&underline.to_ne_bytes());
		for (chunk, &mark) in bytes[8..].as_chunks_mut::<4>().0.iter_mut().zip(marks) {
			*chunk = u32::from(mark).to_ne_bytes();
		}
		state.write(&bytes[..8 + 4 * marks.len()]);
		if let Some(multicell) = &self.multicell {
			multicell.hash(state);
		}
	}
}

/// Slots in the [`Extras::with_mark`] cache.
const MEMO: usize = 256;

/// A cached `with_mark` answer: `id` with `mark` appended is `result`.
#[derive(Clone, Copy)]
struct Memo {
	id: u16,
	result: u16,
	/// `u32::MAX`, never a `char`, in an empty slot.
	mark: u32,
}

fn memo_slot(id: u16, mark: char) -> usize {
	let hash = u32::from(id).wrapping_mul(0x9e37_79b9) ^ u32::from(mark).wrapping_mul(0x85eb_ca6b);
	(hash >> 24) as usize % MEMO
}

/// Interned [`Extra`] entries. Ids are stable for the table's lifetime and
/// identical entries share an id, so the table is bounded by distinct content.
#[derive(Clone)]
pub(crate) struct Extras {
	entries: Vec<Extra>,
	ids: HashMap<Extra, u16>,
	/// Recent `with_mark` answers, so text repeating the same marks skips
	/// building and hashing entries. Ids never change meaning, so answers
	/// stay right; colliding pairs only evict each other.
	memo: [Memo; MEMO],
	/// `with_hyperlink(NONE, link)` by `link`, or `NONE` when not yet asked:
	/// shells reopen the same few links on every prompt.
	links: Vec<u16>,
}

impl Default for Extras {
	fn default() -> Self {
		let none = Extra::default();
		Self {
			ids: HashMap::from([(none.clone(), NONE)]),
			entries: vec![none],
			memo: [Memo {
				id: NONE,
				result: NONE,
				mark: u32::MAX,
			}; MEMO],
			links: Vec::new(),
		}
	}
}

impl Extras {
	pub(crate) fn marks(&self, id: u16) -> &[char] {
		self
			.entries
			.get(id as usize)
			.map_or(&[], |extra| &extra.marks)
	}

	/// The OSC 8 hyperlink index for `id` (0 when none).
	pub(crate) fn hyperlink(&self, id: u16) -> u16 {
		self
			.entries
			.get(id as usize)
			.map_or(0, |extra| extra.hyperlink)
	}

	/// [`Self::with_mark`]'s answer if it is cached, found without `&mut`, so
	/// callers can skip making a shared table unique.
	#[inline]
	pub(crate) fn remembered_mark(&self, id: u16, mark: char) -> Option<u16> {
		let memo = self.memo[memo_slot(id, mark)];
		((memo.id, memo.mark) == (id, u32::from(mark))).then_some(memo.result)
	}

	/// The id for `id`'s content with `mark` appended, or `None` if the table is full.
	pub(crate) fn with_mark(&mut self, id: u16, mark: char) -> Option<u16> {
		if let Some(result) = self.remembered_mark(id, mark) {
			return Some(result);
		}
		let slot = memo_slot(id, mark);
		let current = self.entries.get(id as usize)?;
		let result = if current.marks.len() >= MAX_MARKS {
			id
		} else {
			let mut marks = Vec::with_capacity(current.marks.len() + 1);
			marks.extend_from_slice(&current.marks);
			marks.push(mark);
			self.intern(Extra {
				hyperlink: current.hyperlink,
				marks: marks.into(),
				multicell: current.multicell.clone(),
				underline: current.underline,
				protected: current.protected,
			})?
		};
		self.memo[slot] = Memo {
			id,
			result,
			mark: mark.into(),
		};
		Some(result)
	}

	/// [`Self::with_hyperlink`]'s answer for a plain cell if it is known,
	/// found without `&mut`.
	#[inline]
	pub(crate) fn remembered_link(&self, link: u16) -> Option<u16> {
		self
			.links
			.get(usize::from(link))
			.copied()
			.filter(|&id| id != NONE)
	}

	/// The id for `id`'s marks with hyperlink `link`, or `None` if the table is full.
	pub(crate) fn with_hyperlink(&mut self, id: u16, link: u16) -> Option<u16> {
		let current = self.entries.get(id as usize)?;
		if current.hyperlink == link {
			return Some(id);
		}
		let result = self.intern(Extra {
			hyperlink: link,
			..current.clone()
		})?;
		if id == NONE {
			let slot = usize::from(link);
			self.links.resize(self.links.len().max(slot + 1), NONE);
			self.links[slot] = result;
		}
		Some(result)
	}

	/// The SGR 58 underline color for `id`, if it has one.
	pub(crate) fn underline_color(&self, id: u16) -> Option<u32> {
		self.entries.get(id as usize)?.underline
	}

	/// The id for `id`'s content with underline color `color`, or `None` if
	/// the table is full.
	pub(crate) fn with_underline_color(&mut self, id: u16, color: Option<u32>) -> Option<u16> {
		let current = self.entries.get(id as usize)?;
		if current.underline == color {
			return Some(id);
		}
		self.intern(Extra {
			underline: color,
			..current.clone()
		})
	}

	/// Whether DECSCA protected `id`'s cell from selective erase.
	pub(crate) fn protected(&self, id: u16) -> bool {
		self
			.entries
			.get(id as usize)
			.is_some_and(|extra| extra.protected)
	}

	/// The id for `id`'s content, protected or not, or `None` if the table is full.
	pub(crate) fn with_protection(&mut self, id: u16, protected: bool) -> Option<u16> {
		let current = self.entries.get(id as usize)?;
		if current.protected == protected {
			return Some(id);
		}
		self.intern(Extra {
			protected,
			..current.clone()
		})
	}

	/// The part of a block of scaled text `id`'s cell holds, if any.
	pub(crate) fn multicell(&self, id: u16) -> Option<&Multicell> {
		self.entries.get(id as usize)?.multicell.as_deref()
	}

	/// The id for `id`'s content holding `multicell`, or `None` if the table is full.
	pub(crate) fn with_multicell(&mut self, id: u16, multicell: Option<Multicell>) -> Option<u16> {
		let current = self.entries.get(id as usize)?;
		self.intern(Extra {
			multicell: multicell.map(Box::new),
			..current.clone()
		})
	}

	fn intern(&mut self, extra: Extra) -> Option<u16> {
		match self.ids.entry(extra) {
			Entry::Occupied(entry) => Some(*entry.get()),
			Entry::Vacant(entry) => {
				let id = u16::try_from(self.entries.len()).ok()?;
				self.entries.push(entry.key().clone());
				Some(*entry.insert(id))
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn plain_cells_need_no_entry() {
		let mut extras = Extras::default();
		assert_eq!(extras.with_hyperlink(NONE, 0), Some(NONE));
		assert!(extras.marks(NONE).is_empty());
		assert_eq!(extras.hyperlink(NONE), 0);
	}

	#[test]
	fn identical_entries_are_shared() {
		let mut extras = Extras::default();
		let a = extras.with_mark(NONE, '\u{301}').unwrap();
		let b = extras.with_mark(NONE, '\u{301}').unwrap();
		assert_eq!(a, b);
		assert_eq!(extras.marks(a), ['\u{301}']);
	}

	#[test]
	fn marks_and_hyperlinks_combine() {
		let mut extras = Extras::default();
		let linked = extras.with_hyperlink(NONE, 7).unwrap();
		let marked = extras.with_mark(linked, '\u{301}').unwrap();
		assert_eq!(extras.hyperlink(marked), 7);
		assert_eq!(extras.marks(marked), ['\u{301}']);
		assert_eq!(extras.hyperlink(linked), 7);
		assert!(extras.marks(linked).is_empty());
	}

	#[test]
	fn repeated_marks_resolve_to_the_entries_they_name() {
		let mut extras = Extras::default();
		let mut seen = HashMap::new();
		// More marks per id than the cache has slots, so pairs collide; each
		// is asked for twice.
		for round in 0..2 {
			for base in 0..8u16 {
				let id = extras.with_hyperlink(NONE, base + 1).unwrap();
				for mark in '\u{300}'..char::from_u32(0x300 + 2 * MEMO as u32).unwrap() {
					let marked = extras.with_mark(id, mark).unwrap();
					assert_eq!(extras.marks(marked), [mark]);
					assert_eq!(extras.hyperlink(marked), base + 1);
					assert_eq!(
						*seen.entry((id, mark)).or_insert(marked),
						marked,
						"round {round}"
					);
				}
			}
		}
	}

	/// The memo answers only for plain cells, and only with the id
	/// `with_hyperlink` gave; marked or colored cells still intern.
	#[test]
	fn remembered_links_are_only_ever_plain_with_hyperlink_answers() {
		let mut extras = Extras::default();
		assert_eq!(extras.remembered_link(3), None);
		let accent = extras.with_mark(NONE, '\u{301}').unwrap();
		let marked = extras.with_hyperlink(accent, 3).unwrap();
		assert_eq!(extras.remembered_link(3), None);
		let plain = extras.with_hyperlink(NONE, 3).unwrap();
		assert_ne!(plain, marked);
		assert_eq!(extras.remembered_link(3), Some(plain));
		assert_eq!(extras.remembered_link(4), None);
		assert_eq!(extras.with_hyperlink(NONE, 0), Some(NONE));
		assert_eq!(extras.remembered_link(0), None);
		assert_eq!(extras.remembered_link(u16::MAX), None);
		let colored = extras.with_underline_color(plain, Some(0xff)).unwrap();
		assert_eq!(extras.hyperlink(colored), 3);
		assert_eq!(extras.remembered_link(3), Some(plain));
	}

	/// Under any mix of marks, links and colors, the memo agrees with
	/// interning: a remembered link is the plain entry for that link, and
	/// asking again never changes the answer.
	#[test]
	fn remembered_links_match_interning_under_random_operations() {
		let mut rng = crate::bench::corpus::Rng::new(0x11);
		let mut extras = Extras::default();
		let mut ids = vec![NONE];
		let mut plain = HashMap::new();
		for _ in 0..20_000 {
			let id = *rng.pick(&ids);
			let link = rng.below(40) as u16;
			let next = match rng.below(3) {
				0 => extras.with_mark(id, *rng.pick(&['\u{301}', '\u{302}', '\u{20e3}'])),
				1 => extras.with_hyperlink(id, link).inspect(|&fresh| {
					if id == NONE && link != 0 {
						assert_eq!(*plain.entry(link).or_insert(fresh), fresh);
					}
				}),
				_ => extras.with_underline_color(id, rng.pick(&[None, Some(0), Some(0xff)]).to_owned()),
			}
			.unwrap();
			ids.push(next);
			let probe = rng.below(40) as u16;
			if let Some(remembered) = extras.remembered_link(probe) {
				assert_eq!(plain.get(&probe), Some(&remembered));
				assert_eq!(extras.hyperlink(remembered), probe);
				assert!(extras.marks(remembered).is_empty());
				assert_eq!(extras.underline_color(remembered), None);
			}
		}
	}

	#[test]
	fn remembered_marks_are_only_ever_with_marks_answers() {
		let mut extras = Extras::default();
		assert_eq!(extras.remembered_mark(NONE, '\u{301}'), None);
		let accent = extras.with_mark(NONE, '\u{301}').unwrap();
		assert_eq!(extras.remembered_mark(NONE, '\u{301}'), Some(accent));
		assert_eq!(extras.remembered_mark(NONE, '\u{302}'), None);
		assert_eq!(extras.remembered_mark(accent, '\u{301}'), None);
		// Fill every slot several times over: whatever is still cached must
		// be the answer with_mark gives, and asking must not change it.
		for mark in '\u{300}'..char::from_u32(0x300 + 4 * MEMO as u32).unwrap() {
			extras.with_mark(accent, mark).unwrap();
		}
		let mut hits = 0;
		for mark in '\u{300}'..char::from_u32(0x300 + 4 * MEMO as u32).unwrap() {
			if let Some(cached) = extras.remembered_mark(accent, mark) {
				hits += 1;
				assert_eq!(extras.marks(cached), ['\u{301}', mark]);
				assert_eq!(extras.with_mark(accent, mark), Some(cached));
			}
		}
		assert!((1..=MEMO).contains(&hits), "{hits}");
		// Another id whose answer would sit in the same slot is not answered.
		let slot = memo_slot(accent, '\u{300}');
		let other = (0..=u16::MAX).find(|&id| id != accent && memo_slot(id, '\u{300}') == slot);
		extras.with_mark(accent, '\u{300}').unwrap();
		assert_eq!(extras.remembered_mark(other.unwrap(), '\u{300}'), None);
	}

	#[test]
	fn mark_count_is_bounded() {
		let mut extras = Extras::default();
		let mut id = NONE;
		for _ in 0..MAX_MARKS + 3 {
			id = extras.with_mark(id, '\u{301}').unwrap();
		}
		assert_eq!(extras.marks(id).len(), MAX_MARKS);
	}

	#[test]
	fn a_full_table_refuses_new_entries() {
		let mut extras = Extras::default();
		for link in 1..=u16::MAX {
			assert!(extras.with_hyperlink(NONE, link).is_some());
		}
		assert_eq!(extras.with_mark(NONE, 'x'), None, "65,536 ids are in use");
		assert!(
			extras.with_hyperlink(NONE, 1).is_some(),
			"existing entries still resolve"
		);
	}

	#[test]
	fn underline_colors_are_interned_with_the_rest_of_the_cell() {
		let mut extras = Extras::default();
		assert_eq!(extras.underline_color(NONE), None);
		assert_eq!(extras.with_underline_color(NONE, None), Some(NONE));
		let red = extras.with_underline_color(NONE, Some(0xff0000)).unwrap();
		let black = extras.with_underline_color(NONE, Some(0)).unwrap();
		assert_ne!(red, NONE);
		assert_ne!(black, NONE, "black is a color");
		assert_ne!(red, black);
		assert_eq!(extras.with_underline_color(NONE, Some(0xff0000)), Some(red));
		assert_eq!(extras.underline_color(red), Some(0xff0000));
		assert_eq!(extras.underline_color(black), Some(0));
		assert_eq!(extras.with_underline_color(red, None), Some(NONE));
	}

	#[test]
	fn underline_colors_survive_marks_and_hyperlinks() {
		let mut extras = Extras::default();
		let red = extras.with_underline_color(NONE, Some(0xff0000)).unwrap();
		let marked = extras.with_mark(red, '\u{301}').unwrap();
		let linked = extras.with_hyperlink(marked, 3).unwrap();
		assert_eq!(extras.underline_color(marked), Some(0xff0000));
		assert_eq!(extras.underline_color(linked), Some(0xff0000));
		assert_eq!(extras.marks(linked), ['\u{301}']);
		assert_ne!(marked, extras.with_mark(NONE, '\u{301}').unwrap());
		let unlinked = extras.with_hyperlink(linked, 0).unwrap();
		assert_eq!(unlinked, marked);
		let blue = extras.with_underline_color(linked, Some(0xff)).unwrap();
		assert_eq!(extras.hyperlink(blue), 3);
		assert_eq!(extras.marks(blue), ['\u{301}']);
	}
}
