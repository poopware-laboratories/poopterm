//! What the system keeps of the window's edges on iOS: the safe area (the
//! status bar, the home indicator) and the on-screen keyboard, which the
//! terminal stays clear of.
#![cfg_attr(
	all(not(target_os = "ios"), not(test)),
	expect(dead_code, reason = "only iOS reports insets")
)]

/// The top and bottom of the screen the system keeps for itself, from a
/// window `outer` pixels tall whose safe area starts `inner_top` pixels
/// down and is `inner_height` pixels tall.
pub(crate) fn safe_insets(outer: usize, inner_top: usize, inner_height: usize) -> (usize, usize) {
	let top = inner_top.min(outer);
	(top, outer.saturating_sub(top + inner_height))
}

/// Points of a screen `screen` points tall that a keyboard whose top is
/// `top` points down covers: none once it has moved off the bottom.
pub(crate) fn covered_points(screen: f64, top: f64) -> f64 {
	let covered = screen - top;
	if covered.is_finite() {
		covered.clamp(0.0, screen.max(0.0))
	} else {
		0.0
	}
}

/// The bottom inset in pixels: the safe area's, or the keyboard's
/// (`keyboard` points at `factor` pixels each) while it reaches higher.
pub(crate) fn bottom_inset(safe_bottom: usize, keyboard: f64, factor: f64) -> usize {
	let pixels = (keyboard * factor).round();
	let keyboard = if pixels.is_finite() && pixels > 0.0 {
		pixels as usize
	} else {
		0
	};
	safe_bottom.max(keyboard)
}

/// Where a sheet such as Settings goes on a phone, in points: the whole
/// width of a window `size` pixels at `factor`, from below the top inset
/// (`safe`, pixels, top and bottom) down to at most `height` points, so
/// what is under it shows and a tap there can close it.
pub(crate) fn sheet_frame(
	size: (u32, u32),
	safe: (usize, usize),
	factor: f64,
	height: f64,
) -> ((f64, f64), (f64, f64)) {
	let factor = if factor.is_finite() && factor > 0.0 {
		factor
	} else {
		1.0
	};
	let (width, tall) = (f64::from(size.0) / factor, f64::from(size.1) / factor);
	let (top, bottom) = (safe.0 as f64 / factor, safe.1 as f64 / factor);
	let room = (tall - top - bottom).max(0.0);
	((0.0, top.min(tall)), (width, height.clamp(0.0, room)))
}

#[cfg(test)]
#[path = "insets_tests.rs"]
mod tests;
