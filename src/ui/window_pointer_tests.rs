//! Tests of the pointer over a window that draws its own title bar: what a
//! press hits, the cursor it shows, clicks on the buttons and double-clicks
//! on the bar.
use super::{
	window_controls::{ButtonLayout, WindowButton, place},
	window_pointer::{
		BarPress, DOUBLE_CLICK, PointerTarget, Presses, Route, Surface, pointer_icon, pointer_target,
	},
};
use std::time::{Duration, Instant};
use winit::window::{CursorIcon, ResizeDirection};

const SURFACE: Surface = Surface {
	size: (1100, 700),
	bar: (0, 44),
	scale: 1,
	maximized: false,
};

fn target(point: (f64, f64), surface: Surface) -> Option<PointerTarget> {
	let placement = place(&ButtonLayout::fallback(), surface.size.0, surface.scale);
	pointer_target(&placement, point, surface)
}

#[test]
fn a_press_on_a_button_finds_the_button() {
	// The fallback layout puts close last, 8 pixels from the right edge.
	assert_eq!(
		target((1074.0, 22.0), SURFACE),
		Some(PointerTarget::Button(WindowButton::Close))
	);
	assert_eq!(
		target((1002.0, 22.0), SURFACE),
		Some(PointerTarget::Button(WindowButton::Minimize))
	);
}

#[test]
fn edges_win_over_everything_else() {
	use ResizeDirection::*;
	assert_eq!(
		target((1099.0, 22.0), SURFACE),
		Some(PointerTarget::Resize(East))
	);
	assert_eq!(
		target((500.0, 0.0), SURFACE),
		Some(PointerTarget::Resize(North))
	);
	assert_eq!(
		target((1099.0, 0.0), SURFACE),
		Some(PointerTarget::Resize(NorthEast))
	);
	// Far from both, the middle of the grid is nothing of ours.
	assert_eq!(target((500.0, 300.0), SURFACE), None);
	// So is the empty bar: the header decides what it is.
	assert_eq!(target((500.0, 22.0), SURFACE), None);
}

#[test]
fn a_maximized_window_has_no_edges_but_keeps_its_buttons() {
	let maximized = Surface {
		maximized: true,
		..SURFACE
	};
	assert_eq!(target((1099.0, 22.0), maximized), None);
	assert_eq!(target((500.0, 0.0), maximized), None);
	assert_eq!(
		target((1074.0, 22.0), maximized),
		Some(PointerTarget::Button(WindowButton::Close))
	);
}

#[test]
fn every_length_scales_with_the_window() {
	let big = Surface {
		size: (2200, 1400),
		bar: (0, 88),
		scale: 2,
		maximized: false,
	};
	assert_eq!(
		target((2148.0, 44.0), big),
		Some(PointerTarget::Button(WindowButton::Close))
	);
	assert_eq!(
		target((2190.0, 700.0), big),
		Some(PointerTarget::Resize(ResizeDirection::East))
	);
	// 12 pixels in is an edge at 2x and not at 1x.
	assert_eq!(
		target((2188.0, 700.0), big),
		Some(PointerTarget::Resize(ResizeDirection::East))
	);
	assert_eq!(target((1088.0, 350.0), SURFACE), None);
}

#[test]
fn every_resize_direction_has_its_own_cursor() {
	use CursorIcon as C;
	use ResizeDirection::*;
	let cursors = [
		North, South, East, West, NorthEast, NorthWest, SouthEast, SouthWest,
	]
	.map(|edge| pointer_icon(Some(edge), false, None));
	assert_eq!(
		cursors,
		[
			C::NResize,
			C::SResize,
			C::EResize,
			C::WResize,
			C::NeResize,
			C::NwResize,
			C::SeResize,
			C::SwResize
		]
	);
}

#[test]
fn an_edge_shows_its_cursor_over_links_and_program_shapes() {
	let edge = Some(ResizeDirection::West);
	assert_eq!(pointer_icon(edge, true, Some("text")), CursorIcon::WResize);
	assert_eq!(pointer_icon(None, true, Some("text")), CursorIcon::Pointer);
	assert_eq!(pointer_icon(None, false, Some("text")), CursorIcon::Text);
	assert_eq!(
		pointer_icon(None, false, Some("no such shape")),
		CursorIcon::Default
	);
	assert_eq!(pointer_icon(None, false, None), CursorIcon::Default);
}

#[test]
fn a_button_acts_only_when_released_over_itself() {
	let mut presses = Presses::default();
	presses.press(WindowButton::Close);
	assert_eq!(presses.held(), Some(WindowButton::Close));
	assert_eq!(
		presses.release(Some(WindowButton::Close)),
		Some(WindowButton::Close)
	);
	assert_eq!(presses.held(), None);
	// A second release, with nothing held, does nothing.
	assert_eq!(presses.release(Some(WindowButton::Close)), None);
}

#[test]
fn sliding_off_a_button_before_releasing_cancels_it() {
	let mut presses = Presses::default();
	presses.press(WindowButton::Minimize);
	assert_eq!(presses.release(Some(WindowButton::Close)), None);
	assert_eq!(presses.held(), None);
	presses.press(WindowButton::Maximize);
	assert_eq!(presses.release(None), None);
	assert_eq!(presses.release(None), None);
}

