//! Tests of the curve store: records keep their words and place while used,
//! fill pages before evicting, evict the page used longest ago, and report
//! exactly the words a backend must upload.
#![expect(
	clippy::single_range_in_vec_init,
	reason = "dirty words are a list of ranges, often of one"
)]
use super::{
	atlas::Refused,
	bands::Banded,
	curves::{Record, Store},
	record::encode,
};
use crate::fonts::{BUNDLED, Font};

fn words(key: u32, len: usize) -> Vec<u32> {
	(0..len as u32).map(|i| key << 16 | i).collect()
}

#[test]
fn a_record_is_written_once_and_found_across_frames() {
	let mut store = Store::new(64, 1);
	let record = store.insert('a', &words(1, 3)).unwrap();
	assert_eq!(record, Record { start: 0, len: 3 });
	assert_eq!(&store.data()[..3], words(1, 3));
	for _ in 0..3 {
		store.begin_frame();
		assert_eq!(store.get(&'a'), Some(record));
	}
	assert_eq!(store.get(&'b'), None);
	// A key names its glyph, so asking again finds it without writing.
	assert_eq!(store.insert('a', &words(2, 5)), Ok(record));
	assert_eq!(&store.data()[..3], words(1, 3));
	assert_eq!(store.insert('b', &words(3, 2)).unwrap().start, 3);
}

#[test]
fn records_follow_each_other_then_open_the_next_page() {
	let mut store = Store::new(10, 2);
	let starts: Vec<_> = (0..4)
		.map(|key| store.insert(key, &words(key, 4)).unwrap().start)
		.collect();
	assert_eq!(starts, [0, 4, 10, 14]);
	assert_eq!(store.data().len(), 20);
	// Two words are left on each page.
	assert_eq!(store.insert(4, &words(4, 3)), Err(Refused::Full));
	assert_eq!(store.insert(5, &words(5, 2)).unwrap().start, 8);
	assert_eq!(store.insert(6, &words(6, 2)).unwrap().start, 18);
}

#[test]
fn a_record_may_fill_a_page_but_not_exceed_it() {
	let mut store = Store::new(16, 1);
	assert_eq!(store.insert(0, &words(0, 17)), Err(Refused::TooBig));
	assert_eq!(store.insert(1, &words(1, 16)).unwrap().len, 16);
	store.begin_frame();
	assert_eq!(store.insert(2, &words(2, 17)), Err(Refused::TooBig));
	assert!(store.get(&1).is_some(), "a refused record evicts nothing");
}

#[test]
fn when_full_the_page_used_longest_ago_is_emptied_and_reused() {
	let mut store = Store::new(8, 3);
	for key in 0..3 {
		store.insert(key, &words(key, 8)).unwrap();
	}
	store.begin_frame();
	store.get(&0);
	store.get(&2);
	store.begin_frame();
	let record = store.insert(3, &words(3, 5)).unwrap();
	assert_eq!(record.start, 8, "page 1, used two frames ago");
	assert_eq!(&store.data()[8..13], words(3, 5));
	assert_eq!(store.get(&1), None);
	assert_eq!(store.get(&0).unwrap().start, 0);
	assert_eq!(store.get(&2).unwrap().start, 16);
}

#[test]
fn records_used_this_frame_are_never_evicted() {
	let mut store = Store::new(8, 2);
	store.insert(0, &words(0, 8)).unwrap();
	store.insert(1, &words(1, 8)).unwrap();
	assert_eq!(store.insert(2, &words(2, 1)), Err(Refused::Full));
	store.begin_frame();
	store.get(&1);
	assert_eq!(store.insert(2, &words(2, 1)).unwrap().start, 0);
	assert_eq!(store.insert(3, &words(3, 8)), Err(Refused::Full));
	assert_eq!(store.get(&1).unwrap().start, 8);
}

