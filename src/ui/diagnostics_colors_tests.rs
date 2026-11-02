//! Tests for the Diagnostics page's colors, on every built-in theme.
use super::{
	contrast::{TEXT, luminance, ratio},
	diagnostics_colors::diagnostics_colors,
};
use crate::theme::{Theme, ThemeKind};

/// Every built-in theme, starting from the first.
fn built_in() -> Vec<Theme> {
	let mut kinds = vec![ThemeKind::MacOsDark];
	while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
		kinds.push(kinds.last().unwrap().next());
	}
	kinds.into_iter().map(Theme::from_kind).collect()
}

/// Whether `color`'s luminance lies between `a`'s and `b`'s.
fn between(color: u32, a: u32, b: u32) -> bool {
	let (a, b) = (luminance(a), luminance(b));
	(a.min(b)..=a.max(b)).contains(&luminance(color))
}

#[test]
fn every_text_color_reads_on_the_page_and_in_the_well() {
	for theme in built_in() {
		let colors = diagnostics_colors(&theme);
		let texts = [
			colors.muted,
			colors.accelerated,
			colors.limited,
			colors.software,
		];
		for (text, background) in texts
			.into_iter()
			.flat_map(|t| [(t, theme.bg), (t, colors.well)])
		{
			assert!(
				ratio(text, background) >= TEXT,
				"{:?}: {text:06x} on {background:06x}",
				theme.kind
			);
		}
	}
}

#[test]
fn muted_text_is_quieter_than_the_theme_text() {
	for theme in built_in() {
		let muted = diagnostics_colors(&theme).muted;
		assert!(
			ratio(muted, theme.bg) < ratio(theme.fg, theme.bg),
			"{:?}: muted {muted:06x} is as loud as {:06x}",
			theme.kind,
			theme.fg
		);
	}
}

#[test]
fn the_three_statuses_have_three_colors() {
	for theme in built_in() {
		let colors = diagnostics_colors(&theme);
		assert_ne!(colors.accelerated, colors.limited, "{:?}", theme.kind);
		assert_ne!(colors.accelerated, colors.software, "{:?}", theme.kind);
		assert_ne!(colors.limited, colors.software, "{:?}", theme.kind);
	}
}

#[test]
fn statuses_take_the_hue_of_green_and_yellow() {
	for theme in built_in() {
		let colors = diagnostics_colors(&theme);
		// Some themes' green is a teal (Rosé Pine's pine): it still has
		// more green than red.
		let (red, green, blue) = (
			|c: u32| c >> 16 & 0xFF,
			|c: u32| c >> 8 & 0xFF,
			|c: u32| c & 0xFF,
		);
		assert!(
			green(colors.accelerated) > red(colors.accelerated),
			"{:?}: accelerated {:06x} is not green",
			theme.kind,
			colors.accelerated
		);
		assert!(
			red(colors.limited) > blue(colors.limited),
			"{:?}: limited {:06x} is not yellow",
			theme.kind,
			colors.limited
		);
	}
}

#[test]
fn software_is_muted_because_its_mark_already_says_it() {
	for theme in built_in() {
		let colors = diagnostics_colors(&theme);
		assert_eq!(colors.software, colors.muted, "{:?}", theme.kind);
	}
}

#[test]
fn the_well_and_the_rules_are_shades_between_the_page_and_its_border() {
	for theme in built_in() {
		let colors = diagnostics_colors(&theme);
		for shade in [colors.well, colors.rule] {
			assert!(between(shade, theme.bg, theme.border), "{:?}", theme.kind);
			assert_ne!(
				shade, theme.border,
				"{:?}: as loud as the border",
				theme.kind
			);
		}
		if theme.bg != theme.border {
			assert_ne!(colors.well, theme.bg, "{:?}: the well vanishes", theme.kind);
			assert_ne!(colors.rule, theme.bg, "{:?}: the rules vanish", theme.kind);
		}
	}
}

#[test]
fn a_faint_palette_moves_toward_the_text_only_as_far_as_it_must() {
	let mut theme = Theme::from_kind(ThemeKind::MacOsLight);
	(theme.bg, theme.fg, theme.border) = (0xFFFFFF, 0x000000, 0xCCCCCC);
	theme.palette[2] = 0x80FF80;
	let colors = diagnostics_colors(&theme);
	assert!(ratio(colors.accelerated, colors.well) >= TEXT);
	assert!(
		ratio(colors.accelerated, colors.well) < TEXT + 0.2,
		"{:06x}",
		colors.accelerated
	);
	theme.palette[2] = 0x006000;
	assert_eq!(
		diagnostics_colors(&theme).accelerated,
		0x006000,
		"a readable green is kept"
	);
}

#[test]
fn the_page_and_the_wells_edge_are_the_themes_own() {
	for theme in built_in() {
		let colors = diagnostics_colors(&theme);
		assert_eq!(
			(colors.page, colors.border),
			(theme.bg, theme.border),
			"{:?}",
			theme.kind
		);
	}
}
