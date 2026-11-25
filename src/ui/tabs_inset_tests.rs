//! The vertical sidebar clear of what iOS keeps: the status bar, the home
//! indicator and key bar, and a phone on its side's sensor housing.
use super::*;

fn options() -> VerticalTabOptions {
	VerticalTabOptions {
		top_reserve: 0,
		status_height: 26,
		scroll: 0,
		active_tab: 0,
		compact_tabs: false,
		show_plus: true,
		show_settings: true,
	}
}

#[test]
fn no_insets_is_the_plain_layout() {
	let plain = vertical_tab_layout(800, 3, 1, options());
	let inset = inset_vertical_tab_layout(800, 3, 1, options(), SidebarInsets::default());
	let fields = |l: &VerticalTabLayout| {
		(
			l.sidebar_width,
			l.left,
			l.tab_start_y,
			l.plus_x,
			l.plus_y,
			l.settings_x,
			l.visible_count,
		)
	};
	assert_eq!(fields(&inset), fields(&plain));
	assert_eq!(inset.left, 0);
}

#[test]
fn tabs_start_below_the_status_bar() {
	let plain = vertical_tab_layout(800, 3, 2, options());
	let insets = SidebarInsets {
		top: 118,
		..SidebarInsets::default()
	};
	let inset = inset_vertical_tab_layout(800, 3, 2, options(), insets);
	assert_eq!(inset.tab_start_y, plain.tab_start_y + 118);
	assert_eq!(inset.row_y(0), plain.row_y(0) + 118);
}

#[test]
fn buttons_sit_above_the_home_indicator_and_key_bar() {
	let plain = vertical_tab_layout(800, 3, 2, options());
	let insets = SidebarInsets {
		bottom: 150,
		..SidebarInsets::default()
	};
	let inset = inset_vertical_tab_layout(800, 3, 2, options(), insets);
	assert_eq!(inset.plus_y, plain.plus_y.map(|y| y - 150));
	assert_eq!(inset.settings_y, plain.settings_y.map(|y| y - 150));
	assert!(
		inset.visible_count <= plain.visible_count,
		"less room for tabs"
	);
}

#[test]
fn on_its_side_the_content_moves_right_and_the_background_reaches_the_edge() {
	let plain = vertical_tab_layout(400, 3, 3, options());
	let insets = SidebarInsets {
		left: 177,
		..SidebarInsets::default()
	};
	let inset = inset_vertical_tab_layout(400, 3, 3, options(), insets);
	assert_eq!(inset.left, 177);
	assert_eq!(inset.sidebar_width, plain.sidebar_width + 177);
	assert_eq!(inset.tab_width, plain.tab_width, "tabs keep their size");
	assert_eq!(inset.plus_x, plain.plus_x + 177);
	assert_eq!(inset.settings_x, plain.settings_x + 177);
	let (x, ..) = crate::ui::sidebar::row_rect(&inset, 0);
	let (plain_x, ..) = crate::ui::sidebar::row_rect(&plain, 0);
	assert_eq!(x, plain_x + 177, "the tab rows too");
}

#[test]
fn a_window_too_narrow_for_the_sidebar_gets_the_rail() {
	// A phone upright: 402 points at 3x.
	assert!(sidebar_is_rail(false, 1206, 3));
	// On its side, 874 points: the full sidebar fits in a third... nearly.
	assert!(!sidebar_is_rail(false, 2622, 3));
	assert!(sidebar_is_rail(true, 2622, 3), "asked for");
	assert!(
		!sidebar_is_rail(false, 3 * sidebar_width(1), 1),
		"exactly a third fits"
	);
	assert!(sidebar_is_rail(false, 3 * sidebar_width(1) - 1, 1));
}
