//! Keyboard events: application shortcuts, overlays, then PTY input.
use super::*;
use crate::input::{
	keypad::encode_keypad,
	keys::{KeyModes, encode_key},
	shortcuts::ShortcutAction,
};
use crate::ui::overlays::{palette_matches, selected_command, stays_open, step_selection};
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
			crate::warnings::warning!("could not copy command output: {error}");
		}
	}

	pub(super) fn handle_key(&mut self, event_loop: &ActiveEventLoop, event: KeyEvent) {
		let typed = Typed {
			key: event.logical_key,
			repeat: event.repeat,
			numpad: event.location == KeyLocation::Numpad,
		};
		// The key bar's Ctrl, held for this key (iOS); iOS reports no
		// modifiers of its own, so they go back to none after it.
		let held = self.mods;
		self.mods = self.key_bar.take(&typed.key, held);
		let latched = self.mods != held;
		self.handle_typed(event_loop, &typed);
		if latched {
			self.mods = held;
			self.request_redraw();
		}
	}

	/// Opens the command palette, empty.
	pub(super) fn open_command_palette(&mut self) {
		self.end_search();
		self.command_palette = true;
		self.renaming_tab = false;
		self.command_query.clear();
		self.command_selected = 0;
		self.request_redraw();
	}

	/// Does what typing `typed` does, with the modifiers held now: answers a
	/// prompt, drives the palette or search, runs a shortcut, or sends it.
	fn handle_typed(&mut self, event_loop: &ActiveEventLoop, typed: &Typed) {
		let event = typed;
		if self.answer_close(&event.key, event_loop) {
			return;
		}
		if self.hint_key(&event.key) {
			return;
		}
		if self
			.shortcuts
			.command_palette
			.matches(&event.key, self.mods)
		{
			return self.open_command_palette();
		}
		if self.command_palette {
			if self.renaming_tab {
				match &event.key {
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
			match &event.key {
				Key::Named(NamedKey::Escape) => self.command_palette = false,
				Key::Named(NamedKey::Backspace) => {
					self.command_query.pop();
					self.command_selected = 0;
				}
				Key::Named(key @ (NamedKey::ArrowDown | NamedKey::ArrowUp)) => {
					// pr-check: window glue: steps the tested selection by the arrow's direction
					let step = if *key == NamedKey::ArrowDown { 1 } else { -1 };
					let count = palette_matches(&self.command_query).len();
					self.command_selected = step_selection(self.command_selected, count, step);
					// pr-check: end window glue
				}
				Key::Named(NamedKey::Enter) => {
					let command = selected_command(&self.command_query, self.command_selected);
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
								self.open_about(event_loop);
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
							"Save scrollback to file" => {
								self.command_palette = false;
								self.save_scrollback();
							}
							"Broadcast input to all panes" => {
								self.command_palette = false;
								self.toggle_broadcast();
							}
							other => {
								if let Some(action) = crate::ui::overlays::command_action(other) {
									self.command_palette = false;
									self.run_action(action, event_loop);
								}
							}
						}
					}
				}
				Key::Character(value) if !self.mods.ctrl && !self.mods.super_key => {
					let available = 64usize.saturating_sub(self.command_query.chars().count());
					self
						.command_query
						.extend(value.chars().filter(|c| !c.is_control()).take(available));
					self.command_selected = 0;
				}
				_ => {}
			}
			if let Some(window) = &self.window {
				window.request_redraw();
			}
			return;
		}
		let search_shortcut = self.shortcuts.search.matches(&event.key, self.mods);
		if search_shortcut {
			self.begin_search();
			return;
		}
		if self.search_active {
			if self.shortcuts.paste.matches(&event.key, self.mods) {
				if let Ok(text) = clipboard_read() {
					let available = 128usize.saturating_sub(self.search_query.chars().count());
					self
						.search_query
						.extend(text.chars().filter(|c| !c.is_control()).take(available));
					self.search(1, false);
				}
				return;
			}
			match &event.key {
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
		if self.shortcuts.cycle_theme.matches(&event.key, self.mods) {
			self.cycle_theme();
			return;
		}
		if let Some(zoom) = zoom_action(&self.shortcuts, &event.key, self.mods) {
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
		.find_map(|(shortcut, direction)| shortcut.matches(&event.key, self.mods).then_some(direction));
		if let Some(direction) = prompt_jump {
			if self.active_term().jump_to_prompt(direction) {
				self.request_redraw();
			}
			return;
		}
		if self.shortcuts.new_tab.matches(&event.key, self.mods) {
			self.add_tab();
			return;
		}
		if let Some(command) = (self.shortcuts.panes).command(&event.key, self.mods) {
			self.run_pane_command(command);
			return;
		}
		let clipboard = ShortcutAction::CLIPBOARD;
		match (self.shortcuts).matching(&clipboard, &event.key, self.mods) {
			Some(ShortcutAction::Copy) if self.selection.is_some() => return self.copy_selection(),
			Some(ShortcutAction::Paste) => return self.paste_clipboard(),
			Some(ShortcutAction::PreviousTab) => return self.switch_tab(-1),
			Some(ShortcutAction::NextTab) => return self.switch_tab(1),
			Some(ShortcutAction::QuickSelect) => return self.start_hints(),
			Some(_) => return,
			None => {}
		}
		if (self.mods.super_key || self.mods.ctrl)
			&& !self.mods.shift
			&& let Key::Character(value) = &event.key
			&& value.len() == 1
			&& let Some(index) = value.chars().next().and_then(|ch| ch.to_digit(10))
			&& (1..=9).contains(&index)
			&& (index as usize) <= self.sessions.len()
		{
			self.activate_tab(index as usize - 1);
			return;
		}
		if self.shortcuts.close_tab.matches(&event.key, self.mods) {
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
		let keypad = (event.numpad)
			.then(|| encode_keypad(&event.key, self.mods, modes))
			.flatten();
		if let Some(bytes) = keypad.or_else(|| encode_key(&event.key, self.mods, modes)) {
			self.write_input(&bytes);
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

	/// Does what a key of the bar above the on-screen keyboard does: types
	/// its key, as the keyboard's own would be, holds Ctrl, or opens the
	/// palette.
	pub(super) fn press_bar_key(
		&mut self,
		event_loop: &ActiveEventLoop,
		key: crate::ui::key_bar::BarKey,
	) {
		use crate::ui::key_bar::Press;
		match self.key_bar.press(key) {
			Press::Send(key, mods) => {
				let held = std::mem::replace(&mut self.mods, mods);
				let typed = Typed {
					key,
					repeat: false,
					numpad: false,
				};
				self.handle_typed(event_loop, &typed);
				self.mods = held;
			}
			Press::Latched(_) => {}
			Press::Palette => self.open_command_palette(),
			Press::HideKeyboard => self.hide_keyboard(),
		}
		self.request_redraw();
	}

	fn hide_keyboard(&self) {
		// pr-check: window glue: resigns the UIKit terminal input view
		#[cfg(target_os = "ios")]
		if let Some(window) = &self.window {
			window.set_ime_allowed(false);
		}
		// pr-check: end window glue
	}

	/// Activates the selected palette row using the same key path as Return.
	pub(super) fn activate_palette_command(&mut self, event_loop: &ActiveEventLoop) {
		let typed = Typed {
			key: Key::Named(NamedKey::Enter),
			repeat: false,
			numpad: false,
		};
		self.handle_typed(event_loop, &typed);
	}
}

impl App {
	/// Does what typing `key` with `mods` held on a hardware keyboard does
	/// (iOS, where winit reports neither).
	#[cfg_attr(
		not(target_os = "ios"),
		expect(dead_code, reason = "only iOS reports hardware keys apart")
	)]
	pub(super) fn press_hardware_key(
		&mut self,
		event_loop: &ActiveEventLoop,
		key: Key,
		mods: Modifiers,
	) {
		let held = std::mem::replace(&mut self.mods, mods);
		let typed = Typed {
			key,
			repeat: false,
			numpad: false,
		};
		self.handle_typed(event_loop, &typed);
		self.mods = held;
		self.request_redraw();
	}
}

/// A key typed, from the keyboard or the key bar: what `handle_typed` needs.
pub(super) struct Typed {
	key: Key,
	repeat: bool,
	/// Typed on the numeric keypad, which keypad modes encode apart.
	numpad: bool,
}
