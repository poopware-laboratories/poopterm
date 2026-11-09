//! Drawing the Settings window. Row positions are logical pixels, multiplied by
//! the window scale; `settings_hit.rs` hit-tests clicks against the same
//! constants.
use super::*;
use super::{
	settings_diagnostics::compose_diagnostics,
	settings_hit::{SettingsPage, font_list_rows},
	settings_nav::{NAV_WIDTH, compose_nav},
	settings_pen::Pen,
};
use crate::input::shortcuts::ShortcutAction;
use crate::render::{painter::ButtonStyle, scene::Scene};
use crate::ui::{
	about::{Tone, about_lines},
	chrome_colors::chrome_colors,
	settings_layout::{MAIN, MIN_HEIGHT},
};

pub(super) const THEME_ROW: usize = MAIN.theme;
/// The picked theme's credit, between the picker and the wallpaper label.
pub(super) const THEME_CREDIT_ROW: usize = MAIN.credit;
pub(super) const WALLPAPER_ROW: usize = MAIN.wallpaper;
pub(super) const FONT_ROW: usize = MAIN.font;
pub(super) const SCROLLBACK_ROW: usize = MAIN.scrollback;
pub(super) const RESTORE_ROW: usize = MAIN.restore;
pub(super) const STRENGTH_ROW: usize = MAIN.strength;
pub(super) const OPACITY_ROW: usize = MAIN.opacity;
pub(super) const SHOW_TABS_ROW: usize = 104;
pub(super) const VERTICAL_TABS_ROW: usize = 152;
pub(super) const NEW_TAB_BUTTON_ROW: usize = 200;
pub(super) const SETTINGS_BUTTON_ROW: usize = 248;
pub(super) const BOTTOM_BAR_ROW: usize = 296;
pub(super) const COMPACT_TABS_ROW: usize = 344;
pub(super) const PADDING_ROW: usize = 392;
pub(super) const FRAME_ROW: usize = 440;
/// Shortcut rows sit closer than other pages' rows so all ten clear the
/// footer at the smallest window size.
pub(super) const SHORTCUT_ROWS: [(ShortcutAction, usize); 10] = {
	let mut rows = [(ShortcutAction::NewTab, 0); 10];
	let mut i = 0;
	while i < rows.len() {
		rows[i] = (ShortcutAction::ALL[i], 104 + 44 * i);
		i += 1;
	}
	rows
};
/// Top of the Fonts page's list and the distance between its rows.
pub(super) const FONT_LIST_TOP: usize = 104;
pub(super) const FONT_LIST_STRIDE: usize = 44;
/// Default and minimum Settings window size, in logical pixels.
pub(super) const DEFAULT_SIZE: (usize, usize) = (760, 640);
pub(super) const MIN_SIZE: (usize, usize) = (680, MIN_HEIGHT);
/// Height of a settings row's controls.
pub(super) const ROW_HEIGHT: usize = 40;

/// Themes in the order the Settings picker cycles through them.
pub(super) const THEMES: [ThemeKind; 18] = [
	ThemeKind::MacOsDark,
	ThemeKind::MacOsLight,
	ThemeKind::Nord,
	ThemeKind::RosePine,
	ThemeKind::RosePineMoon,
	ThemeKind::RosePineDawn,
	ThemeKind::CatppuccinLatte,
	ThemeKind::CatppuccinFrappe,
	ThemeKind::CatppuccinMacchiato,
	ThemeKind::CatppuccinMocha,
	ThemeKind::Bark,
	ThemeKind::AmoledBlack,
	ThemeKind::Dracula,
	ThemeKind::Alucard,
	ThemeKind::GruvboxDark,
	ThemeKind::GruvboxLight,
	ThemeKind::Poop,
	ThemeKind::Custom,
];

/// Returns the theme `step` positions away from `current` in [`THEMES`].
pub(super) fn step_theme(current: ThemeKind, step: isize) -> ThemeKind {
	let index = THEMES
		.iter()
		.position(|&theme| theme == current)
		.unwrap_or(0);
	THEMES[(index as isize + step).rem_euclid(THEMES.len() as isize) as usize]
}

