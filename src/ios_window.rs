//! What iOS reports about the window: where it and its safe area are on
//! the screen and how large, read for `ui::insets::window_insets` along with
//! where the keyboard is.
use winit::{dpi::PhysicalPosition, window::Window};

use crate::ui::insets::{Insets, window_insets};

/// The insets of `window`, drawn at `factor` pixels a point.
pub(crate) fn insets(window: &Window, factor: f64) -> Insets {
	let place = |at: Result<PhysicalPosition<i32>, _>| at.map_or((0, 0), |at| (at.x, at.y));
	let at = (
		place(window.inner_position()),
		place(window.outer_position()),
	);
	let (outer, inner) = (window.outer_size(), window.inner_size());
	window_insets(outer, inner, at, crate::ios_keyboard::top(), factor)
}
