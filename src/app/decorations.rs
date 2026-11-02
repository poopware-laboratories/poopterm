//! Choosing who draws the title bar on Linux and FreeBSD, from what the
//! compositor offers and the settings, and showing the desktop's buttons.
use super::*;
use crate::ui::decorations::{client_side, shown_buttons};
use std::time::Duration;

/// How long the settings portal may take, starting up included.
const PORTAL_TIMEOUT: Duration = Duration::from_secs(3);

impl App {
	/// Learns what the compositor and the desktop offer, once the window
	/// exists, and decorates it.
	pub(super) fn start_decorations(&mut self) {
		// pr-check: window glue: probes the real window's compositor and starts the portal read
		let window = self.window.clone();
		self.server_decorations = window
			.as_deref()
			.and_then(wayland::offers_server_decorations);
		let proxy = self.proxy.clone();
		crate::portal::read_button_layout(None, PORTAL_TIMEOUT, move |layout| {
			let _ = proxy.send_event(UserEvent::ButtonLayout(layout));
		});
		// pr-check: end window glue
		self.apply_decorations();
	}

	/// The desktop said which buttons it wants.
	pub(super) fn desktop_buttons(&mut self, layout: ButtonLayout) {
		self.desktop_buttons = Some(layout);
		self.apply_decorations();
	}

	/// Draws the title bar or leaves it to the desktop, as `client_side`
	/// chooses for the current settings.
	pub(super) fn apply_decorations(&mut self) {
		let client = client_side(&self.preferences, self.server_decorations);
		self.client_decorations = client;
		self.window_buttons = shown_buttons(client, self.desktop_buttons.as_ref());
		// pr-check: window glue: tells the real window what was chosen
		if let Some(window) = &self.window {
			window.set_decorations(!client);
		}
		// pr-check: end window glue
		self.request_redraw();
	}
}
