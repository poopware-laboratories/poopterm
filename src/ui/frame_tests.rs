//! Tests of the `window_frame` key and what the frame changes in the layout.
use super::{
	decorations::{client_side, window_buttons},
	frame::{WindowFrame, drags_bar, has_titlebar, traffic_lights_reserve},
	layout::layout,
	scale::chrome,
	window_controls::ButtonLayout,
};
use crate::config::{Preferences, updated};
use WindowFrame::*;

fn preferences(text: &str) -> Preferences {
	Preferences::parse(text)
}

fn square() -> Preferences {
	preferences("window_frame = \"none\"")
}

#[test]
fn the_frame_is_kept_unless_the_key_says_none() {
	let parsed = |text: &str| preferences(text).window_frame;
	assert_eq!(parsed(""), Normal);
	assert_eq!(parsed("window_frame = \"none\""), Frameless);
	assert_eq!(parsed("window_frame = \"normal\""), Normal);
	for unknown in ["\"None\"", "\"square\"", "\"\"", "true", "0"] {
		assert_eq!(
			parsed(&format!("window_frame = {unknown}")),
			Normal,
			"{unknown}"
		);
	}
	assert_eq!(Preferences::default().window_frame, Normal);
}

#[test]
fn names_round_trip_through_the_parser() {
	for frame in [Normal, Frameless] {
		let text = format!("window_frame = \"{}\"", frame.name());
		assert_eq!(preferences(&text).window_frame, frame);
	}
}

#[test]
fn saving_writes_the_key_and_keeps_unknown_ones() {
	let saved = updated(
		"future_key = 7\nwindow_frame = \"normal\"\n",
		&[("window_frame", "\"none\"".to_owned())],
	);
	assert_eq!(saved, "future_key = 7\nwindow_frame = \"none\"\n");
	assert_eq!(preferences(&saved).window_frame, Frameless);
}

#[test]
fn only_a_kept_frame_on_macos_has_a_title_bar() {
	let framed = Normal;
	assert!(has_titlebar(framed, true));
	assert!(!has_titlebar(framed, false));
	assert!(!has_titlebar(Frameless, true));
	assert!(!has_titlebar(Frameless, false));
}

#[test]
fn the_traffic_lights_reserve_goes_with_the_title_bar() {
	assert_eq!(traffic_lights_reserve(Normal, true), 80);
	assert_eq!(traffic_lights_reserve(Normal, false), 0);
	assert_eq!(traffic_lights_reserve(Frameless, true), 0);
}

#[test]
fn a_square_macos_window_puts_the_tab_bar_at_the_very_top() {
	let prefs = Preferences {
		padding: 0,
		window_frame: Frameless,
		..Preferences::default()
	};
	for factor in [1.0, 2.0] {
		let titlebar = has_titlebar(prefs.window_frame, true);
		let layout = layout(&chrome(factor, titlebar), factor as usize, &prefs, 0);
		let tab = chrome(factor, false).tab_height;
		assert_eq!(layout.top, tab, "the grid starts right under the tab bar");
		assert_eq!(layout.padding_top, layout.top);
	}
	let framed = Preferences {
		padding: 0,
		..Preferences::default()
	};
	let with = layout(&chrome(1.0, true), 1, &framed, 0);
	assert_eq!(with.top, 40, "a kept frame still has its title bar");
	assert_eq!(with.top, chrome(1.0, true).titlebar_height);
}

#[test]
fn a_square_window_without_tabs_is_only_the_grid() {
	let prefs = preferences("window_frame = \"none\"\nshow_tabs = false\npadding = 0");
	let layout = layout(
		&chrome(1.0, has_titlebar(prefs.window_frame, true)),
		1,
		&prefs,
		0,
	);
	assert_eq!((layout.top, layout.left, layout.margin), (0, 0, 0));
}

#[test]
fn a_square_window_draws_its_own_edges_and_no_buttons() {
	let desktop = ButtonLayout::fallback();
	for server_side in [None, Some(true), Some(false)] {
		let prefs = square();
		assert!(client_side(&prefs, server_side), "{server_side:?}");
		assert_eq!(
			window_buttons(prefs.window_frame, true, Some(&desktop)),
			ButtonLayout::default()
		);
	}
	let kept = Preferences::default();
	assert_eq!(
		window_buttons(kept.window_frame, true, Some(&desktop)),
		desktop
	);
}