#[test]
fn two_quick_presses_in_one_place_toggle_maximize() {
	let mut presses = Presses::default();
	let start = Instant::now();
	assert_eq!(presses.press_bar(start, (300.0, 20.0), 1), BarPress::Drag);
	let second = start + DOUBLE_CLICK;
	assert_eq!(
		presses.press_bar(second, (304.0, 16.0), 1),
		BarPress::ToggleMaximize
	);
	// A third press starts over rather than toggling back.
	let third = second + Duration::from_millis(50);
	assert_eq!(presses.press_bar(third, (304.0, 16.0), 1), BarPress::Drag);
	assert_eq!(
		presses.press_bar(third, (304.0, 16.0), 1),
		BarPress::ToggleMaximize
	);
}

#[test]
fn slow_or_distant_presses_each_drag() {
	let start = Instant::now();
	let late = start + DOUBLE_CLICK + Duration::from_millis(1);
	let mut presses = Presses::default();
	presses.press_bar(start, (300.0, 20.0), 1);
	assert_eq!(presses.press_bar(late, (300.0, 20.0), 1), BarPress::Drag);
	let mut presses = Presses::default();
	presses.press_bar(start, (300.0, 20.0), 1);
	assert_eq!(presses.press_bar(start, (306.0, 20.0), 1), BarPress::Drag);
	let mut presses = Presses::default();
	presses.press_bar(start, (300.0, 20.0), 1);
	assert_eq!(presses.press_bar(start, (300.0, 26.0), 1), BarPress::Drag);
}

#[test]
fn the_double_click_distance_scales() {
	let start = Instant::now();
	let mut presses = Presses::default();
	presses.press_bar(start, (300.0, 20.0), 2);
	assert_eq!(
		presses.press_bar(start, (310.0, 10.0), 2),
		BarPress::ToggleMaximize
	);
}

#[test]
fn a_button_press_between_bar_presses_breaks_the_double_click() {
	let start = Instant::now();
	let mut presses = Presses::default();
	presses.press_bar(start, (300.0, 20.0), 1);
	presses.press(WindowButton::Close);
	presses.release(None);
	assert_eq!(presses.press_bar(start, (300.0, 20.0), 1), BarPress::Drag);
}

#[test]
fn a_clock_going_backwards_is_not_a_double_click() {
	let start = Instant::now() + Duration::from_secs(1);
	let mut presses = Presses::default();
	presses.press_bar(start, (300.0, 20.0), 1);
	let earlier = start - Duration::from_secs(1);
	assert_eq!(presses.press_bar(earlier, (300.0, 20.0), 1), BarPress::Drag);
}

#[test]
fn a_press_on_an_edge_resizes_from_it() {
	let mut presses = Presses::default();
	let edge = Some(PointerTarget::Resize(ResizeDirection::South));
	assert_eq!(
		presses.route(true, edge),
		Route::Resize(ResizeDirection::South)
	);
	assert_eq!(presses.held(), None);
}

#[test]
fn a_button_is_held_on_press_and_acts_on_release_over_it() {
	let mut presses = Presses::default();
	let close = Some(PointerTarget::Button(WindowButton::Close));
	assert_eq!(presses.route(true, close), Route::Held);
	assert_eq!(presses.held(), Some(WindowButton::Close));
	assert_eq!(presses.route(false, close), Route::Act(WindowButton::Close));
	assert_eq!(presses.held(), None);
}

#[test]
fn a_release_away_from_the_held_button_is_swallowed_without_acting() {
	let mut presses = Presses::default();
	presses.route(true, Some(PointerTarget::Button(WindowButton::Minimize)));
	// Over the grid: the program must not see a release it never saw pressed.
	assert_eq!(presses.route(false, None), Route::Held);
	assert_eq!(presses.held(), None);
	presses.route(true, Some(PointerTarget::Button(WindowButton::Minimize)));
	let edge = Some(PointerTarget::Resize(ResizeDirection::East));
	assert_eq!(presses.route(false, edge), Route::Held);
}

#[test]
fn presses_and_releases_elsewhere_pass_through() {
	let mut presses = Presses::default();
	assert_eq!(presses.route(true, None), Route::Pass);
	assert_eq!(presses.route(false, None), Route::Pass);
	// A release over a button that was never pressed is not a click.
	let close = Some(PointerTarget::Button(WindowButton::Close));
	assert_eq!(presses.route(false, close), Route::Pass);
	// Nor is a release on an edge.
	let edge = Some(PointerTarget::Resize(ResizeDirection::East));
	assert_eq!(presses.route(false, edge), Route::Pass);
}

#[test]
fn a_quick_third_press_starts_a_new_pair_instead_of_toggling_back() {
	// All three presses fall within one double-click time of the first.
	let start = Instant::now();
	let mut presses = Presses::default();
	presses.press_bar(start, (300.0, 20.0), 1);
	let second = start + Duration::from_millis(100);
	assert_eq!(
		presses.press_bar(second, (300.0, 20.0), 1),
		BarPress::ToggleMaximize
	);
	let third = second + Duration::from_millis(100);
	assert_eq!(presses.press_bar(third, (300.0, 20.0), 1), BarPress::Drag);
}
