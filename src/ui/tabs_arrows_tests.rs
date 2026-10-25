//! Tests that the tab strip shows scroll arrows only when tabs overflow it.
use super::tabs::{TabLayout, TabStripOptions, tab_layout_with_compaction};

fn strip(width: usize, count: usize, scale: usize, scroll: usize, compact: bool) -> TabLayout {
	tab_layout_with_compaction(
		width,
		count,
		scale,
		TabStripOptions {
			left_reserve: 0,
			scroll,
			show_plus: true,
			show_settings: true,
			active_tab: 0,
			compact_tabs: compact,
		},
	)
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
