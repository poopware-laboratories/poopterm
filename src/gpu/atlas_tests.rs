//! Tests of the bitmap atlas: glyphs keep their slots while used, never
//! overlap, fill shelves and pages before evicting, evict the page used
//! longest ago, and report exactly the regions a backend must upload.
use super::atlas::{Atlas, Refused, Slot};

/// Whether two slots overlap or touch, which would let one bleed into the
/// other under filtering.
fn touch(a: &Slot, b: &Slot) -> bool {
	a.page == b.page
		&& a.x < b.x + b.w + 1
		&& b.x < a.x + a.w + 1
		&& a.y < b.y + b.h + 1
		&& b.y < a.y + a.h + 1
}

/// A deterministic generator of sizes in `1..=max`.
fn size(state: &mut u64, max: usize) -> usize {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	1 + (*state >> 33) as usize % max
}

#[test]
fn a_glyph_keeps_its_slot_across_frames() {
	let mut atlas = Atlas::new(64, 1);
	let slot = atlas.insert('a', (8, 12)).unwrap();
	assert_eq!(
		slot,
		Slot {
			page: 0,
			x: 0,
			y: 0,
			w: 8,
			h: 12
		}
	);
	for _ in 0..5 {
		atlas.begin_frame();
		assert_eq!(atlas.get(&'a'), Some(slot));
	}
	assert_eq!(atlas.get(&'b'), None);
	// Inserting again finds the glyph instead of placing it twice.
	assert_eq!(atlas.insert('a', (8, 12)), Ok(slot));
	assert_eq!(atlas.insert('b', (8, 12)).unwrap().x, 9);
}

#[test]
fn glyphs_of_a_height_share_a_shelf_and_short_ones_skip_tall_shelves() {
	let mut atlas = Atlas::new(64, 1);
	let a = atlas.insert(0, (10, 20)).unwrap();
	let b = atlas.insert(1, (10, 16)).unwrap();
	assert_eq!((b.x, b.y), (11, 0), "fits the 20-pixel shelf");
	// Five pixels would waste three quarters of a 20-pixel shelf.
	let c = atlas.insert(2, (10, 5)).unwrap();
	assert_eq!((c.x, c.y), (0, a.h + 1), "opens its own shelf");
	let d = atlas.insert(3, (10, 4)).unwrap();
	assert_eq!((d.x, d.y), (11, c.y), "joins the 5-pixel shelf");
	// A full shelf sends the next glyph to a new one.
	let wide = atlas.insert(4, (60, 20)).unwrap();
	assert_eq!((wide.x, wide.y), (0, c.y + c.h + 1));
}

#[test]
fn random_glyphs_never_overlap_or_touch_and_stay_in_their_page() {
	let mut atlas = Atlas::new(256, 4);
	let mut state = 0x9e37_79b9_7f4a_7c15;
	let mut slots = Vec::new();
	for key in 0..400 {
		let (w, h) = (size(&mut state, 40), size(&mut state, 40));
		match atlas.insert(key, (w, h)) {
			Ok(slot) => {
				assert_eq!((slot.w, slot.h), (w, h));
				assert!(slot.x + w <= 256 && slot.y + h <= 256, "{slot:?}");
				assert!(slot.page < 4);
				slots.push(slot);
			}
			Err(refused) => assert_eq!(refused, Refused::Full),
		}
	}
	assert!(slots.len() > 100, "only {} placed", slots.len());
	for (i, a) in slots.iter().enumerate() {
		for b in &slots[i + 1..] {
			assert!(!touch(a, b), "{a:?} and {b:?}");
		}
	}
}

#[test]
fn a_full_page_opens_another_up_to_the_limit() {
	let mut atlas = Atlas::new(32, 2);
	// Three 15×15 glyphs to a shelf would need 47 pixels: two fit a row,
	// two rows a page, so four a page.
	let pages: Vec<_> = (0..8)
		.map(|key| atlas.insert(key, (15, 15)).unwrap().page)
		.collect();
	assert_eq!(pages, [0, 0, 0, 0, 1, 1, 1, 1]);
	assert_eq!(atlas.insert(8, (15, 15)), Err(Refused::Full));
}

