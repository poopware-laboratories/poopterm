//! The Settings window: editable preferences, saving, and input handling.
use super::{
	settings_fonts::FontPicker,
	settings_hit::{SettingsPage, SettingsTarget, font_list_rows, settings_target},
	settings_view::*,
	*,
};
use crate::input::shortcuts::{ShortcutAction, Shortcuts};
use crate::ui::font_size::{SETTINGS_MAX, SETTINGS_MIN, SETTINGS_STEP};
use winit::{dpi::LogicalSize, event::MouseScrollDelta};

/// Unsaved preference edits shown in the Settings window.
pub(crate) struct SettingsEditor {
	pub(crate) theme: ThemeKind,
	pub(crate) preferences: Preferences,
	pub(crate) wallpaper_path: Option<PathBuf>,
	pub(crate) wallpaper_strength: usize,
	pub(crate) status: String,
	pub(crate) page: SettingsPage,
	pub(crate) recording_shortcut: Option<ShortcutAction>,
	pub(crate) shortcuts: Shortcuts,
	/// Installed families are listed when the Fonts page first opens.
	pub(crate) fonts: FontPicker,
}

impl SettingsEditor {
	pub(super) fn load() -> Self {
		Self {
			theme: Theme::load().kind,
			preferences: Preferences::load(),
			wallpaper_path: read_config_value("wallpaper")
				.filter(|path| !path.is_empty())
				.map(PathBuf::from),
			wallpaper_strength: read_config_value("wallpaper_strength")
				.and_then(|value| value.parse().ok())
				.unwrap_or(35)
				.clamp(0, 100),
			status: "Change preferences, then click Save.".into(),
			page: SettingsPage::Main,
			recording_shortcut: None,
			shortcuts: Shortcuts::load(),
			fonts: FontPicker::new(
				Vec::new(),
				&read_config_value("font_family").unwrap_or_default(),
			),
		}
	}

	/// Shows `page`, listing the installed fonts the first time the Fonts
	/// page opens.
	pub(super) fn open(&mut self, page: SettingsPage) {
		self.page = page;
		self.recording_shortcut = None;
		if page == SettingsPage::Fonts && self.fonts.families.is_empty() {
			self.fonts.families = crate::fonts::system_fonts().monospace_families();
			if std::env::var_os("POOPTERM_FONT").is_some()
				|| read_config_value("font_path").is_some_and(|path| !path.trim().is_empty())
			{
				self.status = "POOPTERM_FONT or font_path overrides the font chosen here.".into();
			}
		}
	}

	/// Applies a `−` (`step < 0`) or `+` click on the stepper in `row`.
	fn step(&mut self, row: usize, step: isize) {
		let up = step > 0;
		let prefs = &mut self.preferences;
		match row {
			FONT_ROW => {
				prefs.font_size = prefs
					.font_size
					.stepped(SETTINGS_STEP, up, SETTINGS_MIN, SETTINGS_MAX)
			}
			SCROLLBACK_ROW => {
				prefs.scrollback_lines = if up {
					(prefs.scrollback_lines + 1000).min(100_000)
				} else {
					prefs.scrollback_lines.saturating_sub(1000)
				}
			}
			STRENGTH_ROW => {
				self.wallpaper_strength = if up {
					(self.wallpaper_strength + 5).min(100)
				} else {
					self.wallpaper_strength.saturating_sub(5)
				}
			}
			PADDING_ROW => {
				prefs.padding = if up {
					(prefs.padding + 1).min(32)
				} else {
					prefs.padding.saturating_sub(1)
				}
			}
			OPACITY_ROW => {
				prefs.opacity = if up {
					(prefs.opacity + 5).min(100)
				} else {
					prefs.opacity.saturating_sub(5).max(60)
				}
			}
			_ => {}
		}
	}
}

impl App {
	pub(super) fn open_settings_window(&mut self, event_loop: &ActiveEventLoop) {
		if let Some(window) = &self.settings_window {
			window.focus_window();
			return;
		}
		self.settings_editor = SettingsEditor::load();
		let attributes = crate::platform::identify(Window::default_attributes())
			.with_title("Poopterm Settings")
			.with_inner_size(LogicalSize::new(
				DEFAULT_SIZE.0 as f64,
				DEFAULT_SIZE.1 as f64,
			))
			.with_min_inner_size(LogicalSize::new(MIN_SIZE.0 as f64, MIN_SIZE.1 as f64));
		let window = Arc::new(
			event_loop
				.create_window(attributes)
				.expect("failed to create settings window"),
		);
		let context = softbuffer::Context::new(window.clone()).expect("settings context");
		let mut surface = softbuffer::Surface::new(&context, window.clone()).expect("settings surface");
		let size = window.inner_size();
		if let (Some(width), Some(height)) = (
			NonZeroU32::new(size.width.max(1)),
			NonZeroU32::new(size.height.max(1)),
		) {
			let _ = surface.resize(width, height);
		}
		self.settings_width = size.width;
		self.settings_height = size.height;
		self.settings_surface = Some(surface);
		self._settings_context = Some(context);
		self.settings_window = Some(window.clone());
		window.request_redraw();
	}

