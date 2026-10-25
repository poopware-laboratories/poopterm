//! Tests of the tab strip's edges: scroll arrows only when tabs overflow
//! it, and the buttons and window controls around it.
use super::tabs::{TabLayout, TabStripOptions, tab_layout_with_compaction};

fn options(scroll: usize, compact: bool) -> TabStripOptions {
	TabStripOptions {
		left_reserve: 0,
		scroll,
		show_plus: true,
		show_settings: true,
		active_tab: 0,
		compact_tabs: compact,
	}
}

fn strip(width: usize, count: usize, scale: usize, scroll: usize, compact: bool) -> TabLayout {
	tab_layout_with_compaction(width, count, scale, options(scroll, compact))
}

#[test]
fn tabs_that_fit_start_at_the_edge_without_arrows() {
	for (scale, compact) in [(1, false), (2, false), (1, true)] {
		let layout = strip(1000 * scale, 3, scale, 0, compact);
		// The count is of slots, which may outnumber the tabs.
		assert!(layout.visible_count >= 3);
		assert_eq!(layout.arrow_w, 0, "scale {scale}, compact {compact}");
		assert_eq!(layout.start, 8 * scale);
		assert_eq!(layout.tab_x(0), 8 * scale);
	}
}

#[test]
fn tabs_that_fit_end_before_the_buttons() {
	let layout = strip(1000, 3, 1, 0, false);
	let end = layout.tab_x(2) + layout.tab_width(2);
	assert!(
		end + 8 <= layout.plus_x,
		"tabs end at {end}, + starts at {}",
		layout.plus_x
	);
}

#[test]
fn overflowing_tabs_get_both_arrows_around_them() {
	let layout = strip(1000, 30, 1, 0, false);
	assert!(layout.visible_count < 30);
	assert_eq!(layout.arrow_w, 28);
	assert_eq!(layout.left_arrow_x, 8);
	assert_eq!(layout.start, 8 + 28 + 6);
	let last = layout.first_visible + layout.visible_count - 1;
	assert!(layout.tab_x(last) + layout.tab_width(last) <= layout.right_arrow_x);
	assert!(layout.right_arrow_x + layout.arrow_w <= layout.plus_x);
}

#[test]
fn the_tab_count_that_just_fits_needs_no_arrows_and_one_more_does() {
	// Grow the count until the strip overflows: the count before needed no
	// arrows, and the first that overflows gets them.
	let fits = (1..64)
		.take_while(|&count| strip(1000, count, 1, 0, false).arrow_w == 0)
		.last()
		.unwrap();
	let fitting = strip(1000, fits, 1, 0, false);
	assert!(fitting.visible_count >= fits);
	// Even a full strip stops 8 pixels before the buttons.
	let end = fitting.tab_x(fits - 1) + fitting.tab_width(fits - 1);
	assert!(
		end + 8 <= fitting.plus_x,
		"{end} against {}",
		fitting.plus_x
	);
	let overflowing = strip(1000, fits + 1, 1, 0, false);
	assert!(overflowing.visible_count <= fits);
	assert_eq!(overflowing.arrow_w, 28);
}

#[test]
fn a_scrolled_strip_keeps_its_arrows() {
	let layout = strip(1000, 30, 1, 29, false);
	assert!(layout.first_visible > 0);
	assert_eq!(layout.arrow_w, 28);
}

#[test]
fn the_buttons_sit_at_the_right_edge_four_pixels_apart() {
	for scale in [1, 2] {
		let layout = strip(1000 * scale, 3, scale, 0, false);
		let settings_x = 1000 * scale - 8 * scale - 36 * scale;
		assert_eq!(
			(layout.settings_x, layout.settings_w),
			(settings_x, 36 * scale)
		);
		let plus_x = settings_x - 4 * scale - 36 * scale;
		assert_eq!((layout.plus_x, layout.plus_w), (plus_x, 36 * scale));
	}
}

#[test]
fn a_lone_button_takes_the_edge_without_a_gap() {
	let only_plus = TabStripOptions {
		show_settings: false,
		..options(0, false)
	};
	let layout = tab_layout_with_compaction(1000, 3, 1, only_plus);
	assert_eq!((layout.plus_x, layout.settings_w), (1000 - 8 - 36, 0));
	let only_menu = TabStripOptions {
		show_plus: false,
		..options(0, false)
	};
	let layout = tab_layout_with_compaction(1000, 3, 1, only_menu);
	assert_eq!((layout.settings_x, layout.plus_w), (1000 - 8 - 36, 0));
	let end = layout.tab_x(2) + layout.tab_width(2);
	assert!(end + 8 <= layout.settings_x, "tabs end at {end}");
}

#[test]
fn without_buttons_tabs_may_run_to_the_right_margin() {
	let bare = TabStripOptions {
		show_plus: false,
		show_settings: false,
		..options(0, false)
	};
	let wide = tab_layout_with_compaction(1000, 40, 1, bare);
	let with_buttons = strip(1000, 40, 1, 0, false);
	assert!(wide.visible_count > with_buttons.visible_count);
	let last = wide.first_visible + wide.visible_count - 1;
	assert!(wide.right_arrow_x + wide.arrow_w <= 1000 - 8);
	assert!(wide.tab_x(last) + wide.tab_width(last) <= wide.right_arrow_x);
}

#[test]
fn the_window_buttons_reserve_room_left_of_tabs_and_arrows() {
	let reserved = |count| {
		let options = TabStripOptions {
			left_reserve: 80,
			..options(0, false)
		};
		tab_layout_with_compaction(1000, count, 1, options)
	};
	let few = reserved(3);
	assert_eq!((few.arrow_w, few.start, few.tab_x(0)), (0, 88, 88));
	let many = reserved(30);
	assert_eq!((many.left_arrow_x, many.start), (88, 88 + 28 + 6));
}

#[test]
fn compact_tabs_that_overflow_get_arrows_too() {
	let layout = strip(600, 30, 1, 0, true);
	assert_eq!(layout.arrow_w, 28);
	assert!(layout.visible_count < 30);
	assert_eq!(layout.start, 8 + 28 + 6);
	let last = layout.first_visible + layout.visible_count - 1;
	assert!(layout.tab_x(last) + layout.tab_width(last) <= layout.right_arrow_x);
}

#[test]
fn arrows_take_room_from_the_tabs_they_scroll() {
	// The count that just overflows shows fewer tabs than fit without arrows.
	let fits = (1..64)
		.take_while(|&count| strip(1000, count, 1, 0, false).arrow_w == 0)
		.last()
		.unwrap();
	let overflowing = strip(1000, fits + 1, 1, 0, false);
	let room = overflowing.right_arrow_x - 8 - overflowing.start;
	let used = overflowing.visible_count * (overflowing.width + overflowing.gap) - overflowing.gap;
	assert!(used <= room, "{used} in {room}");
	assert_eq!(overflowing.right_arrow_x, 1000 - 8 - 36 - 4 - 36 - 8 - 28);
}
