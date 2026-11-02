//! The pointer over the title bar and edges the window draws itself:
//! routes what `ui::window_pointer` decides to the window.
use super::*;
use crate::ui::{
	window_controls::{WindowButton, resize_edge},
	window_pointer::{BarPress, Route, Surface, pointer_target},
};
use std::time::Instant;
use winit::{event::ElementState, window::ResizeDirection};

impl App {
	fn pointer_surface(&self) -> Surface {
		let maximized = self.window.as_ref().is_some_and(|w| w.is_maximized());
		Surface {
			size: (self.width as usize, self.height as usize),
			bar: (self.tab_y(), self.tab_height),
			scale: self.scale(),
			maximized,
		}
	}

	/// The edge under the pointer, when the window draws its own.
	pub(super) fn hovered_edge(&self) -> Option<ResizeDirection> {
		let Surface {
			size,
			scale,
			maximized,
			..
		} = self.pointer_surface();
		let edge = resize_edge(self.cursor_pos, size, scale, maximized);
		edge.filter(|_| self.client_decorations)
	}

	/// Handles a press or release of the main button on the window's own
	/// edges and buttons; false when it is not theirs.
	pub(super) fn handle_window_pointer(
		&mut self,
		event_loop: &ActiveEventLoop,
		state: ElementState,
	) -> bool {
		if !self.client_decorations {
			return false;
		}
		let target = pointer_target(
			&self.window_controls(),
			self.cursor_pos,
			self.pointer_surface(),
		);
		let route = self
			.window_presses
			.route(state == ElementState::Pressed, target);
		// pr-check: window glue: hands what the tested route decided to the window
		let window = self.window.clone();
		match (route, window) {
			(Route::Pass, _) => return false,
			(Route::Resize(edge), Some(window)) => drop(window.drag_resize_window(edge)),
			(Route::Act(WindowButton::Minimize), Some(window)) => window.set_minimized(true),
			(Route::Act(WindowButton::Maximize), Some(window)) => {
				window.set_maximized(!window.is_maximized())
			}
			(Route::Act(WindowButton::Close), _) => self.close_window(event_loop),
			_ => {}
		}
		// pr-check: end window glue
		self.request_redraw();
		true
	}

	/// A press on the empty header bar: drags the window, or maximizes and
	/// restores it on a double-click.
	pub(super) fn press_bar(&mut self) {
		if !self.client_decorations {
			return;
		}
		let press = self
			.window_presses
			.press_bar(Instant::now(), self.cursor_pos, self.scale());
		// pr-check: window glue: hands what the tested press_bar decided to the window
		let Some(window) = &self.window else { return };
		match press {
			BarPress::Drag => drop(window.drag_window()),
			BarPress::ToggleMaximize => window.set_maximized(!window.is_maximized()),
		}
		// pr-check: end window glue
	}

	/// Saves the sessions and quits, as the window system's close does.
	pub(super) fn close_window(&mut self, event_loop: &ActiveEventLoop) {
		self.refresh_directories(false);
		self.persist_sessions();
		event_loop.exit();
	}
}
