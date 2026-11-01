//! Tests of the image textures kept between frames: found again by their
//! bitmap alone, dropped with it, evicted least recently used first within
//! the budget, and never evicted while the frame being drawn uses them.
use super::textures::{Textures, runs};
use crate::graphics::Bitmap;
use std::{ops::Range, sync::Arc};

/// A bitmap `width` × `height`, 4 bytes a pixel.
fn bitmap(width: usize, height: usize) -> Arc<Bitmap> {
	Arc::new(Bitmap::new(
		width,
		height,
		vec![0xff00_0000; width * height],
	))
}

/// Starts a frame, then gets `bitmap`'s texture, inserting `name` if it is
/// new and there is room: the name of the texture drawn, if any.
fn draw(
	textures: &mut Textures<&'static str>,
	bitmap: &Arc<Bitmap>,
	name: &'static str,
) -> Option<&'static str> {
	if let Some(&texture) = textures.get(bitmap) {
		return Some(texture);
	}
	textures
		.make_room(bitmap)
		.then(|| *textures.insert(bitmap, name))
}

#[test]
fn a_texture_is_found_by_its_bitmap_and_not_by_an_equal_one() {
	let mut textures = Textures::new(1024);
	let (first, equal) = (bitmap(2, 2), bitmap(2, 2));
	textures.begin_frame();
	assert_eq!(textures.get(&first), None);
	assert!(textures.make_room(&first));
	assert_eq!(*textures.insert(&first, "first"), "first");
	assert_eq!(textures.get(&first), Some(&"first"));
	assert_eq!(textures.get(&Arc::clone(&first)), Some(&"first"));
	// Equal pixels, another image: animation frames are new bitmaps.
	assert_eq!(textures.get(&equal), None);
	textures.begin_frame();
	assert_eq!(textures.get(&first), Some(&"first"));
}

#[test]
fn a_texture_goes_once_its_bitmap_is_dropped() {
	let mut textures = Textures::new(1024);
	let image = bitmap(4, 4);
	textures.begin_frame();
	assert_eq!(draw(&mut textures, &image, "image"), Some("image"));
	assert_eq!(textures.bytes(), 64);
	drop(image);
	// Still held until the next frame starts.
	assert_eq!((textures.len(), textures.bytes()), (1, 64));
	textures.begin_frame();
	assert_eq!((textures.len(), textures.bytes()), (0, 0));
}

#[test]
fn room_is_made_by_evicting_the_least_recently_used_first() {
	// Room for three 2×2 bitmaps of 16 bytes each.
	let mut textures = Textures::new(48);
	let [a, b, c, d] = [(); 4].map(|()| bitmap(2, 2));
	textures.begin_frame();
	draw(&mut textures, &a, "a");
	textures.begin_frame();
	draw(&mut textures, &b, "b");
	textures.begin_frame();
	draw(&mut textures, &c, "c");
	textures.begin_frame();
	// `a` is used again, so `b` is now the least recently used.
	assert_eq!(draw(&mut textures, &a, "a again"), Some("a"));
	assert_eq!(draw(&mut textures, &d, "d"), Some("d"));
	assert_eq!(textures.get(&b), None);
	for (bitmap, name) in [(&a, "a"), (&c, "c"), (&d, "d")] {
		assert_eq!(textures.get(bitmap), Some(&name));
	}
	assert_eq!((textures.len(), textures.bytes()), (3, 48));
}

#[test]
fn textures_the_frame_uses_are_never_evicted() {
	let mut textures = Textures::new(48);
	let [a, b, c, d] = [(); 4].map(|()| bitmap(2, 2));
	textures.begin_frame();
	for (bitmap, name) in [(&a, "a"), (&b, "b"), (&c, "c")] {
		assert_eq!(draw(&mut textures, bitmap, name), Some(name));
	}
	// Full of textures this frame draws: no room for a fourth.
	assert!(!textures.make_room(&d));
	assert_eq!((textures.len(), textures.bytes()), (3, 48));
	// The next frame, only `c` is drawn before `d`: `a` goes, `b` stays.
	textures.begin_frame();
	assert_eq!(draw(&mut textures, &c, "c"), Some("c"));
	assert_eq!(draw(&mut textures, &d, "d"), Some("d"));
	assert_eq!(textures.get(&a), None);
	assert_eq!(textures.get(&b), Some(&"b"));
}

