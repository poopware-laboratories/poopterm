//! Keyboard events: application shortcuts, overlays, then PTY input.
use super::*;
use crate::input::{
	keypad::encode_keypad,
	keys::{KeyModes, encode_key},
};
use crate::ui::overlays::{palette_matches, stays_open};
use winit::{event::KeyEvent, keyboard::KeyLocation};

impl App {
	pub(super) fn key_modes(&self) -> KeyModes {
		let term = self.active_term();
		KeyModes {
			application_cursor: term.application_cursor,
			application_keypad: term.application_keypad,
			newline: term.newline_mode,
			options: term.key_options,
			keyboard: term.keyboard,
			kitty: term.kitty_keyboard,
		}
	}

	/// Copies the output of the most recent command (needs shell integration).
	fn copy_last_command_output(&self) {
		let Some(text) = self.active_term().last_command_output() else {
			return;
		};
		if let Err(error) = clipboard_write(&text) {
			eprintln!("poopterm: could not copy command output: {error}");
		}
	}

	pub(super) fn handle_key(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
		if self
			.shortcuts
			.command_palette
			.matches(&event.logical_key, self.mods)
		{
			self.end_search();
			self.command_palette = true;
			self.renaming_tab = false;
			self.command_query.clear();
			if let Some(window) = &self.window {
				window.request_redraw();
			}
			return;
		}
		if self.command_palette {
			if self.renaming_tab {
				match &event.logical_key {
					Key::Named(NamedKey::Escape) => {
						self.renaming_tab = false;
						self.command_palette = false;
					}
					Key::Named(NamedKey::Backspace) => {
						self.command_query.pop();
					}
					Key::Named(NamedKey::Enter) => {
						let title = self.command_query.trim().to_owned();
						self.sessions[self.active].custom_title = (!title.is_empty()).then_some(title);
						self.renaming_tab = false;
						self.command_palette = false;
						self.update_window_title();
					}
					Key::Character(value) if !self.mods.ctrl && !self.mods.super_key => {
						let available = 64usize.saturating_sub(self.command_query.chars().count());
						self
							.command_query
							.extend(value.chars().filter(|c| !c.is_control()).take(available));
					}
					_ => {}
				}
				if let Some(window) = &self.window {
					window.request_redraw();
				}
				return;
			}
			match &event.logical_key {
				Key::Named(NamedKey::Escape) => self.command_palette = false,
				Key::Named(NamedKey::Backspace) => {
					self.command_query.pop();
				}
				Key::Named(NamedKey::Enter) => {
					let command = palette_matches(&self.command_query).first().copied();
					self.command_palette = command.is_some_and(stays_open);
					if let Some(command) = command {
						match command {
							"Rename active tab" => {
								self.renaming_tab = true;
								let title = self.sessions[self.active].title();
								self.command_query = title;
							}
							"New tab" => {
								self.command_palette = false;
								self.add_tab();
							}
							"Settings" => {
								self.command_palette = false;
								self.open_settings_window(event_loop);
							}
							"About poopterm" => {
								self.command_palette = false;
								self.open_settings_window(event_loop);
								self.settings_editor.page = super::settings_hit::SettingsPage::About;
								if let Some(window) = &self.settings_window {
									window.request_redraw();
								}
							}
							"Cycle theme" => {
								self.command_palette = false;
								self.cycle_theme();
							}
							"Close tab" => {
								self.command_palette = false;
								self.close_active_tab(event_loop);
							}
							"Copy last command output" => self.copy_last_command_output(),
							_ => {}
						}
					}
				}
				Key::Character(value) if !self.mods.ctrl && !self.mods.super_key => {
					let available = 64usize.saturating_sub(self.command_query.chars().count());
					self
						.command_query
						.extend(value.chars().filter(|c| !c.is_control()).take(available));
				}
				_ => {}
			}
			if let Some(window) = &self.window {
				window.request_redraw();
			}
			return;
		}
		let search_shortcut = self.shortcuts.search.matches(&event.logical_key, self.mods);
		if search_shortcut {
			self.begin_search();
			return;
		}
		if self.search_active {
			if (self.mods.super_key || (self.mods.ctrl && self.mods.shift))
				&& matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("v"))
			{
				if let Ok(text) = clipboard_read() {
					let available = 128usize.saturating_sub(self.search_query.chars().count());
					self
						.search_query
						.extend(text.chars().filter(|c| !c.is_control()).take(available));
					self.search(1, false);
				}
				return;
			}
			match &event.logical_key {
				Key::Named(NamedKey::Escape) => self.end_search(),
				Key::Named(NamedKey::Enter) => self.search(if self.mods.shift { -1 } else { 1 }, true),
				Key::Named(NamedKey::Backspace) => {
					self.search_query.pop();
					self.search(1, false);
				}
				Key::Character(value) if !self.mods.ctrl && !self.mods.super_key => {
					let available = 128usize.saturating_sub(self.search_query.chars().count());
					self.search_query.extend(value.chars().take(available));
					self.search(1, false);
				}
				_ => {}
			}
			return;
		}
		if self
			.shortcuts
			.cycle_theme
			.matches(&event.logical_key, self.mods)
		{
			self.cycle_theme();
			return;
		}
		if let Some(zoom) = zoom_action(&self.shortcuts, &event.logical_key, self.mods) {
			self.font_points = zoomed(self.font_points, self.preferences.font_size, zoom);
			self.apply_scale();
			self.resize(self.width, self.height);
			return;
		}
		let prompt_jump = [
			(&self.shortcuts.previous_prompt, -1),
			(&self.shortcuts.next_prompt, 1),
		]
		.into_iter()
		.find_map(|(shortcut, direction)| {
			shortcut
				.matches(&event.logical_key, self.mods)
				.then_some(direction)
		});
		if let Some(direction) = prompt_jump {
			if self.active_term().jump_to_prompt(direction) {
				self.request_redraw();
			}
			return;
		}
		if self
			.shortcuts
			.new_tab
			.matches(&event.logical_key, self.mods)
		{
			self.add_tab();
			return;
		}
		let copy_shortcut = self.mods.super_key
			&& matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("c"))
			|| self.mods.ctrl
				&& self.mods.shift
				&& matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("c"));
		if copy_shortcut {
			if self.selection.is_some() {
				self.copy_selection();
			}
			return;
		}
		let paste_shortcut = self.mods.super_key
			&& matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("v"))
			|| self.mods.ctrl
				&& self.mods.shift
				&& matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("v"));
		if paste_shortcut {
			self.paste_clipboard();
			return;
		}
		if matches!(&event.logical_key, Key::Named(NamedKey::Tab)) && self.mods.ctrl {
			self.switch_tab(if self.mods.shift { -1 } else { 1 });
			return;
		}
		if (self.mods.super_key || self.mods.ctrl)
			&& !self.mods.shift
			&& let Key::Character(value) = &event.logical_key
			&& value.len() == 1
			&& let Some(index) = value.chars().next().and_then(|ch| ch.to_digit(10))
			&& (1..=9).contains(&index)
			&& (index as usize) <= self.sessions.len()
		{
			self.activate_tab(index as usize - 1);
			return;
		}
		if self
			.shortcuts
			.close_tab
			.matches(&event.logical_key, self.mods)
		{
			self.close_active_tab(event_loop);
			return;
		}
		// KAM: a locked keyboard sends nothing; DECARM reset drops repeats.
		let term = self.active_term();
		let blocked = term.keyboard_locked || (event.repeat && !term.keyboard.auto_repeat);
		drop(term);
		if blocked {
			return;
		}
		let modes = self.key_modes();
		let keypad = (event.location == KeyLocation::Numpad)
			.then(|| encode_keypad(&event.logical_key, self.mods, modes))
			.flatten();
		if let Some(bytes) = keypad.or_else(|| encode_key(&event.logical_key, self.mods, modes)) {
			self.active_session().write(&bytes);
			// Typing means finished work reported in this session was seen,
			// and shows the live screen the input goes to.
			let mut term = self.active_term();
			if term.local_echo {
				term.echo_input(&bytes);
			}
			let generation = term.status.generation;
			term.status.acknowledge();
			let scrolled = term.scroll_to_bottom();
			let changed = scrolled || term.status.generation != generation;
			drop(term);
			if changed {
				self.request_redraw();
			}
		}
	}
}