#[test]
fn a_square_window_overrides_a_server_decoration_choice() {
	let prefs = preferences("window_frame = \"none\"\ndecorations = \"server\"");
	assert!(client_side(&prefs, Some(false)));
}

#[test]
fn the_header_drags_the_window_when_nothing_native_does() {
	assert!(!drags_bar(false, Normal), "macOS title bar drags itself");
	assert!(drags_bar(true, Normal));
	assert!(drags_bar(false, Frameless));
	assert!(drags_bar(true, Frameless));
}

#[test]
fn the_tab_bar_is_centred_in_a_title_bar_and_flush_with_the_top_without_one() {
	use super::frame::tab_bar_top;
	assert_eq!(tab_bar_top(44, 40), 2);
	assert_eq!(tab_bar_top(40, 40), 0);
	assert_eq!(tab_bar_top(0, 40), 0, "padding must not push the bar down");
	assert_eq!(tab_bar_top(0, 0), 0);
	assert_eq!(tab_bar_top(30, 40), 0, "a bar taller than the title bar");
}

#[test]
fn padding_sits_below_the_tab_bar_of_a_square_window() {
	let prefs = preferences("window_frame = \"none\"\npadding = 8");
	let chrome = chrome(1.0, has_titlebar(prefs.window_frame, true));
	let layout = layout(&chrome, 1, &prefs, 0);
	assert_eq!(
		super::frame::tab_bar_top(chrome.titlebar_height, chrome.tab_height),
		0
	);
	assert_eq!(
		layout.top,
		chrome.tab_height + 8,
		"the grid keeps its padding"
	);
}

/// Every frame, platform and tab arrangement a window can have.
fn arrangements() -> Vec<(Preferences, bool)> {
	let mut all = Vec::new();
	for frame in ["normal", "none"] {
		for tabs in [
			"show_tabs = true",
			"show_tabs = false",
			"vertical_tabs = true",
		] {
			for padding in [0, 8] {
				let text = format!("window_frame = \"{frame}\"\n{tabs}\npadding = {padding}");
				for macos in [false, true] {
					all.push((preferences(&text), macos));
				}
			}
		}
	}
	all
}

#[test]
fn a_square_window_never_reserves_title_bar_or_traffic_light_room() {
	for (prefs, macos) in arrangements() {
		let titlebar = has_titlebar(prefs.window_frame, macos);
		let chrome = chrome(1.0, titlebar);
		if prefs.window_frame == Frameless {
			assert_eq!(chrome.titlebar_height, 0, "{} {macos}", describe(&prefs));
			assert_eq!(traffic_lights_reserve(prefs.window_frame, macos), 0);
		}
	}
}

#[test]
fn the_grid_never_starts_above_the_tab_bar_and_fills_what_is_left() {
	for scale in [1usize, 2, 3] {
		for (prefs, macos) in arrangements() {
			let chrome = chrome(scale as f64, has_titlebar(prefs.window_frame, macos));
			let sidebar = if prefs.vertical_tabs { 180 } else { 0 };
			let layout = layout(&chrome, scale, &prefs, sidebar);
			// With a title bar the tab strip sits inside it.
			let bar = if prefs.show_tabs && !prefs.vertical_tabs && chrome.titlebar_height == 0 {
				chrome.tab_height
			} else {
				0
			};
			assert!(
				layout.top >= chrome.titlebar_height + bar,
				"{} x{scale}",
				describe(&prefs)
			);
			let area = layout.grid_area((900 * scale, 560 * scale));
			assert!(
				area.y + area.height <= 560 * scale,
				"{} x{scale}",
				describe(&prefs)
			);
		}
	}
}

#[test]
fn removing_the_frame_gives_the_title_bar_height_to_the_terminal_on_macos() {
	for scale in [1usize, 2] {
		let prefs = |text: &str| preferences(&format!("show_tabs = false\npadding = 0\n{text}"));
		let (framed, square) = (prefs(""), prefs("window_frame = \"none\""));
		let top = |prefs: &Preferences| {
			let chrome = chrome(scale as f64, has_titlebar(prefs.window_frame, true));
			layout(&chrome, scale, prefs, 0).top
		};
		assert_eq!(top(&framed), 40 * scale);
		assert_eq!(top(&square), 0);
	}
}

