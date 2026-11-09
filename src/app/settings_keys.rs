//! Keyboard input in the Settings window: shortcut recording, saving,
//! closing, the theme arrows and the font list.
use super::{
	settings_focus::{center, step_focus},
	settings_hit::{SettingsPage, font_list_rows},
	settings_view::step_theme,
	*,
};
use crate::input::shortcuts::Shortcut;
use winit::event::KeyEvent;

impl App {
	pub(super) fn settings_key(&mut self, event: &KeyEvent) {
		let key = &event.logical_key;
		if let Some(action) = self.settings_editor.recording_shortcut {
			if matches!(key, Key::Named(NamedKey::Escape)) {
				self.settings_editor.recording_shortcut = None;
				self.settings_editor.status = "Shortcut change cancelled.".into();
			} else if let Some(shortcut) = Shortcut::from_event(key, self.settings_mods) {
				if let Some(conflict) = self.settings_editor.shortcuts.conflict(action, &shortcut) {
					self.settings_editor.status =
						format!("That shortcut is already assigned to {}.", conflict.label());
				} else {
					self.settings_editor.shortcuts.set(action, shortcut);
					self.settings_editor.recording_shortcut = None;
					self.settings_editor.status = "Shortcut changed. Click Save to apply.".into();
				}
			} else {
				self.settings_editor.status =
					"Use a character or supported navigation key with at least one modifier.".into();
			}
			return self.redraw_settings();
		}
		let is_save = (self.settings_mods.ctrl || self.settings_mods.super_key)
			&& matches!(key, Key::Character(s) if s.eq_ignore_ascii_case("s"));
		match key {
			_ if is_save => return self.save_settings_config(),
			Key::Named(NamedKey::Tab) => {
				// pr-check: window glue: steps the tested focus order by Tab's direction
				let step = if self.settings_mods.shift { -1 } else { 1 };
				let count = self.settings_focus_rects().len();
				self.settings_focus = step_focus(self.settings_focus, count, step);
				// pr-check: end window glue
			}
			Key::Named(NamedKey::Enter | NamedKey::Space) if self.settings_focus.is_some() => {
				// pr-check: window glue: Enter or Space clicks the center of the focused control
				let focused = self
					.settings_focus
					.and_then(|at| self.settings_focus_rects().get(at).copied());
				if let Some(rect) = focused {
					self.settings_press(center(rect));
				}
				// pr-check: end window glue
			}
			Key::Named(NamedKey::Enter) => return self.save_settings_config(),
			Key::Named(NamedKey::Escape) if self.settings_editor.page != SettingsPage::Main => {
				self.settings_editor.page = SettingsPage::Main
			}
			Key::Named(NamedKey::Escape) => return self.close_settings_window(),
			Key::Named(key @ (NamedKey::ArrowUp | NamedKey::ArrowDown))
				if self.settings_editor.page == SettingsPage::Fonts =>
			{
				let visible = font_list_rows(self.settings_height as usize, self.settings_scale());
				let step = if *key == NamedKey::ArrowUp { -1 } else { 1 };
				self.settings_editor.fonts.step(step, visible);
				self.settings_editor.status = "Font selected. Click Save to apply.".into();
			}
			Key::Named(NamedKey::ArrowLeft) => {
				self.settings_editor.theme = step_theme(self.settings_editor.theme, -1)
			}
			Key::Named(NamedKey::ArrowRight) => {
				self.settings_editor.theme = step_theme(self.settings_editor.theme, 1)
			}
			_ => {}
		}
		self.redraw_settings();
	}
}