fn theme_label(kind: ThemeKind) -> String {
	match kind {
		ThemeKind::MacOsDark => "Dark Mode".to_owned(),
		ThemeKind::MacOsLight => "Light Mode".to_owned(),
		ThemeKind::Nord => "Nord".to_owned(),
		ThemeKind::RosePine => "Rosé Pine".to_owned(),
		ThemeKind::RosePineMoon => "Rosé Pine Moon".to_owned(),
		ThemeKind::RosePineDawn => "Rosé Pine Dawn".to_owned(),
		ThemeKind::Bark => "Bark".to_owned(),
		ThemeKind::AmoledBlack => "AMOLED Black".to_owned(),
		ThemeKind::Dracula => "Dracula".to_owned(),
		ThemeKind::Alucard => "Alucard (Dracula Light)".to_owned(),
		ThemeKind::GruvboxDark => "Gruvbox Dark".to_owned(),
		ThemeKind::GruvboxLight => "Gruvbox Light".to_owned(),
		ThemeKind::Custom => "Custom".to_owned(),
		_ => kind.name().replace("catppuccin-", "Catppuccin "),
	}
}

impl App {
	pub(super) fn draw_settings(&mut self) {
		let scale = self
			.settings_window
			.as_ref()
			.map_or(1, |window| window.scale_factor().round() as usize)
			.max(1);
		let Some(renderer) = self.settings_surface.take() else {
			return;
		};
		let (width, height) = (self.settings_width as usize, self.settings_height as usize);
		// pr-check: window glue: draws the settings window's frame, which needs its window
		let mut scene = Scene::new(width, height);
		self.compose_settings(&mut scene, scale);
		self.settings_surface = self.show_settings(renderer, &scene);
		// pr-check: end window glue
	}

	fn compose_settings(&mut self, p: &mut Scene, scale: usize) {
		let (width, height) = (p.width, p.height);
		p.fill(self.theme.border);
		p.rect(
			2 * scale,
			2 * scale,
			width.saturating_sub(4 * scale),
			height.saturating_sub(4 * scale),
			self.theme.bg,
		);
		// The page is drawn as if it stood alone, then moved right of the sidebar.
		let nav = NAV_WIDTH * scale;
		let mut page = Scene::new(width.saturating_sub(nav), height);
		self.compose_page(&mut page, scale);
		self.compose_sidebar(p, scale);
		p.commands.extend(page.shifted(nav).commands);
	}

	fn compose_sidebar(&mut self, p: &mut Scene, scale: usize) {
		let px = 18.0 * scale as f32;
		let ascent = (self.ui_fonts.primary())
			.horizontal_line_metrics(px)
			.map_or(self.ascent, |metrics| metrics.ascent.round() as i32);
		let colors = chrome_colors(&self.theme);
		let mut pen = Pen {
			fonts: &mut self.ui_fonts,
			px,
			ascent,
			color: self.theme.fg,
			scale,
		};
		let (active, cursor) = (self.settings_editor.page, self.settings_cursor_pos);
		compose_nav(&mut pen, p, (active, cursor), (&colors, self.theme.fg));
	}

