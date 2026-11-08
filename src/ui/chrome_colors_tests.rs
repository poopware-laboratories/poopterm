//! Tests for the chrome colors derived from a theme.
use super::chrome_colors::{ChromeColors, chrome_colors, hairline, is_dark, percent};
use crate::{
	render::blend::blend,
	theme::{Theme, ThemeKind},
	ui::contrast::{TEXT, luminance, ratio},
};

/// Every built-in theme, starting from the first.
fn themes() -> Vec<Theme> {
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	kinds.into_iter().map(Theme::from_kind).collect()
}

#[test]
fn percentages_map_onto_blend_weights_rounded_to_the_nearest() {
	assert_eq!(percent(0), 0);
	assert_eq!(percent(100), 255);
	assert_eq!(percent(50), 128);
	assert_eq!(percent(22), 56);
	assert_eq!(percent(5), 13);
	assert_eq!(percent(38), 97);
}

#[test]
fn percentages_past_a_whole_saturate() {
	assert_eq!(percent(140), 255);
}

#[test]
fn a_theme_is_dark_unless_its_text_is_darker_than_its_background() {
	let dark = Theme::from_kind(ThemeKind::MacOsDark);
	let light = Theme::from_kind(ThemeKind::MacOsLight);
	assert!(is_dark(&dark));
	assert!(!is_dark(&light));
	for theme in themes() {
		let expected = luminance(theme.fg) >= luminance(theme.bg);
		assert_eq!(is_dark(&theme), expected, "{:?}", theme.kind);
	}
}

#[test]
fn dark_themes_sink_the_chrome_toward_black() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let colors = chrome_colors(&theme);
	assert_eq!(colors.chrome, blend(theme.bg, 0x000000, percent(22)));
	assert!(luminance(colors.chrome) < luminance(theme.bg));
}

#[test]
fn light_themes_tint_the_chrome_toward_the_text() {
	let theme = Theme::from_kind(ThemeKind::MacOsLight);
	let colors = chrome_colors(&theme);
	assert_eq!(colors.chrome, blend(theme.bg, theme.fg, percent(5)));
	assert!(luminance(colors.chrome) < luminance(theme.bg));
}

#[test]
fn the_active_tab_takes_the_terminals_own_background() {
	for theme in themes() {
		assert_eq!(chrome_colors(&theme).raised, theme.bg, "{:?}", theme.kind);
	}
}

#[test]
fn the_hairline_follows_the_rule_for_each_kind_of_theme() {
	let dark = Theme::from_kind(ThemeKind::Poop);
	let light = Theme::from_kind(ThemeKind::CatppuccinLatte);
	let hairline = |theme: &Theme| chrome_colors(theme).hairline;
	assert_eq!(hairline(&dark), blend(dark.bg, dark.fg, percent(12)));
	assert_eq!(hairline(&light), blend(light.bg, light.fg, percent(13)));
}

#[test]
fn the_hairline_shows_against_both_the_chrome_and_the_terminal() {
	for theme in themes() {
		let colors = chrome_colors(&theme);
		assert!(
			ratio(colors.hairline, colors.chrome) > 1.1,
			"{:?}",
			theme.kind
		);
		assert!(ratio(colors.hairline, theme.bg) > 1.1, "{:?}", theme.kind);
	}
}

#[test]
fn hover_then_pressed_step_from_the_chrome_toward_the_text() {
	for theme in themes() {
		let colors = chrome_colors(&theme);
		assert_eq!(colors.hover, blend(colors.chrome, theme.fg, percent(8)));
		assert_eq!(colors.pressed, blend(colors.chrome, theme.fg, percent(14)));
		let distance = |color| ratio(color, colors.chrome);
		assert!(distance(colors.hover) > 1.0, "{:?}", theme.kind);
		assert!(
			distance(colors.pressed) > distance(colors.hover),
			"{:?}",
			theme.kind
		);
	}
}

#[test]
fn muted_text_starts_from_the_text_faded_into_the_background() {
	let theme = Theme::from_kind(ThemeKind::MacOsDark);
	let muted = chrome_colors(&theme).muted;
	// Darkmode's fade already reads well, so the rule's color stands.
	assert_eq!(muted, blend(theme.fg, theme.bg, percent(38)));
}

#[test]
fn muted_text_meets_wcag_aa_on_every_surface_it_sits_on() {
	for theme in themes() {
		let colors = chrome_colors(&theme);
		for (name, surface) in [
			("chrome", colors.chrome),
			("hover", colors.hover),
			("terminal", theme.bg),
			("popover", colors.surface),
		] {
			let contrast = ratio(colors.muted, surface);
			assert!(
				contrast >= TEXT,
				"muted on {name} in {:?}: {contrast:.2}",
				theme.kind
			);
		}
	}
}

#[test]
fn muted_text_stays_quieter_than_the_text() {
	for theme in themes() {
		let colors = chrome_colors(&theme);
		let (muted, full) = (ratio(colors.muted, theme.bg), ratio(theme.fg, theme.bg));
		assert!(muted <= full, "{:?}", theme.kind);
	}
}

#[test]
fn popovers_lift_toward_the_text_in_dark_themes_and_toward_white_in_light_ones() {
	let dark = Theme::from_kind(ThemeKind::Nord);
	let light = Theme::from_kind(ThemeKind::MacOsLight);
	let surface = |theme: &Theme| chrome_colors(theme).surface;
	assert_eq!(surface(&dark), blend(dark.bg, dark.fg, percent(7)));
	assert_eq!(surface(&light), blend(light.bg, 0xffffff, percent(75)));
}

#[test]
fn the_popover_line_outlines_the_surface() {
	for theme in themes() {
		let colors = chrome_colors(&theme);
		assert!(
			ratio(colors.pop_line, colors.surface) > 1.1,
			"{:?}",
			theme.kind
		);
	}
}

#[test]
fn a_theme_with_equal_text_and_background_still_gets_distinct_shades() {
	let mut theme = Theme::from_kind(ThemeKind::MacOsDark);
	(theme.fg, theme.bg) = (0x808080, 0x808080);
	let colors = chrome_colors(&theme);
	// Nothing to blend toward, so only the dark rule's black moves the chrome.
	assert_ne!(colors.chrome, theme.bg);
	assert_eq!(colors.raised, theme.bg);
	let ChromeColors { hover, chrome, .. } = colors;
	assert_eq!(hover, blend(chrome, theme.fg, percent(8)));
}

#[test]
fn pane_dividers_take_the_hairline_alone() {
	for theme in themes() {
		assert_eq!(
			hairline(&theme),
			chrome_colors(&theme).hairline,
			"{:?}",
			theme.kind
		);
	}
	// Poop's caramel rim was too loud between panes.
	let poop = Theme::from_kind(ThemeKind::Poop);
	assert_ne!(hairline(&poop), poop.border);
	assert!(ratio(hairline(&poop), poop.bg) < ratio(poop.border, poop.bg));
}