#[test]
fn the_square_window_choice_holds_for_every_decoration_setting_and_compositor() {
	for decorations in ["auto", "client", "server"] {
		for tabs in [true, false] {
			for vertical in [true, false] {
				for server_side in [None, Some(true), Some(false)] {
					let prefs = preferences(&format!(
						"window_frame = \"none\"\ndecorations = \"{decorations}\"\nshow_tabs = {tabs}\nvertical_tabs = {vertical}"
					));
					assert!(
						client_side(&prefs, server_side),
						"{decorations} {tabs} {vertical}"
					);
				}
			}
		}
	}
}

#[test]
fn a_kept_frame_changes_none_of_the_old_decoration_choices() {
	let kept = |text: &str| preferences(&format!("window_frame = \"normal\"\n{text}"));
	assert!(!client_side(&kept(""), None));
	assert!(!client_side(&kept(""), Some(true)));
	assert!(client_side(&kept(""), Some(false)));
	assert!(!client_side(&kept("decorations = \"server\""), Some(false)));
	assert!(client_side(&kept("decorations = \"client\""), Some(true)));
	assert!(
		!client_side(&kept("show_tabs = false"), Some(false)),
		"no header"
	);
}

#[test]
fn buttons_are_hidden_for_every_desktop_layout_in_a_square_window() {
	for text in [
		"",
		"close",
		"appmenu:minimize,maximize,close",
		":close",
		"close:",
	] {
		let desktop = ButtonLayout::parse(text);
		for client in [true, false] {
			assert_eq!(
				window_buttons(Frameless, client, Some(&desktop)),
				ButtonLayout::default(),
				"{text:?} {client}"
			);
			assert_eq!(
				window_buttons(Frameless, client, None),
				ButtonLayout::default()
			);
		}
	}
}

#[test]
fn buttons_of_a_kept_frame_are_those_the_desktop_asked_for_while_we_draw_the_bar() {
	let desktop = ButtonLayout::parse("close:minimize");
	assert_eq!(window_buttons(Normal, true, Some(&desktop)), desktop);
	assert_eq!(
		window_buttons(Normal, false, Some(&desktop)),
		ButtonLayout::default()
	);
	assert_eq!(window_buttons(Normal, true, None), ButtonLayout::default());
}

#[test]
fn the_key_is_found_among_other_keys_and_a_commented_out_one_is_ignored() {
	let text = "theme = \"nord\"\nwindow_frame = \"none\"\npadding = 4\n";
	let parsed = preferences(text);
	assert_eq!(parsed.window_frame, Frameless);
	assert_eq!(parsed.padding, 4, "its neighbours still parse");
	assert_eq!(
		preferences("# window_frame = \"none\"").window_frame,
		Normal
	);
}

#[test]
fn spaces_around_the_equals_sign_do_not_matter() {
	assert_eq!(preferences("window_frame=\"none\"").window_frame, Frameless);
	assert_eq!(
		preferences("  window_frame  =  \"none\"  ").window_frame,
		Frameless
	);
}

#[test]
fn saving_a_missing_key_appends_it_and_saving_twice_changes_nothing() {
	let once = updated("padding = 3\n", &[("window_frame", "\"none\"".to_owned())]);
	assert!(once.contains("padding = 3"));
	assert_eq!(preferences(&once).window_frame, Frameless);
	let twice = updated(&once, &[("window_frame", "\"none\"".to_owned())]);
	assert_eq!(once, twice);
}

#[test]
fn turning_the_frame_back_on_in_the_file_restores_it() {
	let square = updated("", &[("window_frame", "\"none\"".to_owned())]);
	let back = updated(
		&square,
		&[("window_frame", format!("\"{}\"", Normal.name()))],
	);
	assert_eq!(preferences(&back).window_frame, Normal);
	assert_eq!(back.matches("window_frame").count(), 1, "one line, not two");
}

#[test]
fn the_other_preferences_are_untouched_by_the_frame() {
	let (framed, square) = (Preferences::default(), square());
	assert_eq!(square.padding, framed.padding);
	assert_eq!(square.show_tabs, framed.show_tabs);
	assert_eq!(square.vertical_tabs, framed.vertical_tabs);
	assert_eq!(square.font_size, framed.font_size);
	assert_eq!(square.decorations, framed.decorations);
}

fn describe(prefs: &Preferences) -> String {
	format!(
		"{:?} tabs {} vertical {} padding {}",
		prefs.window_frame, prefs.show_tabs, prefs.vertical_tabs, prefs.padding
	)
}
