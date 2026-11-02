//! Tests of the window buttons model: reading the desktop's button layout,
//! placing the buttons in the header, hitting them, and the resize edges.
use super::window_controls::{
	ButtonLayout, Placement, WindowButton, WindowButton::*, button_at, place, resize_edge,
};
use winit::window::ResizeDirection as Edge;

fn parsed(text: &str) -> (Vec<WindowButton>, Vec<WindowButton>) {
	let layout = ButtonLayout::parse(text);
	(layout.left, layout.right)
}

#[test]
fn gnome_default_layout_puts_only_close_on_the_right() {
	assert_eq!(parsed("appmenu:close"), (vec![], vec![Close]));
}

#[test]
fn all_three_buttons_on_the_right_keep_their_order() {
	assert_eq!(
		parsed("appmenu:minimize,maximize,close"),
		(vec![], vec![Minimize, Maximize, Close])
	);
}

#[test]
fn buttons_before_the_colon_sit_on_the_left() {
	assert_eq!(
		parsed("close,minimize,maximize:"),
		(vec![Close, Minimize, Maximize], vec![])
	);
	assert_eq!(parsed("close:maximize"), (vec![Close], vec![Maximize]));
}

#[test]
fn a_layout_without_a_colon_is_all_left() {
	assert_eq!(parsed("close,minimize"), (vec![Close, Minimize], vec![]));
}

#[test]
fn unknown_names_and_spaces_are_ignored() {
	assert_eq!(
		parsed(" icon , close : spacer,minimize ,menu "),
		(vec![Close], vec![Minimize])
	);
	// Names are case-sensitive, as GTK reads them.
	assert_eq!(parsed("CLOSE:Minimize"), (vec![], vec![]));
}

#[test]
fn each_button_appears_once_where_it_is_first_named() {
	assert_eq!(
		parsed("close:close,maximize,maximize"),
		(vec![Close], vec![Maximize])
	);
	assert_eq!(
		parsed("maximize:minimize,maximize"),
		(vec![Maximize], vec![Minimize])
	);
}

#[test]
fn text_after_a_second_colon_is_ignored() {
	assert_eq!(
		parsed("close:minimize:maximize"),
		(vec![Close], vec![Minimize])
	);
}

#[test]
fn an_empty_layout_has_no_buttons_and_the_fallback_has_all_three_right() {
	assert_eq!(parsed(""), (vec![], vec![]));
	assert_eq!(parsed(":"), (vec![], vec![]));
	let fallback = ButtonLayout::fallback();
	assert!(fallback.left.is_empty());
	assert_eq!(fallback.right, [Minimize, Maximize, Close]);
}

/// The buttons' x positions, in order.
fn xs(placement: &Placement) -> Vec<(WindowButton, usize)> {
	placement.buttons.clone()
}

#[test]
fn three_buttons_on_the_right_end_eight_pixels_from_the_edge() {
	let placement = place(&ButtonLayout::fallback(), 1100, 1);
	assert_eq!(
		xs(&placement),
		[(Minimize, 984), (Maximize, 1020), (Close, 1056)]
	);
	assert_eq!(placement.width, 36);
	assert_eq!(placement.dividers, [975]);
	assert_eq!((placement.left_reserve, placement.right_reserve), (0, 125));
}

#[test]
fn three_buttons_on_the_left_start_eight_pixels_in() {
	let placement = place(&ButtonLayout::parse("close,minimize,maximize:"), 1100, 1);
	assert_eq!(xs(&placement), [(Close, 8), (Minimize, 44), (Maximize, 80)]);
	assert_eq!(placement.dividers, [124]);
	// Tabs start 8 pixels after the reserve: at 133, past the divider.
	assert_eq!((placement.left_reserve, placement.right_reserve), (125, 0));
}

#[test]
fn buttons_on_both_sides_get_a_divider_each() {
	let placement = place(&ButtonLayout::parse("close:maximize"), 1100, 1);
	assert_eq!(xs(&placement), [(Close, 8), (Maximize, 1056)]);
	assert_eq!(placement.dividers, [52, 1047]);
	assert_eq!((placement.left_reserve, placement.right_reserve), (53, 53));
}

#[test]
fn every_length_doubles_at_scale_two() {
	let placement = place(&ButtonLayout::fallback(), 2200, 2);
	assert_eq!(
		xs(&placement),
		[(Minimize, 1968), (Maximize, 2040), (Close, 2112)]
	);
	assert_eq!(placement.width, 72);
	assert_eq!(placement.dividers, [1950]);
	assert_eq!(placement.right_reserve, 250);
}

#[test]
fn no_buttons_reserve_nothing() {
	let placement = place(&ButtonLayout::parse("appmenu:"), 1100, 1);
	assert!(placement.buttons.is_empty() && placement.dividers.is_empty());
	assert_eq!((placement.left_reserve, placement.right_reserve), (0, 0));
}

#[test]
fn a_window_narrower_than_its_buttons_does_not_underflow() {
	for width in [0, 1, 60, 124] {
		let placement = place(&ButtonLayout::fallback(), width, 1);
		assert_eq!(placement.buttons.len(), 3, "{width}");
		assert!(
			placement.buttons.iter().all(|&(_, x)| x <= width.max(124)),
			"{width}"
		);
		assert_eq!(placement.right_reserve, 125, "{width}");
	}
}

