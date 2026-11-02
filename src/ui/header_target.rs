//! What a press in the horizontal header bar lands on.
use super::tabs::TabLayout;
use crate::render::header::close_rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderTarget {
	Settings,
	NewTab,
	ScrollLeft,
	ScrollRight,
	Tab {
		index: usize,
		close: bool,
	},
	/// Nothing but the bar itself, which drags the window.
	Bar,
}

/// The part of the bar at `bar` (top and height) under `point`, for a strip
/// of `count` tabs laid out as `layout`. Arrows with nothing left to scroll
/// to, and points outside the bar, are `None`.
pub(crate) fn header_target(
	layout: &TabLayout,
	count: usize,
	bar: (usize, usize),
	(x, y): (f64, f64),
) -> Option<HeaderTarget> {
	use HeaderTarget::*;
	let within =
		|at: f64, start: usize, length: usize| at >= start as f64 && at < (start + length) as f64;
	if !within(y, bar.0, bar.1) {
		return None;
	}
	let across = |start, width| within(x, start, width);
	if across(layout.settings_x, layout.settings_w) {
		return Some(Settings);
	}
	if across(layout.plus_x, layout.plus_w) {
		return Some(NewTab);
	}
	if across(layout.left_arrow_x, layout.arrow_w) {
		return (layout.first_visible > 0).then_some(ScrollLeft);
	}
	if across(layout.right_arrow_x, layout.arrow_w) {
		return (layout.first_visible + layout.visible_count < count).then_some(ScrollRight);
	}
	let shown = layout.first_visible..(layout.first_visible + layout.visible_count).min(count);
	for index in shown {
		let (left, width) = (layout.tab_x(index), layout.tab_width(index));
		if across(left, width) {
			let (cx, cy, size, _) = close_rect(left, width, bar, layout.scale);
			let close = count > 1 && within(x, cx, size) && within(y, cy, size);
			return Some(Tab { index, close });
		}
	}
	Some(Bar)
}
