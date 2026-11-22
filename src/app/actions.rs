//! Running shortcut actions and palette commands by name, saving the
//! scrollback, and sending input to the focused pane or, while
//! broadcasting, to every pane of the tab.
use super::*;
use crate::{
	input::shortcuts::ShortcutAction,
	ui::{broadcast::input_targets, export, zoom::Zoom},
};

impl App {
	/// Does what `action`'s shortcut does, as the palette runs it.
	pub(super) fn run_action(&mut self, action: ShortcutAction, event_loop: &ActiveEventLoop) {
		// pr-check: window glue: each arm calls what the shortcut already does
		let zoom = |zoom| Some(zoom);
		let zoom = match action {
			ShortcutAction::NewTab => return self.add_tab(),
			ShortcutAction::CommandPalette => return,
			ShortcutAction::CycleTheme => return self.cycle_theme(),
			ShortcutAction::Search => return self.begin_search(),
			ShortcutAction::CloseTab => return self.close_active_tab(event_loop),
			ShortcutAction::PreviousPrompt => return self.jump_to_prompt(-1),
			ShortcutAction::NextPrompt => return self.jump_to_prompt(1),
			ShortcutAction::ZoomIn => zoom(Zoom::In),
			// pr-check: end window glue
			// pr-check: window glue: each arm calls what the shortcut already does
			ShortcutAction::ZoomOut => zoom(Zoom::Out),
			ShortcutAction::ZoomReset => zoom(Zoom::Reset),
			ShortcutAction::Pane(command) => return self.run_pane_command(command),
			ShortcutAction::Copy => return self.copy_selection(),
			ShortcutAction::Paste => return self.paste_clipboard(),
			ShortcutAction::PreviousTab => return self.switch_tab(-1),
			ShortcutAction::NextTab => return self.switch_tab(1),
			ShortcutAction::QuickSelect => return self.start_hints(),
		};
		// pr-check: end window glue
		if let Some(zoom) = zoom {
			self.zoom(zoom);
		}
	}

	/// Zooms the font as the zoom shortcuts do.
	pub(super) fn zoom(&mut self, zoom: Zoom) {
		// pr-check: window glue: applies the tested zoom step to the real window
		self.font_points = zoomed(self.font_points, self.preferences.font_size, zoom);
		self.apply_scale();
		self.resize(self.width, self.height);
		// pr-check: end window glue
	}

	/// Moves the view to the previous or next prompt, as the shortcuts do.
	pub(super) fn jump_to_prompt(&mut self, direction: isize) {
		if self.active_term().jump_to_prompt(direction) {
			self.request_redraw();
		}
	}

	/// Writes the active tab's history and screen to a dated file in the
	/// downloads folder, or the home folder without one.
	pub(super) fn save_scrollback(&self) {
		// pr-check: window glue: writes the tested export where the user keeps files
		let text = export::history_text(&self.active_term());
		let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
		let name = export::file_name(now.map_or(0, |since| since.as_secs()));
		let home = std::env::var_os("HOME")
			.map(std::path::PathBuf::from)
			.unwrap_or_default();
		let folder = Some(home.join("Downloads"))
			.filter(|d| d.is_dir())
			.unwrap_or(home);
		if let Err(error) = std::fs::write(folder.join(&name), text) {
			crate::warnings::warning!("could not save the scrollback: {error}");
		}
		// pr-check: end window glue
	}

	/// Turns broadcasting input to every pane of the tab on or off.
	pub(super) fn toggle_broadcast(&mut self) {
		self.broadcast = !self.broadcast;
		self.request_redraw();
	}

	/// Sends what the user typed or pasted to the focused pane, or to every
	/// pane of the tab while broadcasting.
	pub(super) fn write_input(&self, bytes: &[u8]) {
		// pr-check: window glue: writes to the sessions the tested targets name
		let focused = self.sessions[self.active].id;
		let panes: Vec<u64> = self
			.tab_places(self.active)
			.iter()
			.map(|place| place.id)
			.collect();
		let targets = input_targets(focused, &panes, self.broadcast);
		for session in self
			.all_sessions()
			.filter(|session| targets.contains(&session.id))
		{
			session.write(bytes);
		}
		// pr-check: end window glue
	}
}
