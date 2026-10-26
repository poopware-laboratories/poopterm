//! Which code points a font maps, as exact ranges, kept in the font
//! database so fallback only opens fonts that have a character.
use std::{
	collections::{HashMap, hash_map::Entry},
	sync::Arc,
};

/// Inclusive code point ranges, sorted and merged.
pub(crate) type Ranges = Vec<(u32, u32)>;
/// [`Ranges`] shared by the faces that map the same characters.
pub(crate) type Shared = Arc<[(u32, u32)]>;

/// The last code point.
const LAST: u32 = 0x10_FFFF;

/// Inclusive code point `ranges`, in any order, sorted and merged; reversed
/// ranges are dropped and ranges past Unicode clamped.
pub(crate) fn ranges(ranges: impl IntoIterator<Item = (u32, u32)>) -> Ranges {
	let mut spans: Ranges = ranges
		.into_iter()
		.filter(|&(first, last)| first <= last && first <= LAST)
		.map(|(first, last)| (first, last.min(LAST)))
		.collect();
	spans.sort_unstable();
	let mut merged: Ranges = Vec::with_capacity(spans.len());
	for (first, last) in spans {
		match merged.last_mut() {
			Some((_, end)) if first <= end.saturating_add(1) => *end = (*end).max(last),
			_ => merged.push((first, last)),
		}
	}
	merged
}

/// Whether `ch` falls in one of `ranges`.
pub(crate) fn covers(ranges: &[(u32, u32)], ch: char) -> bool {
	let ch = u32::from(ch);
	let after = ranges.partition_point(|&(first, _)| first <= ch);
	after > 0 && ranges[after - 1].1 >= ch
}

/// `ranges` as text for the cache file: `20-7e,a0,4e00-9fff` in hex.
pub(crate) fn encode(ranges: &[(u32, u32)]) -> String {
	let spans: Vec<String> = ranges
		.iter()
		.map(|&(first, last)| {
			if first == last {
				format!("{first:x}")
			} else {
				format!("{first:x}-{last:x}")
			}
		})
		.collect();
	spans.join(",")
}

/// Parses [`encode`]'s output; `None` for anything else, including ranges
/// out of order, touching or past Unicode. One pass over the bytes: caches
/// hold hundreds of thousands of ranges and are read at startup.
pub(crate) fn decode(text: &str) -> Option<Ranges> {
	let mut out: Ranges = Vec::with_capacity(text.len() / 8);
	let mut bytes = text.as_bytes();
	while !bytes.is_empty() {
		let first = hex(&mut bytes)?;
		let last = match bytes.split_first() {
			Some((b'-', rest)) => {
				bytes = rest;
				hex(&mut bytes)?
			}
			_ => first,
		};
		match bytes.split_first() {
			Some((b',', rest)) if !rest.is_empty() => bytes = rest,
			None => {}
			_ => return None,
		}
		let after = out.last().map_or(0, |&(_, end)| end + 2);
		if first < after || first > last || last > LAST {
			return None;
		}
		out.push((first, last));
	}
	Some(out)
}

/// The one to six hex digits that start `bytes`, consumed.
fn hex(bytes: &mut &[u8]) -> Option<u32> {
	let mut value = 0;
	let mut digits = 0;
	for &byte in bytes.iter() {
		let digit = match byte {
			b'0'..=b'9' => byte - b'0',
			b'a'..=b'f' => byte - b'a' + 10,
			b'A'..=b'F' => byte - b'A' + 10,
			_ => break,
		};
		if digits == 6 {
			return None;
		}
		value = value << 4 | u32::from(digit);
		digits += 1;
	}
	*bytes = bytes.get(digits..)?;
	(digits > 0).then_some(value)
}

/// Writes face coverage for the cache, each distinct set once: the faces
/// of a family usually map the same characters, and a later face refers
/// to an earlier set as `=N`, the Nth distinct set written.
#[derive(Default)]
pub(crate) struct Writer<'a> {
	written: HashMap<&'a [(u32, u32)], usize>,
}

