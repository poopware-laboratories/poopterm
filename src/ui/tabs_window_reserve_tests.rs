//! The horizontal tab strip beside the window buttons: the right reserve
//! moves the + and settings buttons and narrows the tabs, and the strip
//! never reaches the buttons or their dividers.
use super::{
	tabs::{TabLayout, TabStripOptions, tab_layout_with_compaction},
	window_controls::{ButtonLayout, place},
};

fn strip(width: usize, count: usize, scale: usize, reserves: (usize, usize)) -> TabLayout {
	strip_with(width, count, scale, reserves, (true, true))
}

fn strip_with(
	width: usize,
	count: usize,
	scale: usize,
	(left_reserve, right_reserve): (usize, usize),
	(show_plus, show_settings): (bool, bool),
) -> TabLayout {
	tab_layout_with_compaction(
		width,
		count,
		scale,
		TabStripOptions {
			left_reserve,
			right_reserve,
			scroll: 0,
			show_plus,
			show_settings,
			active_tab: 0,
			compact_tabs: false,
		},
	)
}

#[test]
fn a_right_reserve_moves_settings_and_plus_left_by_exactly_its_width() {
	let bare = strip(1100, 2, 1, (0, 0));
	assert_eq!((bare.plus_x, bare.settings_x), (1016, 1056));
	// Three window buttons reserve 125 pixels: settings ends 8 pixels
	// before their divider at 975.
	let beside = strip(1100, 2, 1, (0, 125));
	assert_eq!((beside.plus_x, beside.settings_x), (891, 931));
	assert_eq!(beside.settings_x + beside.settings_w, 967);
	assert_eq!(
		(beside.start, beside.left_arrow_x),
		(bare.start, bare.left_arrow_x)
	);
}

#[test]
fn every_length_doubles_at_twice_the_scale() {
	let beside = strip(2200, 2, 2, (0, 250));
	assert_eq!((beside.plus_x, beside.settings_x), (1782, 1862));
	assert_eq!(beside.settings_x + beside.settings_w, 1934);
}

#[test]
fn without_plus_or_settings_tabs_stop_eight_pixels_before_the_reserve() {
	let many = 40;
	let bare = strip_with(1100, many, 1, (0, 125), (false, false));
	assert!(bare.arrow_w > 0, "forty tabs overflow");
	// The right arrow ends 8 pixels before where the controls would start.
	assert_eq!(bare.right_arrow_x + bare.arrow_w, 1100 - 125 - 8 - 8);
}

#[test]
fn a_right_reserve_leaves_less_room_for_tabs() {
	let count = 6;
	let bare = strip(1100, count, 1, (0, 0));
	let beside = strip(1100, count, 1, (0, 125));
	let room = |layout: &TabLayout| {
		let last = layout.first_visible + layout.visible_count - 1;
		layout.tab_x(last) + layout.tab_width(last)
	};
	assert!(room(&beside) < room(&bare));
	assert!(room(&beside) <= beside.plus_x - 8);
}

#[test]
fn a_reserve_wider_than_the_window_does_not_underflow() {
	for width in [0, 60, 124, 168] {
		let layout = strip(width, 3, 1, (0, 125));
		assert_eq!((layout.plus_x, layout.settings_x), (0, 0), "{width}");
	}
}

#[test]
fn tabs_and_controls_stay_clear_of_the_window_buttons_in_every_layout() {
	let layouts = [
		"appmenu:minimize,maximize,close",
		"close,minimize,maximize:",
		"close:maximize",
		"appmenu:close",
		"appmenu:",
	];
	for scale in 1..=3 {
		let width = 1100 * scale;
		for text in layouts {
			let placement = place(&ButtonLayout::parse(text), width, scale);
			let reserves = (placement.left_reserve, placement.right_reserve);
			// The edges of the room the strip may use.
			let start = placement.left_reserve + 8 * scale;
			let end = width - placement.right_reserve - 8 * scale;
			for count in [1, 3, 40] {
				let layout = strip(width, count, scale, reserves);
				let what = format!("{text} at {scale}, {count} tabs");
				assert!(layout.left_arrow_x >= start, "{what}");
				assert!(layout.start >= start, "{what}");
				assert_eq!(layout.settings_x + layout.settings_w, end, "{what}");
				let last = layout.first_visible + layout.visible_count - 1;
				assert!(
					layout.tab_x(last) + layout.tab_width(last) <= layout.plus_x,
					"{what}"
				);
				if layout.arrow_w > 0 {
					assert!(
						layout.right_arrow_x + layout.arrow_w <= layout.plus_x,
						"{what}"
					);
				}
			}
		}
	}
}
