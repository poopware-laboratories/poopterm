//! Tests of shrinking images the GPU cannot hold whole: the texture size
//! chosen (the bitmap's own when it fits, else the size it is shown at,
//! within the device's limit and the room left), the box filter that
//! shrinks it, source rectangles carried into the smaller texture, and
//! shrunk textures made again larger when shown larger.
use super::shrink::{regrown, shrink, source_in, texture_size};
use crate::graphics::{Bitmap, Rect};

const DEVICE: usize = 16384;
const ROOM: usize = 256 << 20;

#[test]
fn a_bitmap_that_fits_keeps_its_size_however_small_it_is_shown() {
	assert_eq!(
		texture_size((100, 50), (10, 5), DEVICE, ROOM),
		Some((100, 50))
	);
	// Exactly the room and the device's width.
	assert_eq!(
		texture_size((DEVICE, 1), (1, 1), DEVICE, DEVICE * 4),
		Some((DEVICE, 1))
	);
}

#[test]
fn a_bitmap_wider_or_taller_than_the_device_makes_is_shrunk_to_its_limit() {
	let shown = (20000, 100);
	assert_eq!(
		texture_size((20000, 100), shown, DEVICE, ROOM),
		Some((DEVICE, 82))
	);
	// The other side is rounded up, never past the limit.
	assert_eq!(
		texture_size((100, 30000), (100, 30000), DEVICE, ROOM),
		Some((55, DEVICE))
	);
}

#[test]
fn a_bitmap_past_the_room_is_shrunk_to_the_size_it_is_shown_at() {
	// A 12-megapixel photo shown as a thumbnail.
	let thumbnail = texture_size((4000, 3000), (400, 300), DEVICE, 1 << 20);
	assert_eq!(thumbnail, Some((400, 300)));
	// Shown squashed more one way: the larger scale keeps every pixel shown.
	let squashed = texture_size((1000, 1000), (100, 500), DEVICE, 1 << 20);
	assert_eq!(squashed, Some((500, 500)));
}

#[test]
fn a_bitmap_shown_larger_than_it_is_never_grows_and_halves_to_fit_the_room() {
	let room = 2000 * 1500 * 4;
	assert_eq!(
		texture_size((4000, 3000), (8000, 6000), DEVICE, room),
		Some((2000, 1500))
	);
	// Odd sides round up as they halve.
	assert_eq!(texture_size((3, 3), (3, 3), DEVICE, 4 * 4), Some((2, 2)));
	assert_eq!(texture_size((3, 3), (3, 3), DEVICE, 4 * 3), Some((1, 1)));
}

#[test]
fn a_bitmap_showing_nothing_or_with_no_room_left_takes_a_texel_or_nothing() {
	assert_eq!(texture_size((4000, 3000), (0, 0), DEVICE, 4), Some((1, 1)));
	assert_eq!(texture_size((4000, 3000), (400, 300), DEVICE, 3), None);
	assert_eq!(texture_size((4000, 3000), (400, 300), DEVICE, 0), None);
}

#[test]
fn sizes_past_what_memory_holds_are_shrunk_without_overflowing() {
	let huge = (usize::MAX / 2, 3);
	let size = texture_size(huge, huge, DEVICE, ROOM).unwrap();
	assert!(size.0 <= DEVICE && size.1 >= 1, "{size:?}");
}

fn bitmap(width: usize, pixels: &[u32]) -> Bitmap {
	Bitmap::new(width, pixels.len() / width, pixels.to_vec())
}

#[test]
fn a_bitmap_of_one_color_shrinks_to_that_color() {
	let one = bitmap(4, &[0xff11_2233; 16]);
	assert_eq!(shrink(&one, (2, 2)), bitmap(2, &[0xff11_2233; 4]));
	assert_eq!(shrink(&one, (1, 3)), bitmap(1, &[0xff11_2233; 3]));
}