impl<'a> Writer<'a> {
	pub(crate) fn write(&mut self, ranges: &'a [(u32, u32)]) -> String {
		let next = self.written.len();
		match self.written.entry(ranges) {
			Entry::Occupied(seen) => format!("={}", seen.get()),
			Entry::Vacant(new) => {
				new.insert(next);
				encode(ranges)
			}
		}
	}
}

/// Reads what [`Writer`] wrote, in the same order, sharing each set
/// between the faces that refer to it.
#[derive(Default)]
pub(crate) struct Reader {
	read: Vec<Shared>,
}

impl Reader {
	/// `None` for malformed text or a reference to a set not read yet.
	pub(crate) fn read(&mut self, text: &str) -> Option<Shared> {
		if let Some(index) = text.strip_prefix('=') {
			// Digits only, without leading zeros, as `write` puts them.
			let digits = !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit());
			if !digits || index.len() > 1 && index.starts_with('0') {
				return None;
			}
			return self.read.get(index.parse::<usize>().ok()?).cloned();
		}
		let ranges: Shared = decode(text)?.into();
		self.read.push(ranges.clone());
		Some(ranges)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ranges_are_sorted_and_merged() {
		assert_eq!(ranges([(0x20, 0x7e)]), [(0x20, 0x7e)]);
		assert_eq!(
			ranges([(0x4e00, 0x9fff), (0x20, 0x17f)]),
			[(0x20, 0x17f), (0x4e00, 0x9fff)]
		);
		// Neighbouring and overlapping ranges merge; a gap keeps them apart.
		assert_eq!(ranges([(0x41, 0x5a), (0x5b, 0x60)]), [(0x41, 0x60)]);
		assert_eq!(ranges([(0x100, 0x3ff), (0x250, 0x260)]), [(0x100, 0x3ff)]);
		assert_eq!(
			ranges([(0x41, 0x5a), (0x5c, 0x60)]),
			[(0x41, 0x5a), (0x5c, 0x60)]
		);
		assert_eq!(ranges([]), []);
	}

	#[test]
	fn reversed_and_out_of_unicode_ranges_are_dropped_or_clamped() {
		assert_eq!(ranges([(0x300, 0x100)]), []);
		assert_eq!(ranges([(0x11_0000, 0xffff_ffff)]), []);
		assert_eq!(ranges([(0x10_ffff, 0xffff_ffff)]), [(0x10_ffff, 0x10_ffff)]);
		assert_eq!(ranges([(0x1f600, u32::MAX)]), [(0x1f600, 0x10_ffff)]);
		assert_eq!(ranges([(u32::MAX, u32::MAX)]), []);
	}

	#[test]
	fn covers_checks_the_exact_character() {
		let set = ranges([(0x20, 0x7e), (0x23f4, 0x23f4), (0x4e00, 0x9fff)]);
		for ch in [' ', '~', '⏴', '一', '\u{9fff}'] {
			assert!(covers(&set, ch), "{ch:?}");
		}
		// The neighbours of ⏴ share its page but are not mapped.
		for ch in [
			'\u{1f}',
			'\u{7f}',
			'⏳',
			'⏵',
			'\u{4dff}',
			'\u{a000}',
			'😀',
			char::MAX,
		] {
			assert!(!covers(&set, ch), "{ch:?}");
		}
		assert!(!covers(&[], 'a'));
		assert!(covers(&[(0x10_ffff, 0x10_ffff)], char::MAX));
		assert!(covers(&[(0, 0)], '\0'));
	}

	#[test]
	fn ranges_round_trip_through_their_text_form() {
		let set = ranges([(0x20, 0x7e), (0x23f5, 0x23f5), (0x1f300, 0x1faff)]);
		assert_eq!(encode(&set), "20-7e,23f5,1f300-1faff");
		assert_eq!(decode(&encode(&set)), Some(set));
		let edges = vec![(0, 0), (0x10_ffff, 0x10_ffff)];
		assert_eq!(encode(&edges), "0,10ffff");
		assert_eq!(decode("0,10ffff"), Some(edges));
		assert_eq!(encode(&[]), "");
		assert_eq!(decode(""), Some(Vec::new()));
	}

