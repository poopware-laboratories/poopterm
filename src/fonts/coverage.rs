//! Which Unicode pages (runs of 256 code points) a font maps, kept in the
//! font database so fallback only opens fonts that may have a character.
/// Inclusive ranges of pages (`code point >> 8`), sorted and merged.
pub(crate) type Pages = Vec<(u16, u16)>;

/// The last page: U+10FF00 to U+10FFFF.
const LAST_PAGE: u32 = 0x10FF;

/// The pages touched by inclusive code point `ranges`, in any order.
pub(crate) fn pages(ranges: impl IntoIterator<Item = (u32, u32)>) -> Pages {
	let mut spans: Vec<(u16, u16)> = ranges
		.into_iter()
		.filter(|&(first, last)| first <= last && first >> 8 <= LAST_PAGE)
		.map(|(first, last)| ((first >> 8) as u16, (last >> 8).min(LAST_PAGE) as u16))
		.collect();
	spans.sort_unstable();
	let mut merged: Pages = Vec::with_capacity(spans.len());
	for (first, last) in spans {
		match merged.last_mut() {
			Some((_, end)) if first <= end.saturating_add(1) => *end = (*end).max(last),
			_ => merged.push((first, last)),
		}
	}
	merged
}

/// Whether `ch` falls in one of `pages`.
pub(crate) fn covers(pages: &[(u16, u16)], ch: char) -> bool {
	let page = (u32::from(ch) >> 8) as u16;
	let after = pages.partition_point(|&(first, _)| first <= page);
	after > 0 && pages[after - 1].1 >= page
}

/// `pages` as text for the cache file: `0-4,1e,4e-9f` in hex.
pub(crate) fn encode(pages: &[(u16, u16)]) -> String {
	let spans: Vec<String> = pages
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

/// Parses [`encode`]'s output; `None` for anything else.
pub(crate) fn decode(text: &str) -> Option<Pages> {
	if text.is_empty() {
		return Some(Vec::new());
	}
	let page = |hex: &str| u16::from_str_radix(hex, 16).ok();
	let spans = text.split(',').map(|span| match span.split_once('-') {
		Some((first, last)) => Some((page(first)?, page(last)?)),
		None => page(span).map(|single| (single, single)),
	});
	let spans: Pages = spans.collect::<Option<_>>()?;
	(pages(
		spans
			.iter()
			.map(|&(a, b)| (u32::from(a) << 8, u32::from(b) << 8)),
	) == spans)
		.then_some(spans)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ranges_become_sorted_merged_pages() {
		assert_eq!(pages([(0x20, 0x7e)]), [(0, 0)]);
		assert_eq!(
			pages([(0x4e00, 0x9fff), (0x20, 0x17f)]),
			[(0, 1), (0x4e, 0x9f)]
		);
		// Neighbouring and overlapping pages merge; a gap keeps them apart.
		assert_eq!(pages([(0x100, 0x1ff), (0x200, 0x2ff)]), [(1, 2)]);
		assert_eq!(pages([(0x100, 0x3ff), (0x250, 0x260)]), [(1, 3)]);
		assert_eq!(pages([(0x100, 0x1ff), (0x300, 0x3ff)]), [(1, 1), (3, 3)]);
		assert_eq!(pages([]), []);
	}

	#[test]
	fn a_range_ending_on_a_page_boundary_stays_in_its_page() {
		assert_eq!(pages([(0xff, 0xff)]), [(0, 0)]);
		assert_eq!(pages([(0x1ff, 0x200)]), [(1, 2)]);
		assert_eq!(pages([(0x2600, 0x26ff)]), [(0x26, 0x26)]);
	}

	#[test]
	fn reversed_and_out_of_unicode_ranges_are_dropped_or_clamped() {
		assert_eq!(pages([(0x300, 0x100)]), []);
		assert_eq!(pages([(0x11_0000, 0xffff_ffff)]), []);
		assert_eq!(pages([(0x10_ff00, 0xffff_ffff)]), [(0x10ff, 0x10ff)]);
		assert_eq!(pages([(0x1f600, u32::MAX)]), [(0x1f6, 0x10ff)]);
	}

	#[test]
	fn covers_checks_the_page_of_a_character() {
		let set = pages([(0x20, 0x7e), (0x2500, 0x25ff), (0x4e00, 0x9fff)]);
		for ch in [' ', '\u{ff}', '─', '\u{25ff}', '一', '\u{9fff}'] {
			assert!(covers(&set, ch), "{ch:?}");
		}
		for ch in [
			'Ā',
			'\u{24ff}',
			'\u{2600}',
			'\u{4dff}',
			'\u{a000}',
			'😀',
			char::MAX,
		] {
			assert!(!covers(&set, ch), "{ch:?}");
		}
		assert!(!covers(&[], 'a'));
		assert!(covers(&[(0x10ff, 0x10ff)], char::MAX));
	}

	#[test]
	fn pages_round_trip_through_their_text_form() {
		let set = pages([(0, 0x4ff), (0x2600, 0x26ff), (0x1f300, 0x1faff)]);
		assert_eq!(encode(&set), "0-4,26,1f3-1fa");
		assert_eq!(decode(&encode(&set)), Some(set));
		assert_eq!(encode(&[]), "");
		assert_eq!(decode(""), Some(Vec::new()));
	}

	#[test]
	fn malformed_or_unordered_page_text_is_rejected() {
		for text in [
			"x", "1-", "-1", "1-2-3", "1,,2", "2,1", "1-3,2", "3-1", "1,2", "10000", " 1",
		] {
			assert_eq!(decode(text), None, "{text:?}");
		}
		assert_eq!(decode("1,3"), Some(vec![(1, 1), (3, 3)]));
	}

	#[test]
	fn covers_agrees_with_a_linear_scan_over_many_spans() {
		// Every 7th page, in spans of one to three pages.
		let ranges = (0u32..300).map(|i| ((i * 7) << 8, ((i * 7 + i % 3) << 8) + 0xff));
		let set = pages(ranges);
		assert_eq!(set.len(), 300);
		for page in 0..0x1100u32 {
			let ch = char::from_u32(page << 8 | 0x41).unwrap_or('\u{fffd}');
			let page = (u32::from(ch) >> 8) as u16;
			let linear = set
				.iter()
				.any(|&(first, last)| (first..=last).contains(&page));
			assert_eq!(covers(&set, ch), linear, "{ch:?}");
		}
	}

	#[test]
	fn unsorted_overlapping_ranges_give_the_same_pages_in_any_order() {
		let ranges = [
			(0x4e00, 0x9fff),
			(0x20, 0x7e),
			(0x3000, 0x30ff),
			(0x9000, 0xa0ff),
			(0x100, 0x2ff),
		];
		let expected = pages(ranges);
		assert_eq!(expected, [(0, 2), (0x30, 0x30), (0x4e, 0xa0)]);
		let mut reversed = ranges;
		reversed.reverse();
		assert_eq!(pages(reversed), expected);
	}
}
