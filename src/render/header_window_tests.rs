//! Pixel tests of the window buttons and dividers in the header bar.
use super::{
	header::{CLOSE_HOVER, HeaderButton, Icon},
	header_tests::{COLORS, button, draw, header},
	window_icons::WindowIcon::*,
};

/// The middle of [`button`]'s 36 by 32 rectangle at (300, 10).
const MIDDLE: (usize, usize) = (318, 26);

fn one(button: HeaderButton) -> super::header_tests::Drawn {
	draw(&header(1, Vec::new(), vec![button]))
}

#[test]
fn a_window_icon_sits_in_the_middle_of_its_button_in_the_text_color() {
	let drawn = one(button(Icon::Window(Close), false, true));
	// The cross meets on the two middle pixels.
	assert_eq!(drawn.at(MIDDLE.0, MIDDLE.1), COLORS.fg);
	assert_eq!(drawn.at(MIDDLE.0 - 1, MIDDLE.1 - 1), COLORS.fg);
	// Its 12 pixel square leaves the rest of the button flat.
	assert!(!drawn.any((300, 10, 36, 9), COLORS.fg));
	assert!(!drawn.any((300, 10, 11, 32), COLORS.fg));
	assert!(!drawn.any((325, 10, 11, 32), COLORS.fg));
	assert_eq!(drawn.at(300, 10), COLORS.bar);
}

#[test]
fn a_hovered_close_button_turns_red_with_white_ink() {
	let drawn = one(button(Icon::Window(Close), true, true));
	assert_eq!(drawn.at(302, 26), CLOSE_HOVER);
	assert_eq!(drawn.at(MIDDLE.0, MIDDLE.1), 0xffffff);
	assert_eq!(drawn.at(300, 10), COLORS.bar, "rounded corner");
	assert!(!drawn.any((300, 10, 36, 32), COLORS.hover));
}

#[test]
fn other_window_buttons_light_up_like_the_header_buttons() {
	for icon in [Minimize, Maximize, Restore] {
		let drawn = one(button(Icon::Window(icon), true, true));
		assert_eq!(drawn.at(302, 26), COLORS.hover, "{icon:?}");
		assert!(!drawn.any((300, 10, 36, 32), CLOSE_HOVER), "{icon:?}");
	}
}

#[test]
fn an_inactive_window_draws_its_buttons_dim_and_never_lit() {
	for icon in [Minimize, Close] {
		let drawn = one(button(Icon::Window(icon), true, false));
		// Both icons fully cover the middle pixel.
		assert_eq!(drawn.at(MIDDLE.0, MIDDLE.1), COLORS.dim, "{icon:?}");
		assert!(!drawn.any((300, 10, 36, 32), COLORS.hover), "{icon:?}");
		assert!(!drawn.any((300, 10, 36, 32), CLOSE_HOVER), "{icon:?}");
		assert!(!drawn.any((300, 10, 36, 32), COLORS.fg), "{icon:?}");
	}
}

#[test]
fn maximize_is_hollow_and_restore_draws_differently() {
	let maximize = one(button(Icon::Window(Maximize), false, true));
	assert_eq!(maximize.at(MIDDLE.0, MIDDLE.1), COLORS.bar);
	// Its left side, 2 units in, is inked.
	assert_ne!(maximize.at(314, MIDDLE.1), COLORS.bar);
	let restore = one(button(Icon::Window(Restore), false, true));
	let differ = (20..32).flat_map(|y| (312..324).map(move |x| (x, y)));
	assert!(
		differ
			.into_iter()
			.any(|(x, y)| maximize.at(x, y) != restore.at(x, y))
	);
}

#[test]
fn dividers_are_drawn_in_the_separator_color_and_nothing_else() {
	let mut bar = header(1, Vec::new(), Vec::new());
	bar.dividers = vec![(200, 16, 1, 20)];
	let drawn = draw(&bar);
	for y in 16..36 {
		assert_eq!(drawn.at(200, y), COLORS.separator, "row {y}");
	}
	for (x, y) in [(200, 15), (200, 36), (199, 26), (201, 26)] {
		assert_eq!(drawn.at(x, y), COLORS.bar, "({x}, {y})");
	}
}

#[test]
fn window_icons_double_at_twice_the_scale() {
	let big = HeaderButton {
		rect: (100, 10, 72, 64),
		icon: Icon::Window(Close),
		hovered: false,
		enabled: true,
	};
	let drawn = draw(&header(2, Vec::new(), vec![big]));
	// The 24 pixel icon starts at (124, 30); its arms start 2.5 units in.
	assert_eq!(drawn.at(136, 42), COLORS.fg);
	assert_eq!(drawn.at(129, 35), COLORS.fg);
	assert_eq!(drawn.at(127, 33), COLORS.bar);
	assert_eq!(drawn.at(142, 35), COLORS.fg, "the other arm");
}
