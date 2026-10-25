//! Display scale factors. Terminal text follows the exact factor (1.25,
//! 1.5, …); window chrome is drawn at one whole-number scale, so every part
//! of it agrees and its one-pixel lines stay crisp.

/// The largest scale honored; beyond it a factor is taken as broken.
pub(crate) const MAX_SCALE: usize = 8;
/// The smallest factor honored; smaller ones would shrink the font to nothing.
pub(crate) const MIN_FACTOR: f64 = 0.25;

/// The factor within the honored bounds, or 1 when it is not a usable number.
fn usable(factor: f64) -> f64 {
	if factor.is_finite() && factor > 0.0 {
		factor.clamp(MIN_FACTOR, MAX_SCALE as f64)
	} else {
		1.0
	}
}

/// The whole-number scale window chrome is drawn at: the factor rounded,
/// at least 1.
pub(crate) fn chrome_scale(factor: f64) -> usize {
	(usable(factor).round() as usize).max(1)
}

/// The font size the chrome is designed around: Settings' default.
pub(crate) const UI_POINTS: usize = 18;

/// The factor the chrome is drawn at: the display factor, grown with fonts
/// larger than [`UI_POINTS`] so tab and status text get larger with them.
pub(crate) fn chrome_factor(factor: f64, font_points: usize) -> f64 {
	factor * (font_points as f64 / UI_POINTS as f64).max(1.0)
}

/// The terminal font size in pixels for a size in points at `factor`.
pub(crate) fn font_px(points: usize, factor: f64) -> f32 {
	points as f32 * usable(factor) as f32
}

/// Window chrome sizes, in physical pixels.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Chrome {
	/// The inset frame around the client area.
	pub(crate) border: usize,
	/// The title bar the window content extends under (macOS), which
	/// holds the tab strip.
	pub(crate) titlebar_height: usize,
	pub(crate) tab_height: usize,
}

