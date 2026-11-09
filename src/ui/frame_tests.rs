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
