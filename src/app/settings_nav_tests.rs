//! Tests of the Settings sidebar: its items and where they sit, which page
//! a click opens, and how a click anywhere in the window reaches the page
//! beside it.
use super::{
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_nav::{ITEMS, NAV_WIDTH, compose_nav, item_rect, nav_target, window_target},
	settings_pen_tests::{record, texts},
};
use crate::{
	render::scene::Command,
	theme::{Theme, ThemeKind},
	ui::chrome_colors::chrome_colors,
};

const PAGES: [SettingsPage; 7] = [
	SettingsPage::Main,
	SettingsPage::Layout,
	SettingsPage::Fonts,
	SettingsPage::Shortcuts,
	SettingsPage::Panes,
	SettingsPage::Diagnostics,
	SettingsPage::About,
];

#[test]
fn the_sidebar_lists_every_page_once_in_reading_order() {
	let pages: Vec<_> = ITEMS.iter().map(|&(page, _)| page).collect();
	assert_eq!(pages, PAGES);
	let labels: Vec<_> = ITEMS.iter().map(|&(_, label)| label).collect();
	assert_eq!(
		labels,
		[
			"Appearance",
			"Layout",
			"Fonts",
			"Shortcuts",
			"Panes",
			"Diagnostics",
			"About"
		]
	);
}

#[test]
fn items_are_stacked_thirty_six_pixels_apart_inside_the_sidebar() {
	let rects: Vec<_> = (0..ITEMS.len()).map(|index| item_rect(index, 1)).collect();
	assert_eq!(rects[0], (8, 24, 172, 32));
	for pair in rects.windows(2) {
		assert_eq!(pair[1].1 - pair[0].1, 36);
		assert_eq!((pair[0].0, pair[0].2), (pair[1].0, pair[1].2));
	}
	for &(x, _, w, _) in &rects {
		assert!(x + w < NAV_WIDTH, "{x} + {w}");
	}
}

#[test]
fn item_rectangles_double_at_twice_the_scale() {
	for index in 0..ITEMS.len() {
		let (one, two) = (item_rect(index, 1), item_rect(index, 2));
		assert_eq!(two, (one.0 * 2, one.1 * 2, one.2 * 2, one.3 * 2));
	}
}

#[test]
fn clicking_an_item_opens_its_page_at_every_scale() {
	for scale in [1, 2, 3] {
		for (index, &page) in PAGES.iter().enumerate() {
			let (x, y, w, h) = item_rect(index, scale);
			for point in [(x, y), (x + w - 1, y + h - 1), (x + w / 2, y + h / 2)] {
				assert_eq!(nav_target(point, scale), Some(page), "{page:?} at {scale}x");
			}
		}
	}
}

#[test]
fn the_gaps_between_items_and_the_space_around_them_open_nothing() {
	let (x, y, w, h) = item_rect(0, 1);
	assert_eq!(nav_target((x, y + h), 1), None, "the gap under an item");
	assert_eq!(nav_target((x + w, y + 5), 1), None, "right of an item");
	assert_eq!(nav_target((x - 1, y + 5), 1), None, "left of an item");
	assert_eq!(nav_target((x + 5, y - 1), 1), None, "above the first item");
	let (_, ly, _, lh) = item_rect(ITEMS.len() - 1, 1);
	assert_eq!(
		nav_target((x + 5, ly + lh + 40), 1),
		None,
		"below the last item"
	);
}

#[test]
fn a_click_in_the_sidebar_opens_the_page_whatever_page_is_open() {
	for open in PAGES {
		for (index, &page) in PAGES.iter().enumerate() {
			let (x, y, ..) = item_rect(index, 1);
			let target = window_target((x + 4, y + 4), (948, 640), 1, open);
			assert_eq!(
				target,
				Some(SettingsTarget::Open(page)),
				"{open:?} to {page:?}"
			);
		}
	}
}

#[test]
fn the_sidebar_swallows_clicks_that_miss_every_item() {
	for page in PAGES {
		assert_eq!(
			window_target((20, 600), (948, 640), 1, page),
			None,
			"{page:?}"
		);
	}
}

#[test]
fn a_click_beside_the_sidebar_reaches_the_page_as_if_the_sidebar_were_not_there() {
	let size = (948, 640);
	for page in PAGES {
		for (x, y) in [
			(40, 30),
			(300, 160),
			(500, 330),
			(700, 500),
			(770, 604),
			(40, 500),
		] {
			let window = window_target((x + NAV_WIDTH, y), size, 1, page);
			let beside = settings_target((x, y), (size.0 - NAV_WIDTH, size.1), 1, page);
			assert_eq!(window, beside, "{page:?} at ({x}, {y})");
		}
	}
}