#[test]
fn glyphs_larger_than_a_page_are_refused_and_the_largest_fits() {
	let mut atlas = Atlas::new(32, 1);
	assert_eq!(atlas.insert(0, (33, 1)), Err(Refused::TooBig));
	assert_eq!(atlas.insert(1, (1, 33)), Err(Refused::TooBig));
	assert_eq!(atlas.get(&0), None);
	assert_eq!(atlas.insert(2, (32, 32)).unwrap().page, 0);
	assert_eq!(atlas.insert(3, (1, 1)), Err(Refused::Full));
}

#[test]
fn when_full_the_page_used_longest_ago_is_emptied_and_reused() {
	let mut atlas = Atlas::new(32, 3);
	// One 32×32 glyph fills a page: keys 0, 1, 2 on pages 0, 1, 2.
	for key in 0..3 {
		atlas.insert(key, (32, 32)).unwrap();
	}
	atlas.begin_frame();
	atlas.get(&0);
	atlas.get(&2);
	atlas.begin_frame();
	// Page 1 was last used two frames ago, the others one.
	let slot = atlas.insert(3, (32, 32)).unwrap();
	assert_eq!(slot.page, 1);
	assert_eq!(atlas.get(&1), None, "evicted with its page");
	assert_eq!(atlas.get(&0).map(|s| s.page), Some(0));
	assert_eq!(atlas.get(&2).map(|s| s.page), Some(2));
}

#[test]
fn glyphs_used_this_frame_are_never_evicted() {
	let mut atlas = Atlas::new(32, 2);
	atlas.insert(0, (32, 32)).unwrap();
	atlas.insert(1, (32, 32)).unwrap();
	assert_eq!(atlas.insert(2, (1, 1)), Err(Refused::Full));
	atlas.begin_frame();
	atlas.get(&1);
	// Only page 0 is free to go, and only once a new frame began.
	assert_eq!(atlas.insert(2, (1, 1)).unwrap().page, 0);
	assert_eq!(atlas.insert(3, (32, 32)), Err(Refused::Full));
	assert_eq!(atlas.get(&1).map(|s| s.page), Some(1));
}

#[test]
fn an_emptied_page_is_packed_from_its_corner_again() {
	let mut atlas = Atlas::new(32, 1);
	for key in 0..4 {
		atlas.insert(key, (15, 15)).unwrap();
	}
	atlas.begin_frame();
	let slot = atlas.insert(9, (20, 3)).unwrap();
	assert_eq!((slot.page, slot.x, slot.y), (0, 0, 0));
	assert!((0..4).all(|key| atlas.get(&key).is_none()));
}

#[test]
fn dirty_regions_cover_new_glyphs_per_page_and_are_taken_once() {
	let mut atlas = Atlas::new(32, 2);
	assert_eq!(atlas.take_dirty(), []);
	atlas.insert(0, (10, 9)).unwrap();
	atlas.insert(1, (6, 5)).unwrap();
	atlas.insert(2, (32, 32)).unwrap();
	assert_eq!(
		atlas.take_dirty(),
		[
			Slot {
				page: 0,
				x: 0,
				y: 0,
				w: 17,
				h: 9
			},
			Slot {
				page: 1,
				x: 0,
				y: 0,
				w: 32,
				h: 32
			},
		]
	);
	assert_eq!(atlas.take_dirty(), [], "uploaded already");
	// Found glyphs need no upload; a new one dirties only its own rectangle.
	atlas.insert(0, (10, 9)).unwrap();
	atlas.insert(3, (5, 5)).unwrap();
	assert_eq!(
		atlas.take_dirty(),
		[Slot {
			page: 0,
			x: 18,
			y: 0,
			w: 5,
			h: 5
		}]
	);
}