	pub(super) fn close_settings_window(&mut self) {
		self.settings_window = None;
		self.settings_surface = None;
		self._settings_context = None;
	}

	pub(super) fn redraw_settings(&self) {
		if let Some(window) = &self.settings_window {
			window.request_redraw();
		}
	}

	pub(super) fn save_settings_config(&mut self) {
		let editor = &self.settings_editor;
		let preferences = editor.preferences;
		let wallpaper_path = editor.wallpaper_path.as_ref().map_or_else(
			|| "\"\"".to_owned(),
			|path| quote_config_string(&path.to_string_lossy()),
		);
		let mut values = vec![
			("theme", format!("\"{}\"", editor.theme.name())),
			("font_size", preferences.font_size.to_string()),
			("scrollback_lines", preferences.scrollback_lines.to_string()),
			("restore_sessions", preferences.restore_sessions.to_string()),
			("opacity", preferences.opacity.to_string()),
			("show_tabs", preferences.show_tabs.to_string()),
			("vertical_tabs", preferences.vertical_tabs.to_string()),
			("compact_tabs", preferences.compact_tabs.to_string()),
			(
				"show_new_tab_button",
				preferences.show_new_tab_button.to_string(),
			),
			(
				"show_settings_button",
				preferences.show_settings_button.to_string(),
			),
			("show_bottom_bar", preferences.show_bottom_bar.to_string()),
			("padding", preferences.padding.to_string()),
			("wallpaper", wallpaper_path),
			("wallpaper_strength", editor.wallpaper_strength.to_string()),
			("font_family", quote_config_string(&editor.fonts.chosen)),
		];
		let font_changed =
			read_config_value("font_family").unwrap_or_default().trim() != editor.fonts.chosen;
		for action in ShortcutAction::ALL {
			values.push((
				action.config_key(),
				quote_config_string(&editor.shortcuts.get(action).binding()),
			));
		}
		let result = config::save(&config_path(), &values);
		if let Err(error) = result {
			self.settings_editor.status = format!("Could not save: {error}");
		} else {
			self.apply_saved_settings(preferences, font_changed);
		}
		self.redraw_settings();
		self.request_redraw();
	}

	fn apply_saved_settings(&mut self, preferences: Preferences, font_changed: bool) {
		if preferences.font_size != self.preferences.font_size {
			self.font_points = preferences.font_size;
		}
		self.preferences = preferences;
		self.theme = Theme::load();
		self.shortcuts = Shortcuts::load();
		self.wallpaper_strength = self.settings_editor.wallpaper_strength;
		self.wallpaper = None;
		let mut status = "Saved. Settings applied.".to_owned();
		if let Some(path) = &self.settings_editor.wallpaper_path {
			match Wallpaper::load(path) {
				Ok(wallpaper) => self.wallpaper = Some(wallpaper),
				Err(error) => status = format!("Wallpaper could not be loaded: {error}"),
			}
		}
		#[cfg(target_os = "macos")]
		if let Some(window) = &self.window {
			crate::platform::set_window_opacity(window, preferences.opacity);
		}
		for session in &self.sessions {
			let mut term = session.term();
			term.set_theme(self.theme);
			term.set_scrollback_limit(preferences.scrollback_lines);
		}
		if font_changed {
			self.fonts = crate::render::glyphs::FontSet::load();
		}
		self.apply_scale();
		self.selection = None;
		self.end_search();
		self.resize(self.width, self.height);
		self.ensure_active_tab_visible();
		let page = self.settings_editor.page;
		let fonts = std::mem::take(&mut self.settings_editor.fonts);
		self.settings_editor = SettingsEditor::load();
		self.settings_editor.page = page;
		self.settings_editor.fonts = fonts;
		self.settings_editor.status = status;
	}