#[test]
fn the_same_holds_at_twice_the_scale() {
	let size = (1896, 1280);
	for page in PAGES {
		for (x, y) in [(80, 60), (600, 320), (1500, 1208)] {
			let window = window_target((x + NAV_WIDTH * 2, y), size, 2, page);
			let beside = settings_target((x, y), (size.0 - NAV_WIDTH * 2, size.1), 2, page);
			assert_eq!(window, beside, "{page:?} at ({x}, {y})");
		}
	}
}

#[test]
fn the_save_button_stays_at_the_windows_right_edge() {
	let size = (948, 640);
	let save = window_target((size.0 - 24 - 40, size.1 - 40), size, 1, SettingsPage::Main);
	assert_eq!(save, Some(SettingsTarget::Save));
}

#[test]
fn the_sidebars_edge_is_the_first_pixel_of_the_page() {
	let size = (948, 640);
	let at = |x| window_target((x, 30), size, 1, SettingsPage::Main);
	assert_eq!(at(NAV_WIDTH - 1), None);
	assert_eq!(
		at(NAV_WIDTH),
		settings_target((0, 30), (size.0 - NAV_WIDTH, size.1), 1, SettingsPage::Main)
	);
}

/// The commands of a sidebar drawn at `scale` for `page` with the pointer at
/// `cursor`, on the dark theme.
fn drawn(scale: usize, page: SettingsPage, cursor: (f64, f64)) -> Vec<Command> {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let colors = chrome_colors(&theme);
	record(scale, |pen, p, _| {
		compose_nav(pen, p, (page, cursor), (&colors, theme.fg))
	})
}

fn fills(commands: &[Command]) -> Vec<(usize, usize, usize, usize)> {
	commands
		.iter()
		.filter_map(|command| match command {
			Command::RoundRect(rect, ..) => Some(*rect),
			_ => None,
		})
		.collect()
}

const AWAY: (f64, f64) = (-1.0, -1.0);

#[test]
fn every_page_is_listed_by_name_top_to_bottom() {
	for scale in [1, 2] {
		let labels = texts(&drawn(scale, SettingsPage::Main, AWAY));
		let names: Vec<_> = labels.iter().map(|(text, _)| text.as_str()).collect();
		assert_eq!(
			names,
			[
				"Appearance",
				"Layout",
				"Fonts",
				"Shortcuts",
				"Panes",
				"Diagnostics",
				"About"
			]
		);
		let rows: Vec<_> = labels.iter().map(|(_, at)| at.1).collect();
		assert!(rows.windows(2).all(|pair| pair[0] < pair[1]), "{rows:?}");
	}
}

#[test]
fn the_open_page_alone_is_filled_and_the_others_are_bare() {
	for (index, &page) in PAGES.iter().enumerate() {
		let filled = fills(&drawn(1, page, AWAY));
		assert_eq!(filled, [item_rect(index, 1)], "{page:?}");
	}
}

#[test]
fn the_item_under_the_pointer_is_lit_unless_it_is_the_open_one() {
	let (x, y, ..) = item_rect(2, 1);
	let over_fonts = (x as f64 + 4.0, y as f64 + 4.0);
	let lit = fills(&drawn(1, SettingsPage::Main, over_fonts));
	assert_eq!(lit, [item_rect(0, 1), item_rect(2, 1)]);
	let open = fills(&drawn(1, SettingsPage::Fonts, over_fonts));
	assert_eq!(open, [item_rect(2, 1)], "the open item is not lit twice");
}

#[test]
fn a_pointer_in_the_gap_between_items_lights_none() {
	let (x, y, _, h) = item_rect(1, 1);
	let gap = (x as f64 + 4.0, (y + h) as f64 + 1.0);
	assert_eq!(fills(&drawn(1, SettingsPage::Main, gap)), [item_rect(0, 1)]);
}

#[test]
fn the_open_items_label_is_full_strength_and_the_rest_are_muted() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let muted = chrome_colors(&theme).muted;
	let colors: Vec<_> = drawn(1, SettingsPage::Layout, AWAY)
		.iter()
		.filter_map(|command| match command {
			Command::Text { color, .. } => Some(*color),
			_ => None,
		})
		.collect();
	assert_eq!(colors, [muted, theme.fg, muted, muted, muted, muted, muted]);
}

#[test]
fn each_label_sits_inside_its_item() {
	for scale in [1, 2] {
		let labels = texts(&drawn(scale, SettingsPage::Main, AWAY));
		for (index, (_, at)) in labels.iter().enumerate() {
			let (x, y, w, h) = item_rect(index, scale);
			assert!(
				at.0 >= x && at.0 < x + w && at.1 >= y && at.1 < y + h,
				"{index} at {scale}x: {at:?}"
			);
		}
	}
}

