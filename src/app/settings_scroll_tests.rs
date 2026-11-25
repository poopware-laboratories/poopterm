//! A Settings page on a window shorter than it is laid out for (a phone on
//! its side) scrolls: how far it may, and where taps land once scrolled.
use super::{
	settings_hit::{SettingsPage, SettingsTarget},
	settings_shape::{PAGE_HEIGHT, Shape, clamp_scroll, scrolled},
};

/// An iPhone 17 Pro on its side: 874×402 points at 3×, clear of the island.
const LANDSCAPE: (usize, usize) = (2622, 1206);
const S: usize = 3;
const TOP: Shape = Shape::Top {
	inset: (0, 63),
	sides: (186, 186),
};

#[test]
fn a_short_page_across_the_top_is_drawn_as_tall_as_it_is_laid_out_for() {
	let visible = TOP.page(LANDSCAPE, S).3;
	assert!(visible < PAGE_HEIGHT * S, "the phone on its side is short");
	assert_eq!(TOP.extent(visible, S), PAGE_HEIGHT * S);
	assert_eq!(
		TOP.extent(PAGE_HEIGHT * S + 500, S),
		PAGE_HEIGHT * S + 500,
		"taller is as tall"
	);
}

#[test]
fn beside_the_list_the_page_never_scrolls() {
	assert_eq!(Shape::Side.extent(300, 1), 300);
	assert_eq!(Shape::Side.extent(3000, 2), 3000);
}

#[test]
fn scrolling_stops_at_the_top_and_where_the_end_shows() {
	assert_eq!(clamp_scroll(0, 800, 1800), 0);
	assert_eq!(clamp_scroll(500, 800, 1800), 500);
	assert_eq!(
		clamp_scroll(5000, 800, 1800),
		1000,
		"the end meets the bottom"
	);
	assert_eq!(clamp_scroll(70, 900, 900), 0, "a page that fits stays put");
	assert_eq!(
		clamp_scroll(70, 1000, 900),
		0,
		"and one shorter than the room"
	);
}

#[test]
fn a_drag_or_wheel_moves_the_page_by_its_pixels_inside_its_bounds() {
	assert_eq!(scrolled(0, 120.4, 800, 1800), 120);
	assert_eq!(scrolled(120, -20.0, 800, 1800), 100);
	assert_eq!(scrolled(100, -500.0, 800, 1800), 0, "not above the top");
	assert_eq!(scrolled(900, 500.0, 800, 1800), 1000, "not past the end");
	assert_eq!(
		scrolled(40, f64::NAN, 800, 1800),
		40,
		"nonsense moves nothing"
	);
	assert_eq!(scrolled(40, f64::INFINITY, 800, 1800), 40);
}

/// The tap target at the page's own point `(x, y)`, the page scrolled by
/// `scroll`.
fn tapped_on_page(x: usize, y: usize, scroll: usize) -> Option<SettingsTarget> {
	let (px, py, _, ph) = TOP.page(LANDSCAPE, S);
	let window_y = (py + y).checked_sub(scroll)?;
	if window_y >= py + ph || window_y < py {
		return None;
	}
	TOP.target(
		(px + x, window_y),
		LANDSCAPE,
		S,
		(SettingsPage::Main, scroll),
	)
}

#[test]
fn save_at_the_foot_of_the_page_is_reached_by_scrolling() {
	let (_, _, pw, ph) = TOP.page(LANDSCAPE, S);
	let extent = TOP.extent(ph, S);
	// Save sits in the page's footer, at the right, near the end.
	let save = (pw - 90 * S, extent - 30 * S);
	let end = extent - ph;
	assert_eq!(
		tapped_on_page(save.0, save.1, 0),
		None,
		"below the screen at first"
	);
	assert_eq!(
		tapped_on_page(save.0, save.1, end),
		Some(SettingsTarget::Save),
		"there once scrolled to the end"
	);
}

#[test]
fn a_scroll_past_the_end_lands_taps_as_the_end_does() {
	let (px, py, pw, ph) = TOP.page(LANDSCAPE, S);
	let end = TOP.extent(ph, S) - ph;
	let point = (px + pw - 90 * S, py + ph - 30 * S);
	let page = SettingsPage::Main;
	assert_eq!(
		TOP.target(point, LANDSCAPE, S, (page, end + 10_000)),
		TOP.target(point, LANDSCAPE, S, (page, end)),
	);
}

#[test]
fn the_list_and_done_do_not_scroll_with_the_page() {
	let done = TOP.done(LANDSCAPE.0, S).unwrap();
	let center = (done.0 + done.2 / 2, done.1 + done.3 / 2);
	for scroll in [0, 300, 900] {
		assert_eq!(
			TOP.target(center, LANDSCAPE, S, (SettingsPage::Main, scroll)),
			Some(SettingsTarget::Done)
		);
		let item = TOP.item(1, LANDSCAPE.0, S);
		let at = (item.0 + item.2 / 2, item.1 + item.3 / 2);
		assert!(matches!(
			TOP.target(at, LANDSCAPE, S, (SettingsPage::Main, scroll)),
			Some(SettingsTarget::Open(_))
		));
	}
}