	pub(super) fn settings_window_event(&mut self, event: WindowEvent) {
		match event {
			WindowEvent::CloseRequested => self.close_settings_window(),
			WindowEvent::Resized(size) => {
				self.settings_width = size.width.max(1);
				self.settings_height = size.height.max(1);
				if let Some(surface) = self.settings_surface.as_mut()
					&& let (Some(width), Some(height)) = (
						NonZeroU32::new(self.settings_width),
						NonZeroU32::new(self.settings_height),
					) {
					let _ = surface.resize(width, height);
				}
				self.draw_settings();
			}
			WindowEvent::CursorMoved { position, .. } => {
				self.settings_cursor_pos = (position.x, position.y);
				self.redraw_settings();
			}
			WindowEvent::MouseInput {
				state: ElementState::Pressed,
				button: MouseButton::Left,
				..
			} => self.settings_click(),
			WindowEvent::MouseWheel { delta, .. } if self.settings_editor.page == SettingsPage::Fonts => {
				let scale = self.settings_scale();
				let lines = match delta {
					MouseScrollDelta::LineDelta(_, y) => -y.round() as isize,
					MouseScrollDelta::PixelDelta(position) => {
						-(position.y / (FONT_LIST_STRIDE * scale) as f64).round() as isize
					}
				};
				let visible = font_list_rows(self.settings_height as usize, scale);
				self.settings_editor.fonts.scroll_by(lines, visible);
				self.redraw_settings();
			}
			WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
				self.settings_key(&event)
			}
			WindowEvent::ModifiersChanged(modifiers) => {
				let state = modifiers.state();
				self.settings_mods = Modifiers {
					ctrl: state.control_key(),
					shift: state.shift_key(),
					alt: state.alt_key(),
					super_key: state.super_key(),
				};
			}
			WindowEvent::RedrawRequested => self.draw_settings(),
			_ => {}
		}
	}

	pub(super) fn settings_scale(&self) -> usize {
		self
			.settings_window
			.as_ref()
			.map_or(1, |window| window.scale_factor().round() as usize)
			.max(1)
	}

	fn settings_click(&mut self) {
		let scale = self.settings_scale();
		let point = (
			self.settings_cursor_pos.0.max(0.0) as usize,
			self.settings_cursor_pos.1.max(0.0) as usize,
		);
		let size = (self.settings_width as usize, self.settings_height as usize);
		let editor = &mut self.settings_editor;
		match settings_target(point, size, scale, editor.page) {
			Some(SettingsTarget::Open(page)) => editor.open(page),
			Some(SettingsTarget::FontRow(row)) => {
				if let Some(entry) = editor.fonts.entry_at(row) {
					editor.fonts.select(entry);
					editor.status = "Font selected. Click Save to apply.".into();
				}
			}
			Some(SettingsTarget::Back) => {
				editor.page = SettingsPage::Main;
				editor.recording_shortcut = None;
			}
			Some(SettingsTarget::SetShortcut(action)) => {
				editor.recording_shortcut = Some(action);
				editor.status = format!("Press a key chord for {}. Esc cancels.", action.label());
			}
			Some(SettingsTarget::Save) => return self.save_settings_config(),
			Some(SettingsTarget::ChooseWallpaper) => self.choose_wallpaper(),
			Some(SettingsTarget::ClearWallpaper) => {
				editor.wallpaper_path = None;
				editor.status = "Wallpaper cleared. Click Save to apply.".into();
			}
			Some(SettingsTarget::ToggleRestore) => {
				editor.preferences.restore_sessions = !editor.preferences.restore_sessions
			}
			Some(SettingsTarget::ToggleTabs) => {
				editor.preferences.show_tabs = !editor.preferences.show_tabs
			}
			Some(SettingsTarget::ToggleVerticalTabs) => {
				editor.preferences.vertical_tabs = !editor.preferences.vertical_tabs
			}
			Some(SettingsTarget::ToggleCompactTabs) => {
				editor.preferences.compact_tabs = !editor.preferences.compact_tabs
			}
			Some(SettingsTarget::ToggleNewTabButton) => {
				editor.preferences.show_new_tab_button = !editor.preferences.show_new_tab_button
			}
			Some(SettingsTarget::ToggleSettingsButton) => {
				editor.preferences.show_settings_button = !editor.preferences.show_settings_button
			}
			Some(SettingsTarget::ToggleBottomBar) => {
				editor.preferences.show_bottom_bar = !editor.preferences.show_bottom_bar
			}
			Some(SettingsTarget::Step(row, step)) => editor.step(row, step),
			Some(SettingsTarget::Theme(step)) => editor.theme = step_theme(editor.theme, step),
			None => {}
		}
		self.redraw_settings();
	}

	fn choose_wallpaper(&mut self) {
		let picked = self.settings_window.as_ref().and_then(|window| {
			rfd::FileDialog::new()
				.set_title("Choose a terminal wallpaper")
				.add_filter("Wallpaper images", &["png", "jpg", "jpeg", "webp"])
				.set_parent(window.as_ref())
				.pick_file()
		});
		let Some(path) = picked else {
			return;
		};
		let editor = &mut self.settings_editor;
		match Wallpaper::load(&path) {
			Ok(_) => {
				editor.wallpaper_path = Some(path);
				editor.status = "Wallpaper selected. Click Save to apply.".into();
			}
			Err(error) => editor.status = format!("Could not use wallpaper: {error}"),
		}
	}
}
