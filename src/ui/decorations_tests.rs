//! Tests of who draws the window's title bar: the `decorations` key, the
//! choice for each compositor and tab setting, and the buttons shown.
use super::{
	decorations::{DecorationPreference, client_side, shown_buttons},
	window_controls::{ButtonLayout, WindowButton},
};
use crate::config::Preferences;
use DecorationPreference::*;

fn preferences(text: &str) -> Preferences {
	Preferences::parse(text)
}

#[test]
fn the_key_names_each_choice_and_anything_else_is_auto() {
	let parsed = |text: &str| preferences(text).decorations;
	assert_eq!(parsed(""), Auto);
	assert_eq!(parsed("decorations = \"auto\""), Auto);
	assert_eq!(parsed("decorations = \"client\""), Client);
	assert_eq!(parsed("decorations = \"server\""), Server);
	for unknown in ["\"Client\"", "\"none\"", "\"\"", "true", "1"] {
		assert_eq!(
			parsed(&format!("decorations = {unknown}")),
			Auto,
			"{unknown}"
		);
	}
}

#[test]
fn the_key_leaves_every_other_preference_alone() {
	let with = preferences("decorations = \"client\"\nshow_tabs = false\npadding = 4");
	assert!(!with.show_tabs);
	assert_eq!(with.padding, 4);
	assert_eq!(with.renderer, Preferences::default().renderer);
}

#[test]
fn left_to_choose_the_window_draws_its_own_only_where_nobody_else_does() {
	let auto = preferences("");
	// GNOME: no server-side decorations offered.
	assert!(client_side(&auto, Some(false)));
	// sway, KDE: the compositor decorates, or chooses not to.
	assert!(!client_side(&auto, Some(true)));
	// X11, or a probe that failed: the window manager decorates.
	assert!(!client_side(&auto, None));
}

#[test]
fn without_the_header_bar_auto_keeps_the_desktops_frame() {
	// The buttons live in the horizontal header; without it the window
	// would have no way to close or move by pointer.
	for text in ["show_tabs = false", "vertical_tabs = true"] {
		assert!(!client_side(&preferences(text), Some(false)), "{text}");
	}
	let shown = preferences("show_tabs = true\nvertical_tabs = false");
	assert!(client_side(&shown, Some(false)));
}

#[test]
fn an_explicit_choice_wins_everywhere() {
	let client = preferences("decorations = \"client\"\nshow_tabs = false");
	let server = preferences("decorations = \"server\"");
	for offered in [Some(true), Some(false), None] {
		assert!(client_side(&client, offered), "{offered:?}");
		assert!(!client_side(&server, offered), "{offered:?}");
	}
}

#[test]
fn buttons_show_only_when_the_window_draws_its_own_title_bar() {
	let desktop = ButtonLayout::parse("close:maximize");
	assert_eq!(shown_buttons(true, Some(&desktop)), desktop);
	assert_eq!(
		shown_buttons(false, Some(&desktop)),
		ButtonLayout::default()
	);
}

#[test]
fn no_buttons_show_before_the_desktop_has_said_which() {
	// Rather than the fallback flashing before GNOME's single close button.
	assert_eq!(shown_buttons(true, None), ButtonLayout::default());
	assert_eq!(shown_buttons(false, None), ButtonLayout::default());
	let fallback = ButtonLayout::fallback();
	assert_eq!(
		shown_buttons(true, Some(&fallback)).right,
		[
			WindowButton::Minimize,
			WindowButton::Maximize,
			WindowButton::Close
		]
	);
}

#[test]
fn a_desktop_that_wants_no_buttons_gets_none_rather_than_the_fallback() {
	let none = ButtonLayout::parse(":");
	assert_eq!(shown_buttons(true, Some(&none)), ButtonLayout::default());
	assert_ne!(shown_buttons(true, Some(&none)), ButtonLayout::fallback());
}

#[test]
fn saving_the_settings_window_keeps_the_choice() {
	// The settings window does not manage the key; saving it must neither
	// drop nor reset what the user wrote.
	let before = "# mine\ndecorations = \"client\"\nshow_tabs = true\n";
	let saved = crate::config::updated(before, &[("show_tabs", "false".into())]);
	assert!(saved.contains("decorations = \"client\""), "{saved}");
	let after = preferences(&saved);
	assert_eq!(after.decorations, Client);
	assert!(!after.show_tabs);
}

#[test]
fn every_combination_follows_the_table() {
	// (preference, server side offered, show_tabs, vertical_tabs) and
	// whether the window draws its own title bar.
	let offered = [Some(true), Some(false), None];
	for (name, preference) in [("auto", Auto), ("client", Client), ("server", Server)] {
		for server_side in offered {
			for show_tabs in [true, false] {
				for vertical_tabs in [true, false] {
					let text = format!(
						"decorations = \"{name}\"\nshow_tabs = {show_tabs}\nvertical_tabs = {vertical_tabs}"
					);
					let header = show_tabs && !vertical_tabs;
					let want = match preference {
						Client => true,
						Server => false,
						Auto => header && server_side == Some(false),
					};
					assert_eq!(
						client_side(&preferences(&text), server_side),
						want,
						"{text} with {server_side:?}"
					);
				}
			}
		}
	}
}
