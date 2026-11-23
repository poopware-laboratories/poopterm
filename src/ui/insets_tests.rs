//! The safe area's insets and the keyboard's, in pixels.
use super::*;

#[test]
fn the_safe_area_leaves_the_top_and_bottom_it_does_not_cover() {
	// An iPhone in portrait: 2622 pixels, the safe area 186 down and 2370 tall.
	assert_eq!(safe_insets(2622, 186, 2370), (186, 66));
	assert_eq!(
		safe_insets(800, 0, 800),
		(0, 0),
		"no insets on a full window"
	);
}

#[test]
fn a_safe_area_reported_out_of_bounds_is_clamped() {
	assert_eq!(safe_insets(100, 150, 10), (100, 0));
	assert_eq!(safe_insets(100, 20, 500), (20, 0));
}

#[test]
fn a_keyboard_covers_the_screen_below_its_top() {
	assert_eq!(covered_points(874.0, 538.0), 336.0);
	assert_eq!(
		covered_points(874.0, 874.0),
		0.0,
		"hidden at the bottom edge"
	);
	assert_eq!(covered_points(874.0, 1210.0), 0.0, "moved off screen");
}

#[test]
fn a_keyboard_frame_out_of_reason_covers_at_most_the_screen() {
	assert_eq!(covered_points(874.0, -100.0), 874.0);
	assert_eq!(covered_points(874.0, f64::NAN), 0.0);
	assert_eq!(covered_points(f64::INFINITY, 0.0), 0.0);
}

#[test]
fn the_bottom_inset_is_whichever_reaches_higher() {
	assert_eq!(
		bottom_inset(66, 0.0, 3.0),
		66,
		"no keyboard: the home indicator's"
	);
	assert_eq!(
		bottom_inset(66, 336.0, 3.0),
		1008,
		"the keyboard's, in pixels"
	);
	assert_eq!(
		bottom_inset(66, 10.0, 3.0),
		66,
		"a sliver of keyboard under the indicator"
	);
	assert_eq!(bottom_inset(0, 100.4, 2.0), 201, "rounded");
}

#[test]
fn a_keyboard_inset_out_of_reason_is_none() {
	assert_eq!(bottom_inset(5, -40.0, 3.0), 5);
	assert_eq!(bottom_inset(5, f64::NAN, 3.0), 5);
	assert_eq!(bottom_inset(5, f64::INFINITY, 3.0), 5);
}

#[test]
fn a_phone_on_its_side_keeps_both_sides_and_the_home_indicator() {
	use winit::dpi::PhysicalSize;
	// 2622×1206 at 3×; the safe area starts 186 in and is 2250 wide, 1143 tall.
	let (outer, inner) = (PhysicalSize::new(2622, 1206), PhysicalSize::new(2250, 1143));
	let insets = window_insets(outer, inner, (186, 0), 0.0, 3.0);
	assert_eq!(insets.sides, (186, 186));
	assert_eq!(insets.safe, (0, 63));
	assert_eq!(insets.home, 63);
}

#[test]
fn upright_a_phone_keeps_its_top_and_bottom_only() {
	use winit::dpi::PhysicalSize;
	let (outer, inner) = (PhysicalSize::new(1206, 2622), PhysicalSize::new(1206, 2370));
	let insets = window_insets(outer, inner, (0, 186), 336.0, 3.0);
	assert_eq!(insets.sides, (0, 0));
	assert_eq!(
		insets.safe,
		(186, 1008),
		"the keyboard reaches higher than the home indicator"
	);
	assert_eq!(insets.home, 66);
}

#[test]
fn a_safe_area_reported_outside_the_window_is_clamped() {
	use winit::dpi::PhysicalSize;
	let (outer, inner) = (PhysicalSize::new(100, 100), PhysicalSize::new(400, 400));
	let insets = window_insets(outer, inner, (-20, 300), 0.0, 1.0);
	assert_eq!(insets.sides, (0, 0));
	assert_eq!(insets.safe, (100, 0));
}
