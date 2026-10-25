//! Focused tests for horizontal tab strip geometry.
use super::tabs::{TabLayout, TabStripOptions, tab_layout_with_compaction};

fn compact_strip(
	width: usize,
	count: usize,
	scale: usize,
	scroll: usize,
	active_tab: usize,
) -> TabLayout {
	strip(
		width,
		count,
		scale,
		TabStripOptions {
			left_reserve: 0,
			scroll,
			show_plus: true,
			show_settings: true,
			active_tab,
			compact_tabs: true,
		},
	)
}

fn strip(width: usize, count: usize, scale: usize, options: TabStripOptions) -> TabLayout {
	tab_layout_with_compaction(width, count, scale, options)
}

fn assert_strip_geometry(layout: &TabLayout, count: usize) {
	assert!(layout.first_visible + layout.visible_count <= count);
	for index in layout.first_visible..layout.first_visible + layout.visible_count.saturating_sub(1) {
		assert_eq!(
			layout.tab_x(index + 1),
			layout.tab_x(index) + layout.tab_width(index) + layout.gap,
		);
	}
}

#[test]
fn active_tab_0_expands_and_neighbors_stay_compact() {
	let layout = compact_strip(1280, 6, 1, 0, 0);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.tab_width(0), layout.width);
	assert!(layout.width > layout.compact_width);
	assert!(layout.tab_width(1) < layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn active_tab_1_expands_and_neighbors_stay_compact() {
	let layout = compact_strip(1280, 6, 1, 0, 1);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.tab_width(1), layout.width);
	assert!(layout.width > layout.compact_width);
	assert!(layout.tab_width(2) < layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn active_tab_2_expands_and_neighbors_stay_compact() {
	let layout = compact_strip(1280, 6, 1, 0, 2);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.tab_width(2), layout.width);
	assert!(layout.width > layout.compact_width);
	assert!(layout.tab_width(3) < layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn active_tab_3_expands_and_neighbors_stay_compact() {
	let layout = compact_strip(1280, 6, 1, 0, 3);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.tab_width(3), layout.width);
	assert!(layout.width > layout.compact_width);
	assert!(layout.tab_width(4) < layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn active_tab_4_expands_and_neighbors_stay_compact() {
	let layout = compact_strip(1280, 6, 1, 0, 4);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.tab_width(4), layout.width);
	assert!(layout.width > layout.compact_width);
	assert!(layout.tab_width(5) < layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn active_tab_5_expands_and_neighbors_stay_compact() {
	let layout = compact_strip(1280, 6, 1, 0, 5);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.tab_width(5), layout.width);
	assert!(layout.width > layout.compact_width);
	assert!(layout.tab_width(0) < layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn scaled_compact_strip_1_keeps_all_tabs_ordered() {
	let layout = compact_strip(1280, 6, 1, 0, 1);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.width, 320);
	assert!(layout.compact_width >= 80);
	assert!(layout.width > layout.compact_width);
	assert_eq!(layout.tab_width(1), layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn scaled_compact_strip_2_keeps_all_tabs_ordered() {
	let layout = compact_strip(2560, 6, 2, 0, 2);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.width, 320 * 2);
	assert!(layout.compact_width >= 80 * 2);
	assert!(layout.width > layout.compact_width);
	assert_eq!(layout.tab_width(2), layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn scaled_compact_strip_3_keeps_all_tabs_ordered() {
	let layout = compact_strip(3840, 6, 3, 0, 3);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.width, 320 * 3);
	assert!(layout.compact_width >= 80 * 3);
	assert!(layout.width > layout.compact_width);
	assert_eq!(layout.tab_width(3), layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn scaled_compact_strip_4_keeps_all_tabs_ordered() {
	let layout = compact_strip(5120, 6, 4, 0, 4);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.width, 320 * 4);
	assert!(layout.compact_width >= 80 * 4);
	assert!(layout.width > layout.compact_width);
	assert_eq!(layout.tab_width(4), layout.width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn compact_strip_with_2_tabs_keeps_each_tab_reachable() {
	let layout = compact_strip(2400, 2, 1, 0, 1);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 2);
	assert_eq!(layout.tab_width(1), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 2);
}

#[test]
fn compact_strip_with_3_tabs_keeps_each_tab_reachable() {
	let layout = compact_strip(2400, 3, 1, 0, 2);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 3);
	assert_eq!(layout.tab_width(2), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 3);
}

#[test]
fn compact_strip_with_4_tabs_keeps_each_tab_reachable() {
	let layout = compact_strip(2400, 4, 1, 0, 3);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 4);
	assert_eq!(layout.tab_width(3), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 4);
}

#[test]
fn compact_strip_with_5_tabs_keeps_each_tab_reachable() {
	let layout = compact_strip(2400, 5, 1, 0, 4);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 5);
	assert_eq!(layout.tab_width(4), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 5);
}

#[test]
fn compact_strip_with_6_tabs_keeps_each_tab_reachable() {
	let layout = compact_strip(2400, 6, 1, 0, 5);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 6);
	assert_eq!(layout.tab_width(5), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn compact_strip_with_7_tabs_keeps_each_tab_reachable() {
	let layout = compact_strip(2400, 7, 1, 0, 6);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 7);
	assert_eq!(layout.tab_width(6), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 7);
}

#[test]
fn compact_strip_with_8_tabs_keeps_each_tab_reachable() {
	let layout = compact_strip(2400, 8, 1, 0, 7);
	assert_eq!(layout.first_visible, 0);
	assert_eq!(layout.visible_count, 8);
	assert_eq!(layout.tab_width(7), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 8);
}

#[test]
fn compact_strip_control_mask_0_reserves_only_visible_controls() {
	let layout = strip(
		1280,
		6,
		1,
		TabStripOptions {
			left_reserve: 0,
			scroll: 0,
			show_plus: false,
			show_settings: false,
			active_tab: 2,
			compact_tabs: true,
		},
	);
	assert_eq!(layout.plus_w, 0);
	assert_eq!(layout.settings_w, 0);
	assert_eq!(layout.visible_count, 6);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn compact_strip_control_mask_1_reserves_only_visible_controls() {
	let layout = strip(
		1280,
		6,
		1,
		TabStripOptions {
			left_reserve: 0,
			scroll: 0,
			show_plus: true,
			show_settings: false,
			active_tab: 2,
			compact_tabs: true,
		},
	);
	assert!(layout.plus_w > 0);
	assert_eq!(layout.settings_w, 0);
	assert_eq!(layout.visible_count, 6);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn compact_strip_control_mask_2_reserves_only_visible_controls() {
	let layout = strip(
		1280,
		6,
		1,
		TabStripOptions {
			left_reserve: 0,
			scroll: 0,
			show_plus: false,
			show_settings: true,
			active_tab: 2,
			compact_tabs: true,
		},
	);
	assert_eq!(layout.plus_w, 0);
	assert!(layout.settings_w > 0);
	assert_eq!(layout.visible_count, 6);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn compact_strip_control_mask_3_reserves_only_visible_controls() {
	let layout = strip(
		1280,
		6,
		1,
		TabStripOptions {
			left_reserve: 0,
			scroll: 0,
			show_plus: true,
			show_settings: true,
			active_tab: 2,
			compact_tabs: true,
		},
	);
	assert!(layout.plus_w > 0);
	assert!(layout.settings_w > 0);
	assert_eq!(layout.visible_count, 6);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 6);
}

#[test]
fn compact_strip_scroll_0_keeps_the_selected_page_in_bounds() {
	let layout = compact_strip(600, 20, 1, 0, 0);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert_eq!(layout.tab_width(0), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 20);
}

#[test]
fn compact_strip_scroll_1_keeps_the_selected_page_in_bounds() {
	let layout = compact_strip(600, 20, 1, 1, 1);
	assert_eq!(layout.first_visible, 1);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert_eq!(layout.tab_width(1), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 20);
}

#[test]
fn compact_strip_scroll_5_keeps_the_selected_page_in_bounds() {
	let layout = compact_strip(600, 20, 1, 5, 5);
	assert_eq!(layout.first_visible, 5);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert_eq!(layout.tab_width(5), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 20);
}

#[test]
fn compact_strip_scroll_10_keeps_the_selected_page_in_bounds() {
	let layout = compact_strip(600, 20, 1, 10, 10);
	assert_eq!(layout.first_visible, 10);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert_eq!(layout.tab_width(10), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 20);
}

#[test]
fn compact_strip_scroll_17_keeps_the_selected_page_in_bounds() {
	let layout = compact_strip(600, 20, 1, 17, 17);
	// The last page holds four tabs, so the scroll stops at 16.
	assert_eq!((layout.first_visible, layout.visible_count), (16, 4));
	assert_eq!(layout.tab_width(17), layout.width);
	assert!(layout.width > layout.compact_width);
	assert_strip_geometry(&layout, 20);
}