#[test]
fn an_evicted_page_is_dirty_only_where_new_glyphs_land() {
	let mut atlas = Atlas::new(32, 1);
	atlas.insert(0, (32, 32)).unwrap();
	atlas.take_dirty();
	atlas.begin_frame();
	atlas.insert(1, (3, 7)).unwrap();
	// Old pixels elsewhere are never sampled again, so they need no clearing.
	assert_eq!(
		atlas.take_dirty(),
		[Slot {
			page: 0,
			x: 0,
			y: 0,
			w: 3,
			h: 7
		}]
	);
}

#[test]
fn a_glyph_half_a_shelf_tall_fits_it_up_to_the_page_edge() {
	let mut atlas = Atlas::new(32, 1);
	atlas.insert(0, (15, 20)).unwrap();
	// Ten pixels is exactly half the shelf, and 16 + 16 the page width.
	let slot = atlas.insert(1, (16, 10)).unwrap();
	assert_eq!((slot.x, slot.y), (16, 0));
}

#[test]
fn the_tightest_shelf_with_room_wins() {
	let mut atlas = Atlas::new(64, 1);
	atlas.insert(0, (50, 20)).unwrap();
	// Too wide for the rest of the first shelf, so it opens a second.
	assert_eq!(atlas.insert(1, (20, 12)).unwrap().y, 21);
	// Both shelves have room for this; the 12-pixel one wastes less.
	assert_eq!(atlas.insert(2, (10, 10)).unwrap().y, 21);
	assert_eq!(atlas.insert(3, (10, 19)).unwrap().y, 0);
}

#[test]
fn dirty_regions_grow_left_up_and_right() {
	let mut atlas = Atlas::new(64, 1);
	atlas.insert(0, (10, 20)).unwrap();
	atlas.insert(1, (50, 8)).unwrap();
	atlas.take_dirty();
	// First on the lower shelf, then back on the upper one, further right.
	assert_eq!(
		atlas.insert(2, (5, 8)).unwrap(),
		Slot {
			page: 0,
			x: 51,
			y: 21,
			w: 5,
			h: 8
		}
	);
	assert_eq!(
		atlas.insert(3, (4, 20)).unwrap(),
		Slot {
			page: 0,
			x: 11,
			y: 0,
			w: 4,
			h: 20
		}
	);
	assert_eq!(atlas.insert(4, (20, 6)).unwrap().y, 30);
	assert_eq!(
		atlas.take_dirty(),
		[Slot {
			page: 0,
			x: 0,
			y: 0,
			w: 56,
			h: 36
		}]
	);
}

#[test]
fn a_glyph_too_tall_for_a_page_evicts_nothing() {
	let mut atlas = Atlas::new(32, 1);
	atlas.insert(0, (32, 32)).unwrap();
	atlas.begin_frame();
	assert_eq!(atlas.insert(1, (1, 33)), Err(Refused::TooBig));
	assert!(atlas.get(&0).is_some());
}

#[test]
fn a_glyph_takes_room_on_one_page_only() {
	let mut atlas = Atlas::new(32, 2);
	atlas.insert(0, (32, 20)).unwrap();
	atlas.insert(1, (32, 20)).unwrap();
	atlas.take_dirty();
	// Both pages have a free strip; the first gets the glyph and keeps
	// the second's for the next.
	assert_eq!(atlas.insert(2, (32, 10)).unwrap().page, 0);
	assert_eq!(atlas.take_dirty().len(), 1);
	assert_eq!(atlas.insert(3, (32, 10)).unwrap().page, 1);
}

#[test]
fn an_atlas_without_pages_holds_nothing() {
	let mut atlas = Atlas::new(32, 0);
	assert_eq!(atlas.insert('a', (1, 1)), Err(Refused::Full));
	assert_eq!(atlas.take_dirty(), []);
}