#[test]
fn of_pages_equally_old_the_first_is_emptied() {
	let mut store = Store::new(4, 2);
	store.insert(0, &words(0, 4)).unwrap();
	store.insert(1, &words(1, 4)).unwrap();
	store.begin_frame();
	assert_eq!(store.insert(2, &words(2, 4)).unwrap().start, 0);
	assert_eq!(store.insert(3, &words(3, 4)).unwrap().start, 4);
}

#[test]
fn dirty_ranges_cover_new_records_per_page_and_are_taken_once() {
	let mut store = Store::new(16, 2);
	assert_eq!(store.take_dirty(), []);
	store.insert(0, &words(0, 5)).unwrap();
	store.insert(1, &words(1, 6)).unwrap();
	store.insert(2, &words(2, 9)).unwrap();
	assert_eq!(store.take_dirty(), [0..11, 16..25]);
	assert_eq!(store.take_dirty(), []);
	store.insert(0, &words(0, 5)).unwrap();
	store.insert(3, &words(3, 2)).unwrap();
	assert_eq!(store.take_dirty(), [11..13]);
}

#[test]
fn an_emptied_page_is_dirty_only_where_new_records_land() {
	let mut store = Store::new(16, 1);
	store.insert(0, &words(0, 16)).unwrap();
	store.take_dirty();
	store.begin_frame();
	store.insert(1, &words(1, 3)).unwrap();
	assert_eq!(store.take_dirty(), [0..3]);
	// The old words past the new record are never read again.
	assert_eq!(store.data()[3], words(0, 16)[3]);
}

#[test]
fn empty_records_are_found_but_never_uploaded() {
	let mut store = Store::new(16, 1);
	assert_eq!(store.insert(0, &[]), Ok(Record { start: 0, len: 0 }));
	assert_eq!(store.get(&0), Some(Record { start: 0, len: 0 }));
	// Zero-length GPU copies are invalid.
	assert_eq!(store.take_dirty(), []);
	assert_eq!(store.insert(1, &words(1, 2)).unwrap().start, 0);
	assert_eq!(store.take_dirty(), [0..2]);
}

#[test]
fn a_store_without_pages_holds_nothing() {
	let mut store = Store::new(16, 0);
	assert_eq!(store.insert('a', &words(0, 1)), Err(Refused::Full));
	assert_eq!(store.data(), []);
	assert_eq!(store.take_dirty(), []);
}

#[test]
fn a_refused_record_writes_and_dirties_nothing() {
	let mut store = Store::new(8, 1);
	store.insert(0, &words(0, 6)).unwrap();
	store.take_dirty();
	assert_eq!(store.insert(1, &words(1, 3)), Err(Refused::Full));
	assert_eq!(store.insert(2, &words(2, 9)), Err(Refused::TooBig));
	assert_eq!(store.take_dirty(), []);
	assert_eq!(store.data()[6..], [0, 0]);
	assert_eq!((store.get(&1), store.get(&2)), (None, None));
}

#[test]
fn bundled_glyphs_read_back_from_the_buffer() {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	let glyphs: Vec<Banded> = ('!'..='~')
		.filter_map(|ch| Banded::new(font.quads(font.lookup_glyph_index(ch))))
		.collect();
	// Pages that just hold the largest glyph, so the glyphs spread over
	// several and some are evicted.
	let largest = glyphs
		.iter()
		.map(|banded| encode(banded).len())
		.max()
		.unwrap();
	let mut store = Store::new(largest, 4);
	for round in 0..3 {
		store.begin_frame();
		for (key, banded) in glyphs.iter().enumerate().skip(round * 20) {
			let Ok(record) = store.insert(key, &encode(banded)) else {
				continue;
			};
			let words = &store.data()[record.start..record.start + record.len];
			assert_eq!(words, encode(banded), "glyph {key} in round {round}");
		}
	}
	assert!(
		glyphs
			.iter()
			.enumerate()
			.any(|(key, _)| store.get(&key).is_none())
	);
}

