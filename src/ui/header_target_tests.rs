//! Tests of what a press in the horizontal header bar lands on.
use super::{
	header_target::{HeaderTarget, header_target},
	tabs::{TabLayout, TabStripOptions, tab_layout_with_compaction},
};
use crate::render::header::close_rect;
use HeaderTarget::*;

const BAR: (usize, usize) = (0, 44);

fn strip(count: usize, scroll: usize, compact_tabs: bool) -> TabLayout {
	tab_layout_with_compaction(
		1100,
		count,
		1,
		TabStripOptions {
			left_reserve: 0,
			right_reserve: 0,
			scroll,
			show_plus: true,
			show_settings: true,
			active_tab: 0,
			compact_tabs,
		},
	)
}

fn at(layout: &TabLayout, count: usize, x: usize, y: usize) -> Option<HeaderTarget> {
	header_target(layout, count, BAR, (x as f64, y as f64))
}

#[test]
fn the_settings_and_new_tab_buttons_are_found_across_their_width() {
	let layout = strip(2, 0, false);
	assert_eq!((layout.plus_x, layout.settings_x), (1016, 1056));
	for x in [1056, 1074, 1056 + layout.settings_w - 1] {
		assert_eq!(at(&layout, 2, x, 22), Some(Settings), "{x}");
	}
	for x in [1016, 1016 + layout.plus_w - 1] {
		assert_eq!(at(&layout, 2, x, 22), Some(NewTab), "{x}");
	}
	// Between the two buttons is only the bar.
	assert_eq!(at(&layout, 2, 1016 + layout.plus_w, 22), Some(Bar));
}

#[test]
fn a_tab_is_found_by_its_body_and_its_close_button() {
	let layout = strip(3, 0, false);
	for index in 0..3 {
		let (left, width) = (layout.tab_x(index), layout.tab_width(index));
		assert_eq!(
			at(&layout, 3, left + 4, 22),
			Some(Tab {
				index,
				close: false
			})
		);
		let (cx, cy, size, _) = close_rect(left, width, BAR, 1);
		assert_eq!(
			at(&layout, 3, cx + size / 2, cy + size / 2),
			Some(Tab { index, close: true })
		);
	}
}

#[test]
fn the_only_tab_has_no_close_button_to_press() {
	let layout = strip(1, 0, false);
	let (cx, cy, size, _) = close_rect(layout.tab_x(0), layout.tab_width(0), BAR, 1);
	assert_eq!(
		at(&layout, 1, cx + size / 2, cy + size / 2),
		Some(Tab {
			index: 0,
			close: false
		})
	);
}

#[test]
fn the_empty_bar_beside_the_tabs_is_the_bar() {
	let layout = strip(2, 0, false);
	let end = layout.tab_x(1) + layout.tab_width(1);
	assert!(end < layout.plus_x, "two tabs leave room");
	assert_eq!(at(&layout, 2, end, 22), Some(Bar));
	assert_eq!(at(&layout, 2, layout.plus_x - 1, 22), Some(Bar));
	// Left of the first tab, too.
	assert_eq!(at(&layout, 2, layout.start - 1, 22), Some(Bar));
}

#[test]
fn presses_above_or_below_the_bar_are_not_in_it() {
	let layout = strip(2, 0, false);
	let lower = (30, 44);
	let inside = |y: f64| header_target(&layout, 2, lower, (1074.0, y));
	assert_eq!(inside(29.0), None);
	assert_eq!(inside(30.0), Some(Settings));
	assert_eq!(inside(73.9), Some(Settings));
	assert_eq!(inside(74.0), None);
}

#[test]
fn arrows_scroll_only_where_more_tabs_are_hidden() {
	let count = 40;
	let first = strip(count, 0, false);
	assert!(first.arrow_w > 0, "forty tabs overflow");
	let middle = |x: usize| x + first.arrow_w / 2;
	assert_eq!(at(&first, count, middle(first.left_arrow_x), 22), None);
	assert_eq!(
		at(&first, count, middle(first.right_arrow_x), 22),
		Some(ScrollRight)
	);
	let last = strip(count, count, false);
	assert_eq!(last.first_visible + last.visible_count, count);
	assert_eq!(
		at(&last, count, middle(last.left_arrow_x), 22),
		Some(ScrollLeft)
	);
	assert_eq!(at(&last, count, middle(last.right_arrow_x), 22), None);
}

#[test]
fn scrolled_tabs_answer_with_their_own_index() {
	let count = 40;
	let layout = strip(count, 10, false);
	let index = layout.first_visible;
	assert!(index > 0);
	assert_eq!(
		at(&layout, count, layout.tab_x(index) + 2, 22),
		Some(Tab {
			index,
			close: false
		})
	);
}

#[test]
fn compact_tabs_are_found_at_both_of_their_edges_and_gaps_are_bar() {
	let count = 6;
	let layout = strip(count, 0, true);
	assert!(
		layout.compact_width < layout.width,
		"compact tabs are narrower"
	);
	for index in 0..count {
		let (left, width) = (layout.tab_x(index), layout.tab_width(index));
		for x in [left, left + width - 1] {
			let found = at(&layout, count, x, 22);
			assert!(
				matches!(found, Some(Tab { index: i, .. }) if i == index),
				"{index} at {x}: {found:?}"
			);
		}
		if layout.gap > 0 && index + 1 < count {
			assert_eq!(at(&layout, count, left + width, 22), Some(Bar), "{index}");
		}
	}
}

#[test]
fn tabs_past_the_count_are_never_found() {
	// A layout made for more tabs than there are now, as during a close.
	let layout = strip(4, 0, false);
	let gone = layout.tab_x(3) + 2;
	assert_eq!(at(&layout, 3, gone, 22), Some(Bar));
}