#[test]
fn empty_bitmaps_get_a_slot_but_nothing_to_upload() {
	let mut atlas = Atlas::new(32, 1);
	let empty = atlas.insert(0, (0, 7)).unwrap();
	assert_eq!((empty.w, empty.h), (0, 7));
	assert_eq!(atlas.insert(1, (5, 0)).unwrap().h, 0);
	// A GPU copy of zero pixels is invalid, so none is asked for.
	assert_eq!(atlas.take_dirty(), []);
	atlas.insert(2, (3, 3)).unwrap();
	assert_eq!(atlas.take_dirty().len(), 1);
}

#[test]
fn of_pages_equally_old_the_first_is_emptied() {
	let mut atlas = Atlas::new(32, 3);
	for key in 0..3 {
		atlas.insert(key, (32, 32)).unwrap();
	}
	atlas.begin_frame();
	assert_eq!(atlas.insert(3, (32, 32)).unwrap().page, 0);
	assert_eq!(atlas.insert(4, (32, 32)).unwrap().page, 1);
	assert_eq!(atlas.insert(5, (32, 32)).unwrap().page, 2);
	assert_eq!(atlas.insert(6, (1, 1)), Err(Refused::Full));
}

/// The bitmap size of `key`: emoji-like squares and wide or tall strips,
/// the same every time the key is asked for.
fn size_of(key: u32) -> (usize, usize) {
	let mut state = u64::from(key) * 0x9e37_79b9 + 1;
	match key % 4 {
		0 => (size(&mut state, 60), size(&mut state, 8)),
		1 => (size(&mut state, 8), size(&mut state, 60)),
		_ => (24 + size(&mut state, 16), 24 + size(&mut state, 16)),
	}
}

/// What the test knows of the atlas between steps.
#[derive(Default)]
struct Model {
	frame: u64,
	/// The frame each page was last handed out in, by `get` or `insert`.
	used: Vec<u64>,
	/// Slots placed since dirty regions were last taken.
	fresh: Vec<(u32, Slot)>,
}

impl Model {
	fn touch(&mut self, slot: Slot) {
		if self.used.len() <= slot.page {
			self.used.resize(slot.page + 1, 0);
		}
		self.used[slot.page] = self.frame;
	}
}

/// Checks what one step changed, given the frame and when each page was
/// last used before it: live slots never touch and stay put, and a
/// slot goes only with its whole page, the one used longest ago and not
/// this frame, emptied for the bitmap just placed.
fn check_step(
	(frame, used): (u64, &[u64]),
	before: &[(u32, Slot)],
	atlas: &Atlas<u32>,
	placed: Option<(u32, Slot)>,
) {
	let after: Vec<(u32, Slot)> = atlas.slots().map(|(&k, &s)| (k, s)).collect();
	for (i, (key, a)) in after.iter().enumerate() {
		assert!(a.x + a.w <= 128 && a.y + a.h <= 128 && a.page < 3, "{a:?}");
		assert_eq!((a.w, a.h), size_of(*key));
		for (_, b) in &after[i + 1..] {
			assert!(!touch(a, b), "{a:?} and {b:?}");
		}
	}
	let live = |key: u32| after.iter().find(|(k, _)| *k == key).map(|(_, s)| *s);
	for &(key, slot) in before {
		match live(key) {
			Some(now) => assert_eq!(now, slot, "key {key} moved"),
			None => {
				let (_, new) = placed.expect("only placing a bitmap evicts");
				assert_eq!(new.page, slot.page, "key {key} left another page");
				let others = (0..used.len()).filter(|&page| page != slot.page);
				let oldest = used[slot.page];
				assert!(oldest < frame, "page used this frame");
				assert!(others.map(|page| used[page]).all(|other| other >= oldest));
			}
		}
	}
}