#[test]
fn each_texel_averages_the_pixels_it_covers_rounding_to_nearest() {
	let checker = bitmap(2, &[0xff00_0000, 0xffff_ffff, 0xff00_0000, 0xffff_ffff]);
	assert_eq!(shrink(&checker, (1, 1)), bitmap(1, &[0xff80_8080]));
	// Three pixels to two: each texel covers the pixels it overlaps.
	let row = bitmap(3, &[0xff00_0000, 0xff60_6060, 0xffc0_c0c0]);
	let shrunk = shrink(&row, (2, 1));
	assert_eq!(shrunk, bitmap(2, &[0xff30_3030, 0xff90_9090]));
}

#[test]
fn colors_are_weighed_by_their_alpha() {
	// Transparent red beside opaque blue: half-transparent blue, not purple.
	let pair = bitmap(2, &[0x00ff_0000, 0xff00_00ff]);
	assert_eq!(shrink(&pair, (1, 1)), bitmap(1, &[0x8000_00ff]));
	let clear = bitmap(2, &[0x00ff_0000, 0x0000_ff00]);
	assert_eq!(shrink(&clear, (1, 1)), bitmap(1, &[0]));
	assert!(!shrink(&pair, (1, 1)).opaque);
}

#[test]
fn a_source_rectangle_is_carried_into_the_texture_it_was_shrunk_to() {
	let source = Rect {
		x: 10,
		y: 20,
		width: 30,
		height: 40,
	};
	assert_eq!(source_in(source, (100, 100), (100, 100)), source);
	let halved = Rect {
		x: 5,
		y: 10,
		width: 15,
		height: 20,
	};
	assert_eq!(source_in(source, (100, 100), (50, 50)), halved);
}

#[test]
fn a_carried_source_is_never_empty_and_stays_inside_the_texture() {
	let last = Rect {
		x: 99,
		y: 0,
		width: 1,
		height: 100,
	};
	let carried = source_in(last, (100, 100), (10, 3));
	let inside = Rect {
		x: 9,
		y: 0,
		width: 1,
		height: 3,
	};
	assert_eq!(carried, inside);
	let whole = Rect {
		x: 0,
		y: 0,
		width: 100,
		height: 100,
	};
	let texel = Rect {
		x: 0,
		y: 0,
		width: 1,
		height: 1,
	};
	assert_eq!(source_in(whole, (100, 100), (1, 1)), texel);
}

#[test]
fn a_thumbnail_shown_larger_is_made_again_at_the_larger_size() {
	// Its texture's bytes come back to the room when it is made again.
	let room = 400 * 300 * 4 - 100 * 75 * 4;
	let again = regrown((100, 75), (4000, 3000), (400, 300), DEVICE, room);
	assert_eq!(again, Some((400, 300)));
	// With room for the whole bitmap, its own size.
	let whole = regrown((100, 75), (4000, 3000), (400, 300), DEVICE, ROOM);
	assert_eq!(whole, Some((4000, 3000)));
}

#[test]
fn a_texture_shown_no_larger_or_its_bitmaps_own_size_is_kept() {
	for shown in [(100, 75), (50, 30)] {
		assert_eq!(regrown((100, 75), (4000, 3000), shown, DEVICE, 0), None);
	}
	let own = regrown((40, 30), (40, 30), (4000, 3000), DEVICE, ROOM);
	assert_eq!(own, None);
}

#[test]
fn a_texture_its_limits_shrank_is_not_made_again_each_frame() {
	// Halved to fit the budget, which is still all it holds.
	let budget = regrown((200, 150), (4000, 3000), (400, 300), DEVICE, 0);
	assert_eq!(budget, None);
	// As wide as the device makes.
	let device = regrown((DEVICE, 82), (20000, 100), (20000, 100), DEVICE, ROOM);
	assert_eq!(device, None);
}

#[test]
fn a_texture_grows_on_either_side_alone() {
	let taller = regrown((500, 10), (1000, 1000), (100, 500), DEVICE, 500 * 500 * 4);
	assert_eq!(taller, Some((500, 500)));
	let wider = regrown((10, 500), (1000, 1000), (500, 100), DEVICE, 500 * 500 * 4);
	assert_eq!(wider, Some((500, 500)));
}
