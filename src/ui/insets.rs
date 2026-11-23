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

/// What the system keeps of a window `heights` (outer, safe area) pixels
/// tall whose safe area starts `top` down, with a keyboard covering
/// `keyboard` points at `factor`: the top and bottom insets, the bottom
/// being the keyboard's while it reaches higher, and the home indicator's
/// own bottom inset.
pub(crate) fn system_insets(
	(outer, inner): (u32, u32),
	top: usize,
	keyboard: f64,
	factor: f64,
) -> ((usize, usize), usize) {
	let (top, home) = safe_insets(outer as usize, top, inner as usize);
	((top, bottom_inset(home, keyboard, factor)), home)
}

#[cfg(test)]
#[path = "insets_tests.rs"]
mod tests;