#[test]
fn random_steps_keep_every_promise_of_the_atlas() {
	let mut atlas = Atlas::new(128, 3);
	let mut model = Model::default();
	let mut state = 0x2545_f491_4f6c_dd1d;
	let (mut evictions, mut refusals) = (0, 0);
	for _ in 0..4000 {
		let before: Vec<(u32, Slot)> = atlas.slots().map(|(&k, &s)| (k, s)).collect();
		let used = model.used.clone();
		let key = size(&mut state, 120) as u32;
		let mut placed = None;
		match size(&mut state, 20) {
			1 => {
				atlas.begin_frame();
				model.frame += 1;
			}
			2 => {
				let mut dirty = atlas.take_dirty();
				dirty.sort_by_key(|rect| rect.page);
				assert_eq!(dirty, bounds(&model.fresh, &atlas));
				model.fresh.clear();
			}
			3..=8 => match atlas.get(&key) {
				Some(slot) => model.touch(slot),
				None => assert!(before.iter().all(|(k, _)| *k != key)),
			},
			_ => match atlas.insert(key, size_of(key)) {
				Ok(slot) => {
					model.touch(slot);
					if before.iter().all(|(k, _)| *k != key) {
						placed = Some((key, slot));
						model.fresh.push((key, slot));
					}
				}
				Err(refused) => {
					assert_eq!(refused, Refused::Full);
					assert_eq!(model.used.len(), 3);
					assert!(model.used.iter().all(|&used| used == model.frame));
					refusals += 1;
				}
			},
		}
		let gone = before
			.iter()
			.filter(|(k, _)| atlas.slots().all(|(a, _)| a != k));
		evictions += usize::from(gone.count() > 0);
		check_step((model.frame, &used), &before, &atlas, placed);
	}
	assert!(
		evictions > 20 && refusals > 0,
		"{evictions} evictions, {refusals} refusals"
	);
}

/// The bounding box, per page, of the fresh slots still there with pixels.
fn bounds(fresh: &[(u32, Slot)], atlas: &Atlas<u32>) -> Vec<Slot> {
	let mut boxes: Vec<Slot> = Vec::new();
	let live = fresh
		.iter()
		.filter(|(key, slot)| atlas.slots().any(|(k, s)| k == key && s == slot));
	for (_, s) in live.filter(|(_, s)| s.w > 0 && s.h > 0) {
		match boxes.iter_mut().find(|b| b.page == s.page) {
			Some(b) => {
				let (right, bottom) = ((b.x + b.w).max(s.x + s.w), (b.y + b.h).max(s.y + s.h));
				(b.x, b.y) = (b.x.min(s.x), b.y.min(s.y));
				(b.w, b.h) = (right - b.x, bottom - b.y);
			}
			None => boxes.push(*s),
		}
	}
	boxes.sort_by_key(|b| b.page);
	boxes
}

#[test]
fn emoji_sized_bitmaps_fill_most_of_a_page_before_the_next() {
	// Emoji at a 2× scale: two cells of 18 to 20 pixels, a little less tall.
	let mut atlas = Atlas::new(1024, 2);
	let mut state = 0x1234_5678_9abc_def1;
	let mut area = 0;
	for key in 0.. {
		let (w, h) = (36 + size(&mut state, 4), 32 + size(&mut state, 8));
		if atlas.insert(key, (w, h)).unwrap().page == 1 {
			break;
		}
		area += w * h;
	}
	let filled = area as f64 / (1024.0 * 1024.0);
	assert!(filled > 0.8, "page 0 only {:.0}% full", filled * 100.0);
}

#[test]
fn a_bitmap_evicted_and_asked_for_again_is_uploaded_again() {
	let mut atlas = Atlas::new(32, 1);
	atlas.insert('a', (20, 20)).unwrap();
	atlas.take_dirty();
	atlas.begin_frame();
	atlas.insert('b', (32, 32)).unwrap();
	atlas.take_dirty();
	assert_eq!(atlas.get(&'a'), None);
	atlas.begin_frame();
	// Its pixels were drawn over, so the backend must send them again.
	let again = atlas.insert('a', (20, 20)).unwrap();
	assert_eq!(atlas.take_dirty(), [again]);
	assert_eq!(atlas.get(&'b'), None);
}
