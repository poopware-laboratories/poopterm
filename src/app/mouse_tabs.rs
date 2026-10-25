//! Mouse handling for horizontal tab scrolling and the vertical tab sidebar.
use super::*;
use crate::{
	render::header::Icon,
	ui::sidebar::{SidebarTarget, sidebar_target},
};
use winit::event::{ElementState, MouseButton};

impl App {
	pub(super) fn is_over_vertical_sidebar(&self, x: f64) -> bool {
		x < self.sidebar_width() as f64
	}

	pub(super) fn handle_vertical_tab_wheel(&mut self, x: f64, y: f64, scroll_up: bool) -> bool {
		if !self.preferences.show_tabs
			|| !self.preferences.vertical_tabs
			|| x >= self.vertical_tab_layout().sidebar_width as f64
			|| y < self.titlebar_height as f64
		{
			return false;
		}
		let layout = self.vertical_tab_layout();
		if layout.first_visible > 0 && scroll_up {
			self.tab_scroll -= 1;
		} else if layout.first_visible + layout.visible_count < self.sessions.len() && !scroll_up {
			self.tab_scroll += 1;
		}
		self.request_redraw();
		true
	}

	pub(super) fn handle_vertical_tab_mouse(
		&mut self,
		event_loop: &ActiveEventLoop,
		state: ElementState,
		button: MouseButton,
		(x, y): (f64, f64),
	) -> bool {
		if !self.preferences.show_tabs || !self.preferences.vertical_tabs {
			return false;
		}
		let layout = self.vertical_tab_layout();
		if x >= layout.sidebar_width as f64 || y < self.titlebar_height as f64 {
			return false;
		}
		if state == ElementState::Pressed && button == MouseButton::Left {
			// pr-check: window glue: routes a click to what the tested sidebar_target found
			match sidebar_target(&layout, self.sessions.len(), (x, y)) {
				Some(SidebarTarget::Button(Icon::Menu)) => self.open_settings_window(event_loop),
				Some(SidebarTarget::Button(_)) => self.add_tab(),
				Some(SidebarTarget::Tab { index, close: true }) => self.close_tab(index, event_loop),
				Some(SidebarTarget::Tab { index, .. }) => self.activate_tab(index),
				None if y < layout.tab_start_y as f64 => {
					let _ = self.window.as_ref().map(|window| window.drag_window());
				}
				None => {}
			}
			// pr-check: end window glue
			self.request_redraw();
		}
		true
	}
}
