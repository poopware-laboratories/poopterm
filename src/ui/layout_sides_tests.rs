//! The grid between the sides the system keeps (a phone on its side keeps
//! its sensor housing's edge, and an equal one opposite).
use super::*;
use crate::ui::scale::chrome;

fn sided(left: usize, right: usize, sidebar: usize) -> Layout {
	let mut chrome = chrome(3.0, false);
	chrome.sides = (left, right);
	let prefs = Preferences {
		padding: 4,
		..Preferences::default()
	};
	layout(&chrome, 3, &prefs, sidebar)
}

#[test]
fn the_grid_starts_after_the_left_inset_and_ends_before_the_right() {
	let layout = sided(186, 60, 0);
	assert_eq!(layout.left, 12 + 186);
	assert_eq!(layout.right, 12 + 60);
	let area = layout.grid_area((2622, 1206));
	assert_eq!(area.x, 198);
	assert_eq!(area.x + area.width, 2622 - 72);
}

#[test]
fn the_cells_that_fit_leave_both_insets_out() {
	let (bare, sided) = (sided(0, 0, 0), sided(186, 186, 0));
	let cell = (30, 60);
	let (wide, _) = bare.grid_size((2622, 1206), cell);
	let (narrow, _) = sided.grid_size((2622, 1206), cell);
	assert_eq!(wide, (2622 - 24) / 30);
	assert_eq!(narrow, (2622 - 24 - 372) / 30);
}

#[test]
fn a_window_sized_for_a_grid_holds_it_with_its_insets() {
	let layout = sided(186, 90, 0);
	for grid in [(80, 24), (2, 2), (135, 13)] {
		let window = layout.window_size(grid, (30, 60));
		assert_eq!(layout.grid_size(window, (30, 60)), grid, "{grid:?}");
	}
}

#[test]
fn a_vertical_tab_sidebar_sits_after_the_left_inset() {
	let layout = sided(186, 0, 300);
	assert_eq!(layout.left, 12 + 300 + 186);
	assert_eq!(layout.right, 12);
}

#[test]
fn a_desktop_window_has_no_side_insets() {
	let chrome = chrome(1.0, false);
	assert_eq!(chrome.sides, (0, 0));
	let layout = layout(&chrome, 1, &Preferences::default(), 0);
	assert_eq!(layout.left, layout.margin);
	assert_eq!(layout.right, layout.margin);
}

#[test]
fn a_window_with_no_width_or_height_has_nothing_to_lay_out() {
	// iOS reports a window as 0×0 while it moves into its scene, and a
	// minimized desktop window can be: the terminals keep their size.
	assert!(!drawable((0, 0)));
	assert!(!drawable((1398, 0)));
	assert!(!drawable((0, 2034)));
	assert!(drawable((1, 1)), "small, but real");
	assert!(drawable((1398, 2034)));
}
