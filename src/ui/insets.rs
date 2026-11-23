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

/// What the system keeps of a window: its top and bottom (the bottom
/// being the keyboard's while it reaches higher), the home indicator's own
/// bottom, and the left and right, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Insets {
	pub(crate) safe: (usize, usize),
	pub(crate) home: usize,
	pub(crate) sides: (usize, usize),
}

/// The insets of a window `outer` large whose safe area is `inner` large,
/// with `(safe area, window)` at `at` on the screen, and the keyboard's top
/// edge `keyboard` points down the screen (infinite when it is away), at
/// `factor` pixels a point. An iPad window need not start at the screen's
/// corner, so everything is measured from the window's own.
pub(crate) fn window_insets(
	outer: winit::dpi::PhysicalSize<u32>,
	inner: winit::dpi::PhysicalSize<u32>,
	((inner_x, inner_y), (outer_x, outer_y)): ((i32, i32), (i32, i32)),
	keyboard: f64,
	factor: f64,
) -> Insets {
	let start = |inner: i32, outer: i32| usize::try_from(inner.saturating_sub(outer)).unwrap_or(0);
	let (top, home) = safe_insets(
		outer.height as usize,
		start(inner_y, outer_y),
		inner.height as usize,
	);
	let sides = safe_insets(
		outer.width as usize,
		start(inner_x, outer_x),
		inner.width as usize,
	);
	let factor = if factor.is_finite() && factor > 0.0 {
		factor
	} else {
		1.0
	};
	let (window_top, tall) = (
		f64::from(outer_y) / factor,
		f64::from(outer.height) / factor,
	);
	let covered = covered_points(window_top + tall, keyboard).min(tall);
	Insets {
		safe: (top, bottom_inset(home, covered, factor)),
		home,
		sides,
	}
}

#[cfg(test)]
#[path = "insets_tests.rs"]
mod tests;