/// A 44 pixel header at the top of an 1100 pixel window.
fn hit(x: f64, y: f64) -> Option<WindowButton> {
	button_at(
		&place(&ButtonLayout::fallback(), 1100, 1),
		(x, y),
		(0, 44),
		1,
	)
}

#[test]
fn a_press_on_a_button_names_it() {
	assert_eq!(hit(1002.0, 22.0), Some(Minimize));
	assert_eq!(hit(1038.0, 22.0), Some(Maximize));
	assert_eq!(hit(1074.0, 22.0), Some(Close));
}

#[test]
fn buttons_are_32_pixels_tall_and_centred_in_the_bar() {
	assert_eq!(hit(1074.0, 5.9), None);
	assert_eq!(hit(1074.0, 6.0), Some(Close));
	assert_eq!(hit(1074.0, 37.9), Some(Close));
	assert_eq!(hit(1074.0, 38.0), None);
}

#[test]
fn the_divider_gap_and_the_margin_are_not_buttons() {
	assert_eq!(hit(983.9, 22.0), None);
	assert_eq!(hit(984.0, 22.0), Some(Minimize));
	assert_eq!(hit(975.0, 22.0), None);
	assert_eq!(hit(1091.9, 22.0), Some(Close));
	assert_eq!(hit(1092.0, 22.0), None);
	assert_eq!(hit(-1.0, -1.0), None);
}

#[test]
fn buttons_follow_the_bar_where_it_sits() {
	let placement = place(&ButtonLayout::fallback(), 1100, 1);
	assert_eq!(button_at(&placement, (1074.0, 15.9), (10, 44), 1), None);
	assert_eq!(
		button_at(&placement, (1074.0, 16.0), (10, 44), 1),
		Some(Close)
	);
	// A taller bar keeps the buttons in its middle.
	assert_eq!(button_at(&placement, (1074.0, 13.9), (0, 60), 1), None);
	assert_eq!(
		button_at(&placement, (1074.0, 14.0), (0, 60), 1),
		Some(Close)
	);
}

fn edge(x: f64, y: f64) -> Option<Edge> {
	resize_edge((x, y), (1100, 640), 1, false)
}

#[test]
fn each_side_resizes_within_six_pixels() {
	assert_eq!(edge(550.0, 0.0), Some(Edge::North));
	assert_eq!(edge(550.0, 5.9), Some(Edge::North));
	assert_eq!(edge(550.0, 6.0), None);
	assert_eq!(edge(550.0, 634.0), Some(Edge::South));
	assert_eq!(edge(550.0, 633.9), None);
	assert_eq!(edge(2.0, 300.0), Some(Edge::West));
	assert_eq!(edge(5.9, 300.0), Some(Edge::West));
	assert_eq!(edge(6.0, 300.0), None);
	assert_eq!(edge(1094.0, 300.0), Some(Edge::East));
	assert_eq!(edge(1093.9, 300.0), None);
}

#[test]
fn corners_take_twelve_pixels_along_each_side() {
	assert_eq!(edge(0.0, 0.0), Some(Edge::NorthWest));
	assert_eq!(edge(11.9, 3.0), Some(Edge::NorthWest));
	assert_eq!(edge(12.0, 3.0), Some(Edge::North));
	assert_eq!(edge(3.0, 11.9), Some(Edge::NorthWest));
	assert_eq!(edge(3.0, 12.0), Some(Edge::West));
	assert_eq!(edge(1099.0, 1.0), Some(Edge::NorthEast));
	assert_eq!(edge(1095.0, 11.9), Some(Edge::NorthEast));
	assert_eq!(edge(1095.0, 12.0), Some(Edge::East));
	assert_eq!(edge(1.0, 639.0), Some(Edge::SouthWest));
	assert_eq!(edge(1088.0, 628.0), Some(Edge::SouthEast));
	assert_eq!(edge(1087.9, 628.0), None);
}

#[test]
fn the_middle_and_points_outside_do_not_resize() {
	assert_eq!(edge(550.0, 300.0), None);
	assert_eq!(edge(-0.1, 300.0), None);
	assert_eq!(edge(1100.0, 300.0), None);
	assert_eq!(edge(550.0, 640.0), None);
}

#[test]
fn zones_double_at_scale_two() {
	let edge = |x, y| resize_edge((x, y), (2200, 1280), 2, false);
	assert_eq!(edge(1100.0, 11.9), Some(Edge::North));
	assert_eq!(edge(1100.0, 12.0), None);
	assert_eq!(edge(23.9, 5.0), Some(Edge::NorthWest));
	assert_eq!(edge(24.0, 5.0), Some(Edge::North));
}

#[test]
fn a_maximized_window_does_not_resize() {
	assert_eq!(resize_edge((0.0, 0.0), (1100, 640), 1, true), None);
	assert_eq!(resize_edge((550.0, 2.0), (1100, 640), 1, true), None);
}

#[test]
fn a_tiny_window_prefers_corners_and_never_panics() {
	assert_eq!(
		resize_edge((5.0, 5.0), (10, 10), 1, false),
		Some(Edge::NorthWest)
	);
	assert_eq!(resize_edge((0.0, 0.0), (0, 0), 1, false), None);
}
