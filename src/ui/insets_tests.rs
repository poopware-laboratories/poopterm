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
	let insets = window_insets(outer, inner, ((186, 0), (0, 0)), f64::INFINITY, 3.0);
	assert_eq!(insets.sides, (186, 186));
	assert_eq!(insets.safe, (0, 63));
	assert_eq!(insets.home, 63);
}

#[test]
fn upright_a_phone_keeps_its_top_and_bottom_and_the_keyboards_height() {
	use winit::dpi::PhysicalSize;
	let (outer, inner) = (PhysicalSize::new(1206, 2622), PhysicalSize::new(1206, 2370));
	// The keyboard's top is 538 points down a screen 874 points tall.
	let insets = window_insets(outer, inner, ((0, 186), (0, 0)), 538.0, 3.0);
	assert_eq!(insets.sides, (0, 0));
	assert_eq!(insets.safe, (186, 1008), "336 points of keyboard");
	assert_eq!(insets.home, 66);
}

#[test]
fn an_ipad_window_measures_its_insets_from_its_own_corner() {
	use winit::dpi::PhysicalSize;
	// A window 1100×1400 pixels at 2×, 716 in and 114 down the screen, its
	// safe area starting 60 below its top (the window controls).
	let (outer, inner) = (PhysicalSize::new(1100, 1400), PhysicalSize::new(1100, 1340));
	let at = ((716, 174), (716, 114));
	let hidden = window_insets(outer, inner, at, f64::INFINITY, 2.0);
	assert_eq!(
		(hidden.safe, hidden.sides, hidden.home),
		((60, 0), (0, 0), 0)
	);
	// The keyboard's top is 447 points down; the window ends at 757.
	let typing = window_insets(outer, inner, at, 447.0, 2.0);
	assert_eq!(typing.safe, (60, 620), "only what covers the window");
}

#[test]
fn a_keyboard_below_the_window_covers_none_of_it() {
	use winit::dpi::PhysicalSize;
	let (outer, inner) = (PhysicalSize::new(800, 600), PhysicalSize::new(800, 600));
	let insets = window_insets(outer, inner, ((100, 50), (100, 50)), 400.0, 2.0);
	assert_eq!(
		insets.safe,
		(0, 0),
		"the window ends at 325 points, above the keyboard"
	);
	let all = window_insets(outer, inner, ((100, 50), (100, 50)), -10.0, 2.0);
	assert_eq!(all.safe, (0, 600), "never more than the window");
}

#[test]
fn a_safe_area_reported_outside_the_window_is_clamped() {
	use winit::dpi::PhysicalSize;
	let (outer, inner) = (PhysicalSize::new(100, 100), PhysicalSize::new(400, 400));
	let insets = window_insets(outer, inner, ((-20, 300), (0, 0)), f64::NAN, 1.0);
	assert_eq!(insets.sides, (0, 0));
	assert_eq!(insets.safe, (100, 0));
}

#[test]
fn a_phone_turned_the_other_way_keeps_its_housing_on_the_right() {
	use winit::dpi::PhysicalSize;
	let (outer, inner) = (PhysicalSize::new(2622, 1206), PhysicalSize::new(2250, 1143));
	// Turned right, the safe area starts at the left edge and stops 186
	// short of the right.
	let insets = window_insets(outer, inner, ((186, 0), (0, 0)), f64::INFINITY, 3.0);
	let other = window_insets(
		outer,
		PhysicalSize::new(2436, 1143),
		((0, 0), (0, 0)),
		f64::INFINITY,
		3.0,
	);
	assert_eq!(insets.sides, (186, 186));
	assert_eq!(other.sides, (0, 186));
}

#[test]
fn a_window_reaching_past_the_screens_bottom_counts_the_keyboard_from_its_own_end() {
	use winit::dpi::PhysicalSize;
	// 1000 pixels tall at 2×, starting 300 points down: it ends at 800.
	let (outer, inner) = (PhysicalSize::new(800, 1000), PhysicalSize::new(800, 1000));
	let at = ((40, 600), (40, 600));
	assert_eq!(window_insets(outer, inner, at, 700.0, 2.0).safe, (0, 200));
	assert_eq!(
		window_insets(outer, inner, at, 800.0, 2.0).safe,
		(0, 0),
		"just below"
	);
	assert_eq!(
		window_insets(outer, inner, at, 300.0, 2.0).safe,
		(0, 1000),
		"all of it"
	);
}

#[test]
fn a_floating_keyboard_above_the_windows_end_still_reserves_from_its_top_down() {
	use winit::dpi::PhysicalSize;
	// The iPad's small floating keyboard reports its own frame; the
	// terminal keeps clear of everything below its top, the simple choice
	// that never hides the line being typed.
	let (outer, inner) = (PhysicalSize::new(1668, 2420), PhysicalSize::new(1668, 2380));
	let insets = window_insets(outer, inner, ((0, 40), (0, 0)), 900.0, 2.0);
	assert_eq!(insets.safe, (40, 620));
}

#[test]
fn a_scale_out_of_reason_counts_as_one() {
	use winit::dpi::PhysicalSize;
	let (outer, inner) = (PhysicalSize::new(400, 800), PhysicalSize::new(400, 760));
	for factor in [0.0, -2.0, f64::NAN, f64::INFINITY] {
		let insets = window_insets(outer, inner, ((0, 20), (0, 0)), 700.0, factor);
		assert_eq!(insets.safe, (20, 100), "{factor}");
	}
}

#[test]
fn the_window_controls_reserve_what_the_corner_adapted_inset_adds() {
	// An iPad window at 2×: the plain safe area starts at the left edge,
	// adapted around its corner it starts after the controls, 76 points in.
	assert_eq!(corner_reserve(0.0, 76.0, 2.0), 152);
	assert_eq!(
		corner_reserve(62.0, 62.0, 3.0),
		0,
		"a phone's housing is no corner"
	);
	assert_eq!(corner_reserve(62.0, 100.0, 3.0), 114);
}

#[test]
fn a_corner_inset_out_of_reason_reserves_nothing() {
	assert_eq!(
		corner_reserve(10.0, 4.0, 2.0),
		0,
		"smaller than the plain one"
	);
	assert_eq!(corner_reserve(0.0, f64::NAN, 2.0), 0);
	assert_eq!(corner_reserve(0.0, f64::INFINITY, 2.0), 0);
	assert_eq!(
		corner_reserve(0.0, 50.0, f64::NAN),
		50,
		"a factor out of reason counts as one"
	);
}