#[test]
fn the_panel_runs_the_height_of_the_window_inside_its_frame_with_an_edge_line() {
	let commands = drawn(1, SettingsPage::Main, AWAY);
	let rects: Vec<_> = commands
		.iter()
		.filter_map(|command| match command {
			Command::Rect(rect, color) => Some((*rect, *color)),
			_ => None,
		})
		.collect();
	assert_eq!(rects.len(), 2);
	let ((px, py, pw, ph), _) = rects[0];
	assert_eq!((px, py, px + pw, ph), (2, 2, NAV_WIDTH, 600 - 4));
	let ((lx, _, lw, _), _) = rects[1];
	assert_eq!(
		(lx, lw),
		(NAV_WIDTH - 1, 1),
		"a one pixel line at the sidebar's right edge"
	);
}

#[test]
fn the_sidebar_colors_follow_the_theme() {
	let light = Theme::from_kind(ThemeKind::MacOsLight);
	let colors = chrome_colors(&light);
	let commands = record(1, |pen, p, _| {
		compose_nav(pen, p, (SettingsPage::Main, AWAY), (&colors, light.fg))
	});
	let panel = commands.iter().find_map(|command| match command {
		Command::Rect(_, color) => Some(*color),
		_ => None,
	});
	assert_eq!(panel, Some(colors.chrome));
}

#[test]
fn every_control_a_page_answers_to_is_reached_through_the_window_at_its_shifted_place() {
	let size = (948, 640);
	for page in PAGES {
		for scale in [1, 2] {
			let (w, h) = (size.0 * scale, size.1 * scale);
			let nav = NAV_WIDTH * scale;
			let mut answered = 0;
			for y in (0..h).step_by(7) {
				for x in (nav..w).step_by(7) {
					let page_local = settings_target((x - nav, y), (w - nav, h), scale, page);
					assert_eq!(window_target((x, y), (w, h), scale, page), page_local);
					answered += usize::from(page_local.is_some());
				}
			}
			assert!(
				answered > 0 || page == SettingsPage::About,
				"{page:?} at {scale}x answers nowhere"
			);
		}
	}
}

#[test]
fn only_the_sidebar_opens_pages_from_the_sidebars_column() {
	for page in PAGES {
		for scale in [1, 2] {
			for y in (0..640 * scale).step_by(3) {
				for x in (0..NAV_WIDTH * scale).step_by(3) {
					let target = window_target((x, y), (948 * scale, 640 * scale), scale, page);
					assert!(
						matches!(target, None | Some(SettingsTarget::Open(_))),
						"{page:?} at ({x}, {y}): {target:?}"
					);
				}
			}
		}
	}
}

#[test]
fn every_label_fits_inside_its_item() {
	for scale in [1, 2] {
		let mut widest = 0;
		let _ = record(scale, |pen, _, _| {
			for &(_, label) in &ITEMS {
				widest = widest.max(pen.width(label, 0.82));
			}
		});
		let (_, _, item, _) = item_rect(0, scale);
		assert!(
			widest + 24 * scale <= item,
			"{widest} in {item} at {scale}x"
		);
	}
}

#[test]
fn the_last_item_ends_well_above_the_bottom_of_the_smallest_window() {
	let (_, y, _, h) = item_rect(ITEMS.len() - 1, 1);
	assert!(y + h + 100 <= 600, "{}", y + h);
}

#[test]
fn the_pages_keep_their_smallest_width_beside_the_sidebar() {
	use super::settings_view::MIN_SIZE;
	assert_eq!(MIN_SIZE.0 + NAV_WIDTH, 868);
	assert!(
		NAV_WIDTH * 4 < MIN_SIZE.0 + NAV_WIDTH,
		"the sidebar is a quarter of the window at most"
	);
}

#[test]
fn every_sidebar_item_can_be_focused_over_its_whole_rectangle() {
	for scale in [1, 2] {
		let commands = drawn(scale, SettingsPage::Main, AWAY);
		let focusable: Vec<_> = commands
			.iter()
			.filter_map(|command| match command {
				Command::Focusable(rect) => Some(*rect),
				_ => None,
			})
			.collect();
		let wanted: Vec<_> = (0..ITEMS.len())
			.map(|index| item_rect(index, scale))
			.collect();
		assert_eq!(focusable, wanted, "x{scale}");
	}
}

#[test]
fn the_center_of_a_focused_item_opens_its_page() {
	use super::settings_focus::center;
	for scale in [1, 2] {
		for (index, &page) in PAGES.iter().enumerate() {
			assert_eq!(
				nav_target(center(item_rect(index, scale)), scale),
				Some(page)
			);
		}
	}
}