	fn compose_page(&mut self, p: &mut Scene, scale: usize) {
		let px = 18.0 * scale as f32;
		let ascent = self
			.ui_fonts
			.primary()
			.horizontal_line_metrics(px)
			.map_or(self.ascent, |metrics| metrics.ascent.round() as i32);
		let style = self.button_style(scale);
		// The page's own origin is the sidebar's right edge.
		let cursor = (
			self.settings_cursor_pos.0 - (NAV_WIDTH * scale) as f64,
			self.settings_cursor_pos.1,
		);
		let hovered = |x: usize, y: usize, w: usize, h: usize| {
			cursor.0 >= x as f64
				&& cursor.0 < (x + w) as f64
				&& cursor.1 >= y as f64
				&& cursor.1 < (y + h) as f64
		};
		let diagnostics =
			(self.settings_editor.page == SettingsPage::Diagnostics).then(|| self.diagnostics());
		let editor = &self.settings_editor;
		let colors = crate::ui::diagnostics_colors::diagnostics_colors(&self.theme);
		let mut pen = Pen {
			fonts: &mut self.ui_fonts,
			px,
			ascent,
			color: self.theme.fg,
			scale,
		};
		if let Some(diagnostics) = diagnostics {
			let (section, status) = (editor.diagnostics_section, &editor.copied);
			return compose_diagnostics(
				&diagnostics,
				section,
				status,
				&colors,
				style,
				p,
				&mut pen,
				&hovered,
			);
		}
		let x = 24 * scale;
		match editor.page {
			SettingsPage::About => {
				let colors = (self.theme.fg, chrome_colors(&self.theme).muted);
				compose_about(&mut pen, p, colors);
				return;
			}
			SettingsPage::Shortcuts => {
				pen.text(p, x, 28, "Keyboard shortcuts", 1.0);
				pen.text(
					p,
					x,
					68,
					"Click a binding, then press the new key chord. Esc cancels.",
					0.72,
				);
				for (action, row) in SHORTCUT_ROWS {
					let recording = editor.recording_shortcut == Some(action);
					let label = if recording {
						"Press keys…".to_owned()
					} else {
						editor
							.shortcuts
							.get(action)
							.display(cfg!(target_os = "macos"))
					};
					pen.text(p, x, row + 10, action.label(), 0.78);
					pen.wide_button(p, style, row, &label, recording, &hovered);
				}
			}
			SettingsPage::Layout => compose_layout_settings(editor, style, p, &mut pen),
			SettingsPage::Fonts => {
				pen.text(p, x, 28, "Fonts", 1.0);
				let visible = font_list_rows(p.height, scale);
				let fonts = &editor.fonts;
				let hint = if fonts.len() > visible {
					"Monospace fonts on this system. Scroll or use ↑ ↓ for more."
				} else {
					"Monospace fonts installed on this system."
				};
				pen.text(p, x, 68, hint, 0.72);
				let selected = fonts.selected();
				for row in 0..visible {
					let Some(entry) = fonts.entry_at(row) else {
						break;
					};
					let top = FONT_LIST_TOP + row * FONT_LIST_STRIDE;
					let rect = (
						x,
						top * scale,
						p.width.saturating_sub(48 * scale),
						ROW_HEIGHT * scale,
					);
					let lit = selected == Some(entry) || hovered(rect.0, rect.1, rect.2, rect.3);
					p.button(rect, style, lit);
					pen.text(p, x + 16 * scale, top + 9, fonts.label(entry), 0.82);
				}
			}
			SettingsPage::Main => compose_main_settings(editor, style, p, &mut pen),
			// Drawn above, with its own footer.
			SettingsPage::Diagnostics => return,
		}
		pen.footer(p, style, ("Save", 120), &editor.status, &hovered);
	}
}

/// The About page: its lines centered, in `(strong, muted)` colors.
pub(super) fn compose_about(pen: &mut Pen, p: &mut Scene, (strong, muted): (u32, u32)) {
	let version = (env!("CARGO_PKG_VERSION"), env!("POOPTERM_CODENAME"));
	for line in about_lines(version.0, version.1) {
		let color = if line.tone == Tone::Strong {
			strong
		} else {
			muted
		};
		pen.centered_row(p, (line.row, line.size, color), &line.text);
	}
}

/// The Layout page: tab position, switches for the bars and buttons, and
/// the terminal's padding.
pub(super) fn compose_layout_settings(
	editor: &SettingsEditor,
	style: ButtonStyle,
	p: &mut Scene,
	pen: &mut Pen,
) {
	let x = 24 * pen.scale;
	pen.text(p, x, 28, "Layout", 1.0);
	pen.text(
		p,
		x,
		68,
		"Tabs, toolbar buttons, the bottom bar, and the window frame.",
		0.72,
	);
	let prefs = editor.preferences;
	for (row, label, enabled) in [
		(SHOW_TABS_ROW, "Show tabs", prefs.show_tabs),
		(
			NEW_TAB_BUTTON_ROW,
			"Show new tab button",
			prefs.show_new_tab_button,
		),
		(
			SETTINGS_BUTTON_ROW,
			"Show settings button",
			prefs.show_settings_button,
		),
		(BOTTOM_BAR_ROW, "Show bottom bar", prefs.show_bottom_bar),
		(COMPACT_TABS_ROW, "Compact tabs", prefs.compact_tabs),
		(
			FRAME_ROW,
			"Square window (no title bar)",
			prefs.window_frame.is_frameless(),
		),
	] {
		pen.text(p, x, row + 8, label, 0.82);
		pen.switch(p, style, row, enabled);
	}
	pen.text(p, x, VERTICAL_TABS_ROW + 8, "Tab position", 0.82);
	let labels = ["Top", "Side"];
	pen.segmented(
		p,
		style,
		VERTICAL_TABS_ROW,
		&labels,
		usize::from(prefs.vertical_tabs),
	);
	pen.text(p, x, PADDING_ROW + 8, "Padding", 0.82);
	pen.stepper(p, style, PADDING_ROW, &format!("{} px", prefs.padding));
}

