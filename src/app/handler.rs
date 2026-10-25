//! winit `ApplicationHandler`: window creation and event dispatch.
use super::*;
use crate::input::keys::Modifiers;
use std::time::Instant;
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesExtMacOS;
use winit::{
	application::ApplicationHandler,
	dpi::{LogicalSize, PhysicalSize},
	event::Ime,
	event_loop::ControlFlow,
	window::WindowId,
};

impl ApplicationHandler<UserEvent> for App {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if self.window.is_some() {
			return;
		}

		let attrs = crate::platform::identify(Window::default_attributes())
			.with_title("💩 poopterm")
			.with_inner_size(LogicalSize::new(900.0, 560.0))
			.with_min_inner_size(LogicalSize::new(480.0, 240.0));

		// On macOS, keep the native frame so the traffic lights exist,
		// but make the title bar transparent and hide the title text.
		#[cfg(target_os = "macos")]
		let attrs = attrs
			.with_titlebar_transparent(true)
			.with_title_hidden(true)
			.with_fullsize_content_view(true);

		let window = Arc::new(
			event_loop
				.create_window(attrs)
				.expect("failed to create window"),
		);
		#[cfg(target_os = "macos")]
		crate::platform::set_window_opacity(&window, Preferences::load().opacity);

		let (cols, rows) = {
			let term = self.active_term();
			(term.cols, term.rows)
		};
		self.renderer = Some(
			renderer::Renderer::new(window.clone(), self.preferences.renderer)
				.expect("failed to create renderer"),
		);
		self.window = Some(window);
		// pr-check: window glue: runs once the real window exists
		self.apply_scale();
		// pr-check: end window glue
		let size = PhysicalSize::new(
			(self.cw * cols + self.grid_left() + self.margin) as u32,
			(self.ch * rows + self.top + self.margin + self.status_height) as u32,
		);
		let _ = self.window.as_ref().unwrap().request_inner_size(size);
		self.update_window_title();
		if let Some(millihertz) = window_refresh_millihertz(self.window.as_deref()) {
			self.pacer.set_interval(std::time::Duration::from_micros(
				1_000_000_000 / u64::from(millihertz),
			));
		}

		let s = self.window.as_ref().unwrap().inner_size();
		self.resize(s.width, s.height);
	}

	fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
		if self
			.settings_window
			.as_ref()
			.is_some_and(|window| window.id() == id)
		{
			self.settings_window_event(event);
			return;
		}
		match event {
			WindowEvent::CloseRequested => {
				self.refresh_directories(false);
				self.persist_sessions();
				event_loop.exit();
			}
			WindowEvent::ModifiersChanged(m) => {
				let state = m.state();
				self.mods = Modifiers {
					ctrl: state.control_key(),
					shift: state.shift_key(),
					alt: state.alt_key(),
					super_key: state.super_key(),
				};
			}
			WindowEvent::Focused(focused) => {
				self.focused = focused;
				let reporting = self.active_term().focus_reporting;
				if reporting {
					let report: &[u8] = if focused { b"\x1b[I" } else { b"\x1b[O" };
					self.active_session().write(report);
				}
				self.request_redraw();
			}
			WindowEvent::Occluded(occluded) => self.occluded = occluded,
			WindowEvent::MouseWheel { delta, .. } => self.handle_mouse_wheel(delta),
			WindowEvent::Ime(Ime::Commit(text)) => {
				let bytes = text.into_bytes();
				self.active_session().write(&bytes);
			}
			WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
				self.handle_key(event_loop, event)
			}
			WindowEvent::CursorMoved { position, .. } => self.handle_cursor_moved(position),
			WindowEvent::MouseInput { state, button, .. } => {
				self.handle_mouse_input(event_loop, state, button)
			}
			WindowEvent::Resized(size) => self.resize(size.width, size.height),
			// pr-check: window glue: winit sends this when the window moves to another output or its scale changes
			WindowEvent::ScaleFactorChanged { .. } => {
				self.apply_scale();
				self.resize(self.width, self.height);
			}
			// pr-check: end window glue
			WindowEvent::RedrawRequested => self.draw(),
			_ => {}
		}
	}

	fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
		let now = Instant::now();
		let (mut expired, mut animated) = (false, false);
		// The earliest synchronized-output timeout or animation frame.
		let mut term_wake = None;
		for session in &self.sessions {
			let mut term = session.term();
			expired |= term.finish_sync_if_expired(now);
			animated |= term.advance_animations(now);
			term_wake = [term_wake, term.sync_deadline, term.animation_deadline()]
				.into_iter()
				.flatten()
				.min();
		}
		if expired {
			self.search_dirty = true;
			self.update_window_title();
		}
		if expired || animated {
			self.schedule_redraw();
		}
		if self.pacer.due(now) {
			self.request_redraw();
		}
		let wake = [term_wake, self.pacer.deadline()]
			.into_iter()
			.flatten()
			.min();
		event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
	}

	fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
		match event {
			UserEvent::Output(id) => self.process_output(id),
			UserEvent::Notification(key, event) => self.notification_event(key, event),
			UserEvent::Closed(id) => {
				if let Some(index) = self.sessions.iter().position(|s| s.id == id) {
					self.close_tab(index, event_loop);
				}
			}
		}
	}
}

/// The refresh rate of the monitor showing `window`, in millihertz.
fn window_refresh_millihertz(window: Option<&Window>) -> Option<u32> {
	window?
		.current_monitor()?
		.refresh_rate_millihertz()
		.filter(|&rate| rate >= 1000)
}
