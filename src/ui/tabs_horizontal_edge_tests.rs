//! Boundary tests for horizontal tab strips.
use super::tabs::{TabStripOptions, tab_layout_with_compaction};

#[allow(clippy::too_many_arguments)] // Keep each layout input explicit in boundary cases.
fn strip(
	width: usize,
	count: usize,
	scale: usize,
	scroll: usize,
	active_tab: usize,
	compact_tabs: bool,
	show_plus: bool,
	show_settings: bool,
) -> super::tabs::TabLayout {
	tab_layout_with_compaction(
		width,
		count,
		scale,
		TabStripOptions {
			left_reserve: 0,
			right_reserve: 0,
			scroll,
			show_plus,
			show_settings,
			active_tab,
			compact_tabs,
		},
	)
}

#[test]
fn zero_horizontal_tabs_have_a_stable_empty_page() {
	let layout = strip(1280, 0, 1, 99, 0, true, true, true);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.visible_count >= 1);
	assert_eq!(layout.tab_width(0), layout.width);
	assert!(layout.width > 0);
}

#[test]
fn one_compact_horizontal_tab_uses_the_regular_single_tab_width() {
	let compact = strip(1280, 1, 1, 0, 0, true, true, true);
	let regular = strip(1280, 1, 1, 0, 0, false, true, true);
	assert_eq!(compact.width, regular.width);
	assert_eq!(compact.compact_width, compact.width);
	assert_eq!(compact.tab_width(0), compact.width);
	assert!(compact.visible_count >= 1);
}

#[test]
fn noncompact_geometry_does_not_depend_on_active_tab_0_versus_1() {
	let first = strip(980, 12, 1, 0, 0, false, true, true);
	let second = strip(980, 12, 1, 0, 1, false, true, true);
	assert_eq!(first.start, second.start);
	assert_eq!(first.width, second.width);
	assert_eq!(first.compact_width, second.compact_width);
	assert_eq!(first.visible_count, second.visible_count);
	assert_eq!(first.right_arrow_x, second.right_arrow_x);
}

#[test]
fn noncompact_geometry_does_not_depend_on_active_tab_0_versus_5() {
	let first = strip(980, 12, 1, 0, 0, false, true, true);
	let second = strip(980, 12, 1, 0, 5, false, true, true);
	assert_eq!(first.start, second.start);
	assert_eq!(first.width, second.width);
	assert_eq!(first.compact_width, second.compact_width);
	assert_eq!(first.visible_count, second.visible_count);
	assert_eq!(first.right_arrow_x, second.right_arrow_x);
}

#[test]
fn noncompact_geometry_does_not_depend_on_active_tab_2_versus_4() {
	let first = strip(980, 12, 1, 0, 2, false, true, true);
	let second = strip(980, 12, 1, 0, 4, false, true, true);
	assert_eq!(first.start, second.start);
	assert_eq!(first.width, second.width);
	assert_eq!(first.compact_width, second.compact_width);
	assert_eq!(first.visible_count, second.visible_count);
	assert_eq!(first.right_arrow_x, second.right_arrow_x);
}

#[test]
fn noncompact_geometry_does_not_depend_on_active_tab_5_versus_0() {
	let first = strip(980, 12, 1, 0, 5, false, true, true);
	let second = strip(980, 12, 1, 0, 0, false, true, true);
	assert_eq!(first.start, second.start);
	assert_eq!(first.width, second.width);
	assert_eq!(first.compact_width, second.compact_width);
	assert_eq!(first.visible_count, second.visible_count);
	assert_eq!(first.right_arrow_x, second.right_arrow_x);
}

#[test]
fn noncompact_geometry_does_not_depend_on_active_tab_3_versus_3() {
	let first = strip(980, 12, 1, 0, 3, false, true, true);
	let second = strip(980, 12, 1, 0, 3, false, true, true);
	assert_eq!(first.start, second.start);
	assert_eq!(first.width, second.width);
	assert_eq!(first.compact_width, second.compact_width);
	assert_eq!(first.visible_count, second.visible_count);
	assert_eq!(first.right_arrow_x, second.right_arrow_x);
}

