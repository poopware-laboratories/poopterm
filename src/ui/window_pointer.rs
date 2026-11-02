//! The pointer over a window that draws its own title bar: resize edges,
//! the window buttons, and double-clicks on the empty bar.
use super::window_controls::{Placement, WindowButton, button_at, resize_edge};
use std::time::{Duration, Instant};
use winit::window::{CursorIcon, ResizeDirection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointerTarget {
	Resize(ResizeDirection),
	Button(WindowButton),
}

/// The window the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Surface {
	pub size: (usize, usize),
	/// The header bar's top and height.
	pub bar: (usize, usize),
	pub scale: usize,
	pub maximized: bool,
}

/// What a press at `point` acts on: an edge first, since the edges overlap
/// the bar and the grid, then a window button.
pub(crate) fn pointer_target(
	placement: &Placement,
	point: (f64, f64),
	surface: Surface,
) -> Option<PointerTarget> {
	let Surface { size, scale, .. } = surface;
	resize_edge(point, size, scale, surface.maximized)
		.map(PointerTarget::Resize)
		.or_else(|| button_at(placement, point, surface.bar, scale).map(PointerTarget::Button))
}

/// The cursor over a resize `edge`, else a hand over a link, else the
/// program's OSC 22 `shape`.
pub(crate) fn pointer_icon(
	edge: Option<ResizeDirection>,
	link: bool,
	shape: Option<&str>,
) -> CursorIcon {
	if let Some(edge) = edge {
		return edge.into();
	}
	let shape = if link { Some("pointer") } else { shape };
	shape
		.and_then(|name| name.parse().ok())
		.unwrap_or(CursorIcon::Default)
}

/// How far apart in time two presses on the bar may be to maximize, as
/// GTK's default.
pub(crate) const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// And how far apart in unscaled pixels, on each axis.
const DOUBLE_CLICK_DISTANCE: f64 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarPress {
	Drag,
	ToggleMaximize,
}

/// What to do with a press or release of the main button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
	/// Let the window system resize the window from this edge.
	Resize(ResizeDirection),
	/// Act on this button.
	Act(WindowButton),
	/// Ours, with nothing to do yet but redraw the button.
	Held,
	/// Not ours: the header or the grid handles it.
	Pass,
}

/// The window button held down, and the last press on the bar.
#[derive(Debug, Default)]
pub(crate) struct Presses {
	held: Option<WindowButton>,
	last_bar: Option<(Instant, (f64, f64))>,
}

impl Presses {
	pub(crate) fn press(&mut self, button: WindowButton) {
		self.held = Some(button);
		self.last_bar = None;
	}

	pub(crate) fn held(&self) -> Option<WindowButton> {
		self.held
	}

	/// The button to act on when the press ends over `under`: the one held,
	/// if the pointer is still over it.
	pub(crate) fn release(&mut self, under: Option<WindowButton>) -> Option<WindowButton> {
		self.held.take().filter(|&held| Some(held) == under)
	}

	/// Routes a press (`pressed`) or release over `target`. A release ends a
	/// button press wherever it happens, so the grid never sees it.
	pub(crate) fn route(&mut self, pressed: bool, target: Option<PointerTarget>) -> Route {
		let under = match target {
			Some(PointerTarget::Button(button)) => Some(button),
			_ => None,
		};
		match (pressed, target) {
			(true, Some(PointerTarget::Resize(edge))) => Route::Resize(edge),
			(true, Some(PointerTarget::Button(button))) => {
				self.press(button);
				Route::Held
			}
			(false, _) if self.held.is_some() => self.release(under).map_or(Route::Held, Route::Act),
			_ => Route::Pass,
		}
	}

	/// A press on the empty bar at `point`: the second of two close ones
	/// maximizes or restores, any other drags.
	pub(crate) fn press_bar(&mut self, at: Instant, point: (f64, f64), scale: usize) -> BarPress {
		let reach = DOUBLE_CLICK_DISTANCE * scale as f64;
		let double = self.last_bar.take().is_some_and(|(then, last)| {
			at.checked_duration_since(then)
				.is_some_and(|gap| gap <= DOUBLE_CLICK)
				&& (point.0 - last.0).abs() <= reach
				&& (point.1 - last.1).abs() <= reach
		});
		if double {
			BarPress::ToggleMaximize
		} else {
			self.last_bar = Some((at, point));
			BarPress::Drag
		}
	}
}