/// The main page: theme, wallpaper, steppers and session restore.
pub(super) fn compose_main_settings(
	editor: &SettingsEditor,
	style: ButtonStyle,
	p: &mut Scene,
	pen: &mut Pen,
) {
	let (width, s) = (p.width, pen.scale);
	let x = 24 * s;
	let content_w = width.saturating_sub(48 * s);
	pen.text(p, x, 24, "Settings", 1.0);
	pen.text(p, x, MAIN.heading, "Appearance", 0.82);
	pen.text(p, x, MAIN.theme_label, "Color theme", 0.76);

	let button_w = 48 * s;
	let theme_y = THEME_ROW * s;
	for (bx, label) in [(x, "‹"), (x + content_w - button_w, "›")] {
		p.button((bx, theme_y, button_w, ROW_HEIGHT * s), style, false);
		pen.text(p, bx + 18 * s, THEME_ROW + 7, label, 1.0);
	}
	let theme_x = x + button_w + 8 * s;
	let theme_w = content_w.saturating_sub(2 * button_w + 16 * s);
	p.button((theme_x, theme_y, theme_w, ROW_HEIGHT * s), style, false);
	pen.text(
		p,
		theme_x + 12 * s,
		THEME_ROW + 9,
		&theme_label(editor.theme),
		0.82,
	);
	pen.text(p, x, THEME_CREDIT_ROW, &editor.theme.credit().line(), 0.62);

	pen.text(p, x, MAIN.wallpaper_label, "Background wallpaper", 0.76);
	for (bx, bw, label) in [
		(x, 152 * s, "Choose image…"),
		(x + 164 * s, 96 * s, "Clear"),
	] {
		p.button((bx, WALLPAPER_ROW * s, bw, 32 * s), style, false);
		pen.text(p, bx + 12 * s, WALLPAPER_ROW + 6, label, 0.68);
	}
	let wallpaper_name: String = editor
		.wallpaper_path
		.as_ref()
		.and_then(|path| path.file_name())
		.and_then(|name| name.to_str())
		.unwrap_or("No image selected")
		.chars()
		.take(32)
		.collect();
	pen.text(p, x + 272 * s, WALLPAPER_ROW + 6, &wallpaper_name, 0.66);

	let prefs = editor.preferences;
	let mut steppers = vec![
		(FONT_ROW, "Font size", prefs.font_size.to_string()),
		(
			SCROLLBACK_ROW,
			"Scrollback lines",
			prefs.scrollback_lines.to_string(),
		),
		(
			STRENGTH_ROW,
			"Wallpaper strength",
			format!("{}%", editor.wallpaper_strength),
		),
	];
	if cfg!(target_os = "macos") {
		steppers.push((OPACITY_ROW, "Window opacity", format!("{}%", prefs.opacity)));
	}
	for (row, label, value) in steppers {
		pen.text(p, x, row + 8, label, 0.82);
		pen.stepper(p, style, row, &value);
	}

	pen.text(p, x, RESTORE_ROW + 8, "Restore tabs on launch", 0.82);
	pen.switch(p, style, RESTORE_ROW, prefs.restore_sessions);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_picker_lists_every_theme_once_with_custom_last() {
		let mut kind = ThemeKind::MacOsDark;
		loop {
			let count = THEMES.iter().filter(|&&listed| listed == kind).count();
			assert_eq!(count, 1, "{}", kind.name());
			kind = kind.next();
			if kind == ThemeKind::MacOsDark {
				break;
			}
		}
		assert_eq!(THEMES.last(), Some(&ThemeKind::Custom));
	}

	#[test]
	fn the_picker_offers_the_new_themes_by_name() {
		for (kind, label) in [
			(ThemeKind::Dracula, "Dracula"),
			(ThemeKind::Alucard, "Alucard (Dracula Light)"),
			(ThemeKind::GruvboxDark, "Gruvbox Dark"),
			(ThemeKind::GruvboxLight, "Gruvbox Light"),
		] {
			assert!(THEMES.contains(&kind), "{label}");
			assert_eq!(theme_label(kind), label);
		}
		assert_eq!(step_theme(ThemeKind::AmoledBlack, 1), ThemeKind::Dracula);
		assert_eq!(step_theme(ThemeKind::GruvboxLight, 1), ThemeKind::Poop);
		assert_eq!(step_theme(ThemeKind::Dracula, -1), ThemeKind::AmoledBlack);
	}

	#[test]
	fn the_credit_sits_between_the_picker_and_the_wallpaper_label() {
		// The picker ends at THEME_ROW + ROW_HEIGHT; the credit's small text
		// is about 12 high and ends before the wallpaper label.
		const { assert!(THEME_CREDIT_ROW >= THEME_ROW + ROW_HEIGHT) };
		const { assert!(THEME_CREDIT_ROW + 14 <= MAIN.wallpaper_label) };
	}

	#[test]
	fn every_picked_theme_has_a_credit_line() {
		for kind in THEMES {
			let line = kind.credit().line();
			assert!(line.starts_with("by "), "{line}");
			assert!(line.contains(kind.credit().license), "{line}");
		}
	}

	#[test]
	fn theme_picker_includes_nord_and_every_rose_pine_variant() {
		assert_eq!(THEMES.len(), 18);
		assert_eq!(THEMES[2], ThemeKind::Nord);
		assert_eq!(THEMES[3], ThemeKind::RosePine);
		assert_eq!(THEMES[4], ThemeKind::RosePineMoon);
		assert_eq!(THEMES[5], ThemeKind::RosePineDawn);
		assert_eq!(theme_label(ThemeKind::Nord), "Nord");
		assert_eq!(theme_label(ThemeKind::RosePine), "Rosé Pine");
		assert_eq!(theme_label(ThemeKind::RosePineMoon), "Rosé Pine Moon");
		assert_eq!(theme_label(ThemeKind::RosePineDawn), "Rosé Pine Dawn");
		assert_eq!(step_theme(ThemeKind::MacOsLight, 1), ThemeKind::Nord);
		assert_eq!(step_theme(ThemeKind::Nord, 1), ThemeKind::RosePine);
		assert_eq!(step_theme(ThemeKind::RosePine, 1), ThemeKind::RosePineMoon);
		assert_eq!(
			step_theme(ThemeKind::RosePineMoon, 1),
			ThemeKind::RosePineDawn
		);
		assert_eq!(
			step_theme(ThemeKind::RosePineDawn, 1),
			ThemeKind::CatppuccinLatte
		);
	}

	#[test]
	fn theme_picker_steps_back_and_forth_through_new_variants() {
		assert_eq!(step_theme(ThemeKind::Nord, -1), ThemeKind::MacOsLight);
		assert_eq!(step_theme(ThemeKind::Nord, 1), ThemeKind::RosePine);
		assert_eq!(step_theme(ThemeKind::RosePine, -1), ThemeKind::Nord);
		assert_eq!(step_theme(ThemeKind::RosePine, 1), ThemeKind::RosePineMoon);
		assert_eq!(step_theme(ThemeKind::RosePineMoon, -1), ThemeKind::RosePine);
		assert_eq!(
			step_theme(ThemeKind::RosePineMoon, 1),
			ThemeKind::RosePineDawn
		);
		assert_eq!(
			step_theme(ThemeKind::RosePineDawn, -1),
			ThemeKind::RosePineMoon
		);
		assert_eq!(
			step_theme(ThemeKind::RosePineDawn, 1),
			ThemeKind::CatppuccinLatte
		);
	}

	#[test]
	fn theme_picker_wraps_in_both_directions() {
		assert_eq!(step_theme(ThemeKind::MacOsDark, -1), ThemeKind::Custom);
		assert_eq!(step_theme(ThemeKind::Custom, 1), ThemeKind::MacOsDark);
		assert_eq!(step_theme(ThemeKind::Poop, 1), ThemeKind::Custom);
		assert_eq!(step_theme(ThemeKind::CatppuccinMocha, 1), ThemeKind::Bark);
		assert_eq!(step_theme(ThemeKind::Bark, 1), ThemeKind::AmoledBlack);
	}
}