#[test]
fn active_tab_outside_the_compact_page_does_not_expand_an_invisible_item() {
	let layout = strip(600, 20, 1, 0, 19, true, true, true);
	assert_eq!(layout.first_visible, 0);
	assert!(layout.visible_count < 20);
	assert_eq!(layout.width, layout.compact_width);
	assert_eq!(layout.tab_width(0), layout.compact_width);
	assert_eq!(layout.tab_x(0), layout.start);
}

#[test]
fn excessive_horizontal_scroll_clamps_to_the_last_complete_page() {
	let layout = strip(600, 20, 1, usize::MAX, 19, true, true, true);
	assert_eq!(layout.first_visible, 20 - layout.visible_count);
	assert!(layout.first_visible + layout.visible_count <= 20);
	assert!(layout.visible_count >= 1);
}

#[test]
fn very_narrow_horizontal_width_0_does_not_overflow() {
	let layout = strip(0, 10, 1, 7, 7, true, true, true);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible < 10);
	assert!(layout.first_visible + layout.visible_count <= 10);
}

#[test]
fn very_narrow_horizontal_width_1_does_not_overflow() {
	let layout = strip(1, 10, 1, 7, 7, true, true, true);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible < 10);
	assert!(layout.first_visible + layout.visible_count <= 10);
}

#[test]
fn very_narrow_horizontal_width_20_does_not_overflow() {
	let layout = strip(20, 10, 1, 7, 7, true, true, true);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible < 10);
	assert!(layout.first_visible + layout.visible_count <= 10);
}

#[test]
fn very_narrow_horizontal_width_42_does_not_overflow() {
	let layout = strip(42, 10, 1, 7, 7, true, true, true);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible < 10);
	assert!(layout.first_visible + layout.visible_count <= 10);
}

#[test]
fn very_narrow_horizontal_width_80_does_not_overflow() {
	let layout = strip(80, 10, 1, 7, 7, true, true, true);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible < 10);
	assert!(layout.first_visible + layout.visible_count <= 10);
}

#[test]
fn very_narrow_horizontal_width_120_does_not_overflow() {
	let layout = strip(120, 10, 1, 7, 7, true, true, true);
	assert!(layout.visible_count >= 1);
	assert!(layout.first_visible < 10);
	assert!(layout.first_visible + layout.visible_count <= 10);
}

#[test]
fn zero_horizontal_scale_is_treated_as_one() {
	let zero = strip(1280, 6, 0, 0, 2, true, true, true);
	let one = strip(1280, 6, 1, 0, 2, true, true, true);
	assert_eq!(zero.scale, 1);
	assert_eq!(zero.width, one.width);
	assert_eq!(zero.compact_width, one.compact_width);
	assert_eq!(zero.visible_count, one.visible_count);
}

#[test]
fn horizontal_left_reserve_reduces_the_available_tab_region() {
	let no_reserve = tab_layout_with_compaction(
		1280,
		6,
		1,
		TabStripOptions {
			left_reserve: 0,
			right_reserve: 0,
			scroll: 0,
			show_plus: true,
			show_settings: true,
			active_tab: 2,
			compact_tabs: true,
		},
	);
	let reserve = tab_layout_with_compaction(
		1280,
		6,
		1,
		TabStripOptions {
			left_reserve: 120,
			right_reserve: 0,
			scroll: 0,
			show_plus: true,
			show_settings: true,
			active_tab: 2,
			compact_tabs: true,
		},
	);
	assert!(reserve.start > no_reserve.start);
	assert!(reserve.width <= no_reserve.width);
	assert!(reserve.compact_width < no_reserve.compact_width);
}