	#[test]
	fn malformed_unmerged_or_out_of_unicode_text_is_rejected() {
		for text in [
			"x",
			"1-",
			"-1",
			"1-2-3",
			"1,,2",
			"3,1",
			"1-3,2",
			"3-1",
			"1,2",
			"1-2,3",
			"110000",
			"10ffff-110000",
			" 1",
			"+1",
			"0000001",
			"1,",
		] {
			assert_eq!(decode(text), None, "{text:?}");
		}
		assert_eq!(decode("1,3"), Some(vec![(1, 1), (3, 3)]));
		assert_eq!(decode("41-5A"), Some(vec![(0x41, 0x5a)]), "either case");
	}

	#[test]
	fn covers_agrees_with_a_linear_scan_over_many_ranges() {
		// Every 7th code point from 0x100, in runs of one to three.
		let set = ranges((0u32..3000).map(|i| (0x100 + i * 7, 0x100 + i * 7 + i % 3)));
		assert_eq!(set.len(), 3000);
		for code in 0..0x6000u32 {
			let ch = char::from_u32(code).unwrap();
			let linear = set
				.iter()
				.any(|&(first, last)| (first..=last).contains(&code));
			assert_eq!(covers(&set, ch), linear, "{ch:?}");
		}
	}

	#[test]
	fn unsorted_overlapping_ranges_give_the_same_ranges_in_any_order() {
		let input = [
			(0x4e00, 0x9fff),
			(0x20, 0x7e),
			(0x3000, 0x30ff),
			(0x9000, 0xa0ff),
			(0x7f, 0x2ff),
		];
		let expected = ranges(input);
		assert_eq!(
			expected,
			[(0x20, 0x2ff), (0x3000, 0x30ff), (0x4e00, 0xa0ff)]
		);
		let mut reversed = input;
		reversed.reverse();
		assert_eq!(ranges(reversed), expected);
	}

	#[test]
	fn repeated_sets_are_written_once_and_read_back_by_reference() {
		let latin = ranges([(0x20, 0x7e)]);
		let han = ranges([(0x20, 0x7e), (0x4e00, 0x9fff)]);
		let faces = [&latin, &han, &latin, &han, &han, &Vec::new(), &Vec::new()];
		let mut writer = Writer::default();
		let text: Vec<String> = faces.iter().map(|set| writer.write(set)).collect();
		assert_eq!(
			text,
			["20-7e", "20-7e,4e00-9fff", "=0", "=1", "=1", "", "=2"]
		);
		let mut reader = Reader::default();
		for (text, set) in text.iter().zip(faces) {
			assert_eq!(reader.read(text).as_deref(), Some(&set[..]), "{text:?}");
		}
	}

	#[test]
	fn references_to_sets_not_read_or_not_canonical_are_rejected() {
		let mut reader = Reader::default();
		assert_eq!(reader.read("=0"), None, "nothing read yet");
		assert_eq!(reader.read("41").as_deref(), Some(&[(0x41, 0x41)][..]));
		assert_eq!(reader.read("=0").as_deref(), Some(&[(0x41, 0x41)][..]));
		for text in [
			"=1",
			"=",
			"=00",
			"=01",
			"=+0",
			"=-0",
			"= 0",
			"=0x",
			"=99999999999999999999999",
		] {
			assert_eq!(reader.read(text), None, "{text:?}");
		}
		// A malformed set is not stored, so it cannot be referred to.
		assert_eq!(reader.read("2,1"), None);
		assert_eq!(reader.read("=1"), None);
		assert_eq!(reader.read("42").as_deref(), Some(&[(0x42, 0x42)][..]));
		assert_eq!(reader.read("=1").as_deref(), Some(&[(0x42, 0x42)][..]));
	}

	#[test]
	fn faces_referring_to_one_set_share_its_memory() {
		let mut reader = Reader::default();
		let first = reader.read("20-7e,4e00-9fff").unwrap();
		let again = reader.read("=0").unwrap();
		assert!(Arc::ptr_eq(&first, &again));
		// The same text written out again is a new set, not shared.
		let copy = reader.read("20-7e,4e00-9fff").unwrap();
		assert_eq!(copy, first);
		assert!(!Arc::ptr_eq(&first, &copy));
		assert!(Arc::ptr_eq(&copy, &reader.read("=1").unwrap()));
	}
}