pub(crate) fn chrome(factor: f64, macos: bool) -> Chrome {
	let scale = chrome_scale(factor);
	// macOS draws its own rim, which follows the rounded corners.
	let (border, titlebar, tab) = if macos { (0, 44, 40) } else { (2, 0, 44) };
	Chrome {
		border: border * scale,
		titlebar_height: titlebar * scale,
		tab_height: tab * scale,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn whole_factors_are_used_as_they_are() {
		for factor in [1u32, 2, 3, 4] {
			assert_eq!(chrome_scale(f64::from(factor)), factor as usize);
		}
	}

	#[test]
	fn fractional_factors_round_to_the_nearest_whole_scale() {
		assert_eq!(chrome_scale(1.25), 1);
		assert_eq!(chrome_scale(1.49), 1);
		assert_eq!(chrome_scale(1.5), 2);
		assert_eq!(chrome_scale(1.75), 2);
		assert_eq!(chrome_scale(2.25), 2);
		assert_eq!(chrome_scale(2.5), 3);
	}

	#[test]
	fn chrome_is_never_scaled_below_one() {
		assert_eq!(chrome_scale(0.75), 1);
		assert_eq!(chrome_scale(0.4), 1, "rounds to 0");
		assert_eq!(chrome_scale(0.0), 1);
		assert_eq!(chrome_scale(-2.0), 1);
	}

	#[test]
	fn broken_factors_fall_back_to_one_and_huge_ones_are_bounded() {
		assert_eq!(chrome_scale(f64::NAN), 1);
		assert_eq!(chrome_scale(f64::INFINITY), 1);
		assert_eq!(chrome_scale(f64::NEG_INFINITY), 1);
		assert_eq!(chrome_scale(1e9), MAX_SCALE);
		assert_eq!(chrome_scale(MAX_SCALE as f64 + 0.4), MAX_SCALE);
	}

	#[test]
	fn text_follows_the_exact_factor() {
		assert_eq!(font_px(14, 1.0), 14.0);
		assert_eq!(font_px(14, 1.25), 17.5);
		assert_eq!(font_px(14, 1.5), 21.0);
		assert_eq!(font_px(12, 2.0), 24.0);
		assert_eq!(font_px(14, f64::NAN), 14.0, "broken factors draw at 1x");
		assert_eq!(font_px(14, 0.0), 14.0);
		assert_eq!(font_px(14, 1e9), 14.0 * MAX_SCALE as f32);
	}

	#[test]
	fn linux_chrome_has_a_border_and_no_title_bar() {
		let chrome = chrome(1.0, false);
		assert_eq!(
			chrome,
			Chrome {
				border: 2,
				titlebar_height: 0,
				tab_height: 44,
			}
		);
	}

	#[test]
	fn macos_chrome_has_a_title_bar_and_no_drawn_border() {
		// macOS draws its own rim, which follows the rounded corners.
		let chrome = chrome(1.0, true);
		assert_eq!(
			chrome,
			Chrome {
				border: 0,
				titlebar_height: 44,
				tab_height: 40,
			}
		);
	}

	#[test]
	fn every_chrome_size_uses_the_same_whole_scale() {
		// The tab bar is drawn at the whole scale, so its height must be too:
		// at 1.5 it used to be 66 pixels for a bar drawn at 2x.
		for macos in [false, true] {
			for factor in [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
				let (scale, base) = (chrome_scale(factor), chrome(1.0, macos));
				let scaled = chrome(factor, macos);
				assert_eq!(scaled.border, base.border * scale, "{factor}");
				assert_eq!(
					scaled.titlebar_height,
					base.titlebar_height * scale,
					"{factor}"
				);
				assert_eq!(scaled.tab_height, base.tab_height * scale, "{factor}");
			}
		}
	}

	#[test]
	fn the_border_is_at_least_two_pixels_at_any_scale() {
		// `(2.0 * scale) as usize` truncated; small factors must not lose it.
		for factor in [0.5, 1.0, 1.2, 1.4] {
			assert_eq!(chrome(factor, false).border, 2, "{factor}");
		}
	}

	#[test]
	fn tiny_factors_are_raised_to_the_smallest_scale() {
		// A factor that rounds the font to nothing would leave no cells.
		assert_eq!(font_px(14, 1e-300), 14.0 * MIN_FACTOR as f32);
		assert_eq!(font_px(14, 0.01), 14.0 * MIN_FACTOR as f32);
		assert_eq!(font_px(16, 0.5), 8.0, "small real factors are kept");
		assert_eq!(chrome(1e-300, false).border, 2);
	}

	#[test]
	fn factors_inside_the_bounds_are_not_clamped() {
		assert_eq!(font_px(10, 7.5), 75.0);
		assert_eq!(
			font_px(10, MIN_FACTOR + 0.25),
			10.0 * (MIN_FACTOR + 0.25) as f32
		);
		assert_eq!(chrome_scale(7.4), 7);
	}

	#[test]
	fn at_fractional_factors_text_and_chrome_scale_separately() {
		// (factor, chrome scale, 14 point font in pixels)
		for (factor, scale, font) in [
			(1.25, 1, 17.5),
			(1.5, 2, 21.0),
			(1.75, 2, 24.5),
			(2.25, 2, 31.5),
			(2.5, 3, 35.0),
		] {
			assert_eq!(font_px(14, factor), font, "{factor}");
			assert_eq!(chrome(factor, false).tab_height, 44 * scale, "{factor}");
			assert_eq!(chrome(factor, true).titlebar_height, 44 * scale, "{factor}");
		}
	}

	#[test]
	fn the_default_font_size_leaves_the_chrome_at_the_display_scale() {
		for factor in [1.0, 1.25, 1.5, 2.0, 3.0] {
			assert_eq!(chrome_factor(factor, UI_POINTS), factor);
		}
	}

	#[test]
	fn larger_fonts_grow_the_chrome_in_whole_steps() {
		let scale = |factor, points| chrome_scale(chrome_factor(factor, points));
		assert_eq!(scale(1.0, 26), 1, "1.44 rounds down");
		assert_eq!(scale(1.0, 27), 2, "1.5 rounds up");
		assert_eq!(scale(1.0, 32), 2);
		assert_eq!(scale(1.0, 72), 4);
		assert_eq!(scale(2.0, 32), 4);
		assert_eq!(scale(1.25, 24), 2, "both factors count");
	}

	#[test]
	fn smaller_fonts_never_shrink_the_chrome() {
		assert_eq!(chrome_factor(1.0, 6), 1.0);
		assert_eq!(chrome_factor(2.0, 10), 2.0);
		assert_eq!(chrome_factor(1.5, 0), 1.5);
	}

	#[test]
	fn the_chrome_scale_stays_bounded_at_any_font_size() {
		assert_eq!(chrome_scale(chrome_factor(8.0, 72)), MAX_SCALE);
		assert_eq!(
			chrome_scale(chrome_factor(f64::NAN, 72)),
			1,
			"a broken factor"
		);
		assert_eq!(chrome_scale(chrome_factor(1.0, usize::MAX)), MAX_SCALE);
	}
}
