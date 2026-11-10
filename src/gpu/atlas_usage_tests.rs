//! Tests of how full the atlas says it is: the area its shelves hold, gaps
//! included, against all the pages it may use.
use super::atlas::{Atlas, Usage};

#[test]
fn an_empty_atlas_uses_nothing_of_its_pages() {
	let atlas: Atlas<char> = Atlas::new(2048, 4);
	let usage = atlas.usage();
	assert_eq!(
		usage,
		Usage {
			side: 2048,
			pages: 4,
			filled: 0
		}
	);
	assert_eq!(usage.percent(), 0);
}

#[test]
fn a_bitmap_fills_its_width_and_the_gap_after_it_on_its_shelf() {
	let mut atlas = Atlas::new(64, 1);
	atlas.insert('a', (8, 12)).unwrap();
	// 9 columns (8 and the gap) of a 12-row shelf.
	assert_eq!(atlas.usage().filled, 9 * 12);
	atlas.insert('b', (8, 12)).unwrap();
	assert_eq!(atlas.usage().filled, 18 * 12);
}

#[test]
fn a_shorter_bitmap_on_a_taller_shelf_uses_the_shelf_height() {
	// The shelf's height is spent whatever sits on it.
	let mut atlas = Atlas::new(64, 1);
	atlas.insert('a', (8, 12)).unwrap();
	atlas.insert('b', (4, 7)).unwrap();
	assert_eq!(atlas.usage().filled, (9 + 5) * 12);
}

#[test]
fn every_page_counts_toward_the_whole() {
	let mut atlas = Atlas::new(16, 2);
	// Fills page 0 with one 16 × 16 bitmap, then starts page 1.
	atlas.insert('a', (16, 16)).unwrap();
	atlas.insert('b', (16, 16)).unwrap();
	let usage = atlas.usage();
	assert_eq!(usage.filled, 2 * 17 * 16);
	assert_eq!(usage.percent(), 100);
}

#[test]
fn an_evicted_page_is_empty_again() {
	let mut atlas = Atlas::new(16, 1);
	atlas.insert('a', (16, 8)).unwrap();
	let before = atlas.usage().filled;
	atlas.begin_frame();
	// 'b' needs the whole page: 'a' is evicted with it.
	atlas.insert('b', (16, 16)).unwrap();
	assert_eq!(before, 17 * 8);
	assert_eq!(atlas.usage().filled, 17 * 16);
}

#[test]
fn the_percentage_rounds_down_and_never_passes_100() {
	// 400 pixels: 4 make 1%.
	let usage = |filled| {
		Usage {
			side: 20,
			pages: 1,
			filled,
		}
		.percent()
	};
	assert_eq!(usage(0), 0);
	assert_eq!(usage(3), 0);
	assert_eq!(usage(4), 1);
	assert_eq!(usage(399), 99);
	assert_eq!(usage(400), 100);
	// Gaps at a shelf's end can count past the page.
	assert_eq!(usage(420), 100);
	let none = Usage {
		side: 0,
		pages: 0,
		filled: 0,
	};
	assert_eq!(none.percent(), 0);
}

#[test]
fn many_bitmaps_never_fill_more_than_the_pages_hold_but_the_gaps() {
	let mut atlas = Atlas::new(128, 2);
	let mut state = 0x9e37_79b9_7f4a_7c15_u64;
	for key in 0..2000u32 {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		let (w, h) = (1 + (state % 20) as usize, 1 + (state >> 20) as usize % 20);
		atlas.begin_frame();
		atlas.insert(key, (w, h)).unwrap();
		let usage = atlas.usage();
		// A shelf's last gap may lie past the page's edge, one column a row.
		assert!(usage.filled <= 2 * 129 * 128, "{usage:?}");
		assert!(usage.percent() <= 100);
	}
}

#[test]
fn a_pen_reports_its_atlas_as_made_and_as_filled() {
	use crate::gpu::quads::QuadPen;
	let mut pen = QuadPen::new((40, 30), (64, 2));
	assert_eq!(
		pen.atlas_usage(),
		Usage {
			side: 64,
			pages: 2,
			filled: 0
		}
	);
	pen
		.atlas
		.insert(vec![1].into_boxed_slice(), (10, 10))
		.unwrap();
	assert_eq!(pen.atlas_usage().filled, 11 * 10);
}
