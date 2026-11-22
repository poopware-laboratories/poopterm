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
fn a_sheet_spans_the_width_below_the_status_bar() {
	// 1206×2622 pixels at 3×, 186 pixels of status bar, 66 of home indicator.
	let ((x, y), (width, height)) = sheet_frame((1206, 2622), (186, 66), 3.0, 560.0);
	assert_eq!((x, y, width, height), (0.0, 62.0, 402.0, 560.0));
}

#[test]
fn a_sheet_taller_than_the_room_left_is_cut_to_it() {
	let (_, (_, height)) = sheet_frame((1206, 2622), (186, 66), 3.0, 5000.0);
	assert_eq!(height, 790.0);
	let (_, (_, height)) = sheet_frame((300, 300), (150, 150), 1.0, 100.0);
	assert_eq!(height, 0.0, "no room at all");
}

#[test]
fn a_factor_out_of_reason_counts_as_one() {
	assert_eq!(
		sheet_frame((400, 800), (0, 0), 0.0, 100.0),
		((0.0, 0.0), (400.0, 100.0))
	);
	assert_eq!(
		sheet_frame((400, 800), (0, 0), f64::NAN, 100.0),
		((0.0, 0.0), (400.0, 100.0))
	);
}