#[test]
fn a_bitmap_larger_than_the_budget_never_gets_room() {
	let mut textures = Textures::new(48);
	let (small, large) = (bitmap(2, 2), bitmap(4, 4));
	textures.begin_frame();
	draw(&mut textures, &small, "small");
	textures.begin_frame();
	assert!(!textures.make_room(&large));
	// Nothing was evicted in vain.
	assert_eq!(textures.get(&small), Some(&"small"));
	// Exactly the budget fits, once the rest is evicted.
	let fits = Arc::new(Bitmap::new(3, 4, vec![0; 12]));
	textures.begin_frame();
	assert!(textures.make_room(&fits));
	assert_eq!(textures.bytes(), 0);
}

#[test]
fn an_empty_bitmap_needs_no_room() {
	let mut textures = Textures::new(0);
	let empty = Arc::new(Bitmap::new(0, 3, Vec::new()));
	textures.begin_frame();
	assert!(textures.make_room(&empty));
	assert_eq!(*textures.insert(&empty, "empty"), "empty");
	assert_eq!(textures.bytes(), 0);
}

#[test]
fn inserting_a_bitmap_again_replaces_its_texture_and_its_bytes() {
	let mut textures = Textures::new(1024);
	let image = bitmap(2, 2);
	textures.begin_frame();
	textures.insert(&image, "old");
	textures.insert(&image, "new");
	assert_eq!(textures.get(&image), Some(&"new"));
	assert_eq!((textures.len(), textures.bytes()), (1, 16));
}

#[test]
fn clearing_forgets_every_texture() {
	let mut textures = Textures::new(1024);
	let [a, b] = [(); 2].map(|()| bitmap(2, 2));
	textures.begin_frame();
	draw(&mut textures, &a, "a");
	draw(&mut textures, &b, "b");
	textures.clear();
	assert_eq!((textures.len(), textures.bytes()), (0, 0));
	assert_eq!(textures.get(&a), None);
	// Room is counted from nothing again.
	textures.begin_frame();
	assert_eq!(draw(&mut textures, &a, "a again"), Some("a again"));
	assert_eq!(textures.bytes(), 16);
}

#[test]
fn removing_a_texture_frees_its_bytes_and_leaves_the_rest() {
	let mut textures = Textures::new(48);
	let [a, b, c] = [(); 3].map(|()| bitmap(2, 2));
	textures.begin_frame();
	draw(&mut textures, &a, "a");
	draw(&mut textures, &b, "b");
	textures.remove(&a);
	assert_eq!(textures.get(&a), None);
	assert_eq!(textures.get(&b), Some(&"b"));
	assert_eq!((textures.len(), textures.bytes()), (1, 16));
	// Removing one it lacks changes nothing.
	textures.remove(&c);
	textures.remove(&a);
	assert_eq!((textures.len(), textures.bytes()), (1, 16));
}

/// The runs of quads that need these textures, if any, as ranges.
fn runs_of(needs: &[Option<char>]) -> Vec<(Range<u32>, Option<char>)> {
	runs(needs.iter().copied())
}

#[test]
fn quads_needing_no_texture_draw_in_one_run_without_one() {
	assert_eq!(runs_of(&[]), []);
	assert_eq!(runs_of(&[None]), [(0..1, None)]);
	assert_eq!(runs_of(&[None, None, None]), [(0..3, None)]);
}

#[test]
fn quads_needing_no_texture_join_the_run_they_are_in() {
	// Text before the first image draws with its texture bound.
	assert_eq!(runs_of(&[None, None, Some('a'), None]), [(0..4, Some('a'))]);
	assert_eq!(runs_of(&[Some('a'), None, Some('a')]), [(0..3, Some('a'))]);
	assert_eq!(
		runs_of(&[None, Some('a'), None, Some('b'), None]),
		[(0..3, Some('a')), (3..5, Some('b'))]
	);
}

#[test]
fn a_run_ends_only_where_another_texture_is_needed() {
	assert_eq!(
		runs_of(&[Some('a'), Some('a'), Some('b'), Some('a')]),
		[(0..2, Some('a')), (2..3, Some('b')), (3..4, Some('a'))]
	);
	// The same texture again later is a new run: drawing order is kept.
	assert_eq!(
		runs_of(&[Some('a'), None, Some('b'), None, None, Some('a')]),
		[(0..2, Some('a')), (2..5, Some('b')), (5..6, Some('a'))]
	);
}