/// A deterministic generator of numbers in `1..=max`.
fn size(state: &mut u64, max: usize) -> usize {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	1 + (*state >> 33) as usize % max
}

/// The record length of `key`, the same every time.
fn length(key: u32) -> usize {
	1 + (key as usize * 37) % 60
}

/// The record of `key`, leaving its page unmarked.
fn peek(store: &Store<u32>, key: u32) -> Option<Record> {
	store.records().find(|(k, _)| **k == key).map(|(_, r)| *r)
}

#[test]
fn random_steps_keep_every_promise_of_the_store() {
	const PAGE: usize = 128;
	let mut store = Store::new(PAGE, 3);
	let mut state = 0x2545_f491_4f6c_dd1d;
	let (mut frame, mut used) = (0u64, vec![0u64; 3]);
	let mut fresh: Vec<(u32, Record)> = Vec::new();
	let (mut evictions, mut refusals) = (0, 0);
	for _ in 0..4000 {
		let before: Vec<(u32, Record)> = store.records().map(|(&k, &r)| (k, r)).collect();
		let used_before = used.clone();
		let key = size(&mut state, 100) as u32;
		let mut placed = None;
		match size(&mut state, 20) {
			1 => {
				store.begin_frame();
				frame += 1;
			}
			2 => {
				let live = |(key, record): &&(u32, Record)| peek(&store, *key) == Some(*record);
				let mut expected: Vec<std::ops::Range<usize>> = Vec::new();
				for (_, r) in fresh.iter().filter(live) {
					let page = r.start / PAGE;
					match expected.iter_mut().find(|range| range.start / PAGE == page) {
						Some(range) => {
							*range = range.start.min(r.start)..range.end.max(r.start + r.len);
						}
						None => expected.push(r.start..r.start + r.len),
					}
				}
				expected.sort_by_key(|range| range.start);
				assert_eq!(store.take_dirty(), expected);
				fresh.clear();
			}
			3..=8 => {
				if let Some(record) = store.get(&key) {
					used[record.start / PAGE] = frame;
				}
			}
			_ => match store.insert(key, &words(key, length(key))) {
				Ok(record) => {
					used[record.start / PAGE] = frame;
					if before.iter().all(|(k, _)| *k != key) {
						placed = Some(record);
						fresh.push((key, record));
					}
				}
				Err(refused) => {
					assert_eq!(refused, Refused::Full);
					assert_eq!(store.data().len(), 3 * PAGE);
					assert!(used.iter().all(|&u| u == frame));
					refusals += 1;
				}
			},
		}
		// Live records lie inside one page, apart, holding their own words.
		let after: Vec<(u32, Record)> = store.records().map(|(&k, &r)| (k, r)).collect();
		for (i, (key, a)) in after.iter().enumerate() {
			assert_eq!(a.len, length(*key));
			let end = a.start + a.len;
			assert_eq!((end - 1) / PAGE, a.start / PAGE, "{a:?}");
			assert_eq!(&store.data()[a.start..end], words(*key, a.len), "{key}");
			for (_, b) in &after[i + 1..] {
				assert!(end <= b.start || b.start + b.len <= a.start, "{a:?} {b:?}");
			}
		}
		// A record goes only with its whole page: the one used longest ago,
		// not this frame, emptied for the record just placed.
		for &(key, record) in &before {
			if peek(&store, key).is_none() {
				let page = record.start / PAGE;
				let new = placed.expect("only placing a record evicts");
				assert_eq!(new.start / PAGE, page, "key {key} left another page");
				assert!(used_before[page] < frame, "page used this frame");
				assert!(used_before.iter().all(|&u| u >= used_before[page]));
				evictions += 1;
			}
		}
	}
	assert!(
		evictions > 20 && refusals > 0,
		"{evictions} evictions, {refusals} refusals"
	);
}
