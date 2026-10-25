//! Contrast between colors (WCAG 2.2), and the chrome colors derived from a
//! theme so that dimmed and accented text stays readable in every theme.

use crate::{render::blend::blend, theme::Theme};

/// WCAG AA's minimum contrast for normal text.
pub(crate) const TEXT: f64 = 4.5;
/// How far toward the foreground dimmed text starts, out of 255.
pub(crate) const DIM: u32 = 150;

/// WCAG relative luminance of an sRGB color.
fn luminance(color: u32) -> f64 {
	let channel = |shift: u32| {
		let srgb = f64::from((color >> shift) & 0xFF) / 255.0;
		if srgb <= 0.04045 {
			srgb / 12.92
		} else {
			((srgb + 0.055) / 1.055).powf(2.4)
		}
	};
	0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// The contrast ratio between two colors, from 1 to 21.
pub(crate) fn ratio(a: u32, b: u32) -> f64 {
	let (a, b) = (luminance(a), luminance(b));
	(a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `color`, moved toward `target` only as far as it takes to reach `min`
/// against every one of `backgrounds`; `target` when nothing closer does.
pub(crate) fn readable(color: u32, target: u32, backgrounds: &[u32], min: f64) -> u32 {
	(0..=255)
		.map(|a| blend(color, target, a))
		.find(|&c| backgrounds.iter().all(|&bg| ratio(c, bg) >= min))
		.unwrap_or(target)
}

/// What tabs are drawn on: the window, the vertical tab sidebar, and the
/// hovered and active tab shades of both tab strips (`draw_tabs`,
/// `draw_vertical_tabs`).
pub(crate) fn tab_backgrounds(theme: &Theme) -> [u32; 6] {
	[0, 24, 38, 40, 70, 74].map(|shade| blend(theme.bg, theme.border, shade))
}

/// Tab text colors besides the title: dimmed (status text, the activity
/// dot) and accented (the bell and working markers).
pub(crate) fn tab_text_colors(theme: &Theme) -> (u32, u32) {
	let backgrounds = tab_backgrounds(theme);
	let readable = |color| readable(color, theme.fg, &backgrounds, TEXT);
	(
		readable(blend(theme.bg, theme.fg, DIM)),
		readable(theme.palette[4]),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::theme::ThemeKind;

	/// Every built-in theme, starting from the first.
	fn built_in() -> Vec<Theme> {
		let mut kinds = vec![ThemeKind::MacOsDark];
		while kinds.last().unwrap().next() != ThemeKind::MacOsDark {
			kinds.push(kinds.last().unwrap().next());
		}
		kinds.into_iter().map(Theme::from_kind).collect()
	}

	#[test]
	fn ratio_follows_wcag_relative_luminance() {
		assert!((ratio(0x000000, 0xFFFFFF) - 21.0).abs() < 1e-9);
		assert!(
			(ratio(0xFFFFFF, 0x000000) - 21.0).abs() < 1e-9,
			"either order"
		);
		assert!((ratio(0x777777, 0x777777) - 1.0).abs() < 1e-9);
		// WCAG's own boundary: #767676 is the lightest grey that passes on white.
		assert!(ratio(0x767676, 0xFFFFFF) >= TEXT);
		assert!(ratio(0x777777, 0xFFFFFF) < TEXT);
	}

	#[test]
	fn a_color_that_already_reads_is_kept() {
		assert_eq!(readable(0x000000, 0xFFFFFF, &[0xFFFFFF], TEXT), 0x000000);
	}

	#[test]
	fn a_faint_color_moves_toward_the_target_just_far_enough() {
		let (faint, bg) = (0xCCCCCC, 0xFFFFFF);
		let color = readable(faint, 0x000000, &[bg], TEXT);
		assert!(ratio(color, bg) >= TEXT);
		// One step less would not read.
		let lighter = (0..=255)
			.map(|a| blend(faint, 0x000000, a))
			.take_while(|&c| c != color)
			.last()
			.unwrap();
		assert!(ratio(lighter, bg) < TEXT);
	}

	#[test]
	fn every_background_must_be_met() {
		let backgrounds = [0xFFFFFF, 0xDDDDDD];
		let color = readable(0xAAAAAA, 0x000000, &backgrounds, TEXT);
		assert!(backgrounds.iter().all(|&bg| ratio(color, bg) >= TEXT));
		assert!(
			ratio(color, 0xDDDDDD) < 6.0,
			"no darker than the worst case needs"
		);
	}

	#[test]
	fn an_unreachable_ratio_gives_the_target() {
		assert_eq!(readable(0x808080, 0x909090, &[0x888888], TEXT), 0x909090);
		assert_eq!(
			readable(0x123456, 0xABCDEF, &[], TEXT),
			0x123456,
			"no backgrounds"
		);
	}

	#[test]
	fn tab_text_reads_on_every_tab_background_in_every_theme() {
		for theme in built_in() {
			let (dim, accent) = tab_text_colors(&theme);
			for bg in tab_backgrounds(&theme) {
				let name = theme.kind.name();
				assert!(ratio(dim, bg) >= TEXT, "{name} dim {:.2}", ratio(dim, bg));
				assert!(ratio(accent, bg) >= TEXT, "{name} accent");
			}
		}
	}

	#[test]
	fn dim_text_stays_dimmer_than_the_foreground() {
		for theme in built_in() {
			let (dim, _) = tab_text_colors(&theme);
			assert!(
				ratio(dim, theme.bg) < ratio(theme.fg, theme.bg),
				"{}",
				theme.kind.name()
			);
		}
	}

	#[test]
	fn theme_colors_that_already_read_are_left_alone() {
		// macOS Dark's dim text passes as it was; Rose Pine's blue reads too.
		let dark = Theme::from_kind(ThemeKind::MacOsDark);
		assert_eq!(tab_text_colors(&dark).0, blend(dark.bg, dark.fg, DIM));
		let pine = Theme::from_kind(ThemeKind::RosePine);
		assert_eq!(tab_text_colors(&pine).1, pine.palette[4]);
	}

	#[test]
	fn green_weighs_most_and_blue_least() {
		// Relative luminance weights sRGB channels 0.2126, 0.7152, 0.0722.
		let on_black = |color| ratio(color, 0x000000);
		assert!((on_black(0xFF0000) - 5.252).abs() < 1e-3);
		assert!((on_black(0x00FF00) - 15.304).abs() < 1e-3);
		assert!((on_black(0x0000FF) - 2.444).abs() < 1e-3);
	}

	#[test]
	fn dark_channels_use_the_linear_part_of_the_srgb_curve() {
		// 10/255 sits below the 0.04045 knee, so it is divided by 12.92.
		let expected = (10.0 / 255.0 / 12.92 + 0.05) / 0.05;
		assert!((ratio(0x0A0A0A, 0x000000) - expected).abs() < 1e-9);
		// 11/255 sits above it and follows the power curve.
		let above = ((11.0 / 255.0 + 0.055) / 1.055f64).powf(2.4);
		assert!((ratio(0x0B0B0B, 0x000000) - (above + 0.05) / 0.05).abs() < 1e-9);
	}

	#[test]
	fn a_custom_theme_that_cannot_reach_the_ratio_falls_back_to_its_foreground() {
		let mut theme = Theme::from_kind(ThemeKind::Custom);
		(theme.fg, theme.bg, theme.border) = (0x777777, 0x888888, 0x888888);
		assert_eq!(tab_text_colors(&theme), (0x777777, 0x777777));
	}

	#[test]
	fn dim_text_darkens_on_light_themes_and_lightens_on_dark_ones() {
		let latte = Theme::from_kind(ThemeKind::CatppuccinLatte);
		let start = blend(latte.bg, latte.fg, DIM);
		assert!(luminance(tab_text_colors(&latte).0) < luminance(start));
		let frappe = Theme::from_kind(ThemeKind::CatppuccinFrappe);
		let start = blend(frappe.bg, frappe.fg, DIM);
		assert!(luminance(tab_text_colors(&frappe).0) > luminance(start));
	}

	#[test]
	fn exactly_meeting_the_ratio_is_enough() {
		// Any color has a ratio of exactly 1 with itself.
		assert_eq!(readable(0x336699, 0xFFFFFF, &[0x336699], 1.0), 0x336699);
	}

	#[test]
	fn colors_are_changed_only_in_themes_where_they_fail() {
		for theme in built_in() {
			let backgrounds = tab_backgrounds(&theme);
			let reads = |color: u32| backgrounds.iter().all(|&bg| ratio(color, bg) >= TEXT);
			let (dim, accent) = tab_text_colors(&theme);
			let (start_dim, start_accent) = (blend(theme.bg, theme.fg, DIM), theme.palette[4]);
			let name = theme.kind.name();
			assert_eq!(dim == start_dim, reads(start_dim), "{name} dim");
			assert_eq!(accent == start_accent, reads(start_accent), "{name} accent");
		}
	}
}
