//! Application state and the winit event loop.
mod canvas;
mod draw;
mod draw_tabs;
mod draw_vertical_tabs;
mod frame_pacer;
mod handler;
mod keyboard;
mod mouse;
mod mouse_tabs;
mod notifications;
mod output;
mod renderer;
mod repaint;
mod search;
mod settings;
mod settings_fonts;
mod settings_hit;
mod settings_keys;
mod settings_pen;
mod settings_view;
mod tabs;

use portable_pty::PtySize;
use std::{error::Error, num::NonZeroU32, path::PathBuf, sync::Arc};

use winit::{
	event::{ElementState, MouseButton, WindowEvent},
	event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
	keyboard::{Key, NamedKey},
	window::Window,
};

use crate::{
	clipboard::*,
	config::{
		self, Preferences, config_path, quote_config_string, read_config_value, save_config_theme,
	},
	input::{
		keys::Modifiers, mouse::MouseButtonCode, scroll::ScrollAccumulator, shortcuts::Shortcuts,
	},
	pty::{Seen, Session, UserEvent, spawn_session},
	render::{blend::blend, glyphs::FontSet},
	session_store::restored_session_dirs,
	term::{Frame, Term, TermGuard},
	theme::{Theme, ThemeKind},
	ui::{
		contrast::{selection_colors, tab_text_colors},
		layout::{Layout, layout},
		scale::{chrome, chrome_factor, chrome_scale, font_px},
		search::find_matches,
		selection::{CellPoint, GridGeometry, Selection},
		tabs::{
			TabLayout, TabStripOptions, VerticalTabLayout, VerticalTabOptions, compact_sidebar_width,
			sidebar_width, tab_layout_with_compaction, vertical_tab_layout,
		},
		zoom::{zoom_action, zoomed},
	},
	wallpaper::Wallpaper,
};

pub(crate) use settings::SettingsEditor;

pub(crate) struct App {
	theme: Theme,
	preferences: Preferences,
	/// The terminal font size in points: Settings' size, changed by zoom.
	font_points: crate::ui::font_size::FontSize,
	/// What the vertical tab sidebar last drew, for partial redraws.
	sidebar_shown: Option<draw_vertical_tabs::SidebarView>,
	wallpaper: Option<Wallpaper>,
	wallpaper_strength: usize,
	fonts: FontSet,
	px: f32,
	cw: usize,
	ch: usize,
	ascent: i32,
	margin: usize,
	top: usize,
	titlebar_height: usize,
	tab_height: usize,
	status_height: usize,
	border: usize,

	sessions: Vec<Session>,
	active: usize,
	tab_scroll: usize,
	next_session_id: u64,
	proxy: EventLoopProxy<UserEvent>,

	window: Option<Arc<Window>>,
	reported_title: String,
	renderer: Option<renderer::Renderer>,
	settings_window: Option<Arc<Window>>,
	_settings_context: Option<softbuffer::Context<Arc<Window>>>,
	settings_surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
	settings_width: u32,
	settings_height: u32,
	settings_cursor_pos: (f64, f64),
	settings_editor: SettingsEditor,
	width: u32,
	height: u32,
	mods: Modifiers,
	/// Modifiers held in the Settings window, which has its own focus.
	settings_mods: Modifiers,
	shortcuts: Shortcuts,
	cursor_pos: (f64, f64),
	held_button: Option<MouseButtonCode>,
	/// `TERM` and terminfo path for new shells.
	term_env: crate::terminfo::TermEnv,
	scroll: ScrollAccumulator,
	/// Whether the main window has keyboard focus.
	focused: bool,
	/// Whether the main window is hidden from view.
	occluded: bool,
	notifications: notifications::Notifications,
	/// Hyperlink target under the pointer, shown in the status bar.
	hovered_link: Option<String>,
	/// The pointer shape last set on the window.
	pointer_icon: winit::window::CursorIcon,
	pacer: frame_pacer::FramePacer,
	/// The active terminal's visible screen, copied for rendering.
	frame: Frame,
	/// Back buffer holding the last frame, for partial redraws.
	canvas: canvas::Canvas,
	/// `POOPTERM_FULL_REDRAW` set: never redraw partially (for visual checks).
	full_redraw_only: bool,
	/// In lines, which count evicted history, so it stays on its text.
	selection: Option<Selection>,
	/// Where the selection being dragged with the mouse started, in lines,
	/// and whether it is a block.
	drag_anchor: Option<(CellPoint, bool)>,
	search_active: bool,
	search_dirty: bool,
	search_query: String,
	search_matches: Vec<Selection>,
	search_match_index: usize,
	command_palette: bool,
	command_query: String,
	renaming_tab: bool,
}

impl App {
	/// Locks the active terminal. Drop the guard before locking again.
	pub(super) fn active_term(&self) -> TermGuard<'_> {
		self.sessions[self.active].term()
	}

	pub(super) fn active_session(&self) -> &Session {
		&self.sessions[self.active]
	}

	pub(super) fn cycle_theme(&mut self) {
		self.apply_theme(self.theme.kind.next());
	}

	pub(super) fn apply_theme(&mut self, kind: ThemeKind) {
		if let Err(error) = save_config_theme(kind) {
			eprintln!("poopterm: could not save theme config: {error}");
		}
		self.theme = Theme::load();
		for session in &self.sessions {
			session.term().set_theme(self.theme);
		}
		self.canvas.invalidate();
		if let Some(window) = &self.window {
			window.request_redraw();
		}
	}

	pub(super) fn request_redraw(&self) {
		if let Some(window) = &self.window {
			window.request_redraw();
		}
	}

	/// Sets the terminal font size in physical pixels and recomputes cell metrics.
	pub(super) fn set_font_px(&mut self, px: f32) {
		self.px = px;
		let font = self.fonts.primary();
		if let Some(metrics) = font.horizontal_line_metrics(px) {
			self.ascent = metrics.ascent.round() as i32;
			self.ch = metrics.new_line_size.ceil() as usize;
		}
		self.cw = font.metrics('M', px).advance_width.ceil() as usize;
		self.fonts.clear();
	}

	/// Sizes the font and the window chrome for the window's scale factor:
	/// when it opens, when the factor changes, and after settings changes.
	pub(super) fn apply_scale(&mut self) {
		let factor = self.factor();
		// pr-check: window glue: copies the sizes ui::scale computed into App
		let chrome = chrome(self.chrome_factor(), cfg!(target_os = "macos"));
		(self.border, self.titlebar_height, self.tab_height) =
			(chrome.border, chrome.titlebar_height, chrome.tab_height);
		self.set_font_px(font_px(self.font_points, factor));
		let layout = self.layout();
		(self.margin, self.status_height, self.top) = (layout.margin, layout.status_height, layout.top);
		// pr-check: end window glue
	}

	/// The grid's place in the window with the current chrome and settings.
	pub(super) fn layout(&self) -> Layout {
		let chrome = chrome(self.chrome_factor(), cfg!(target_os = "macos"));
		layout(
			&chrome,
			self.scale(),
			&self.preferences,
			self.sidebar_width(),
		)
	}

	/// Width of the vertical tab sidebar, or 0 when it is not shown.
	pub(super) fn sidebar_width(&self) -> usize {
		if self.preferences.show_tabs && self.preferences.vertical_tabs {
			if self.preferences.compact_tabs {
				compact_sidebar_width(self.scale())
			} else {
				sidebar_width(self.scale())
			}
		} else {
			0
		}
	}

	pub(super) fn grid_left(&self) -> usize {
		self.margin + self.sidebar_width()
	}

	/// Tells a terminal about the window showing it and the user's settings.
	pub(super) fn describe_window(&self, term: &mut Term) {
		// pr-check: window glue: copies the window's cell size and settings into a terminal
		term.cell_width = self.cw;
		term.cell_height = self.ch;
		term.scale = self.factor() as f32;
		term.reduce_motion = self.preferences.reduce_motion;
		// pr-check: end window glue
	}

	pub(super) fn resize(&mut self, w: u32, h: u32) {
		// Also reached after font, wallpaper and settings changes.
		self.canvas.invalidate();
		self.width = w.max(1);
		self.height = h.max(1);
		self.ensure_active_tab_visible();

		let window = (self.width as usize, self.height as usize);
		let (cols, rows) = self.layout().grid_size(window, (self.cw, self.ch));

		for session in &self.sessions {
			let mut term = session.term();
			self.describe_window(&mut term);
			if cols != term.cols || rows != term.rows {
				term.resize(cols, rows);
				let _ = session.master.resize(PtySize {
					rows: rows as u16,
					cols: cols as u16,
					pixel_width: 0,
					pixel_height: 0,
				});
			}
		}

		if let Some(renderer) = self.renderer.as_mut() {
			renderer.resize(self.width, self.height);
		}
		if let Some(win) = &self.window {
			win.request_redraw();
		}
	}
}

/// Opens the main window and runs the event loop until the last tab closes.
pub(crate) fn run() -> Result<(), Box<dyn Error>> {
	let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
	let proxy = event_loop.create_proxy();

	let theme = Theme::load();
	let settings_editor = SettingsEditor::load();
	let preferences = settings_editor.preferences;
	let wallpaper_strength = settings_editor.wallpaper_strength;
	let wallpaper = read_config_value("wallpaper")
		.filter(|path| !path.is_empty())
		.and_then(|path| match Wallpaper::load(&PathBuf::from(path)) {
			Ok(wallpaper) => Some(wallpaper),
			Err(error) => {
				eprintln!("poopterm: wallpaper ignored: {error}");
				None
			}
		});
	let mut term_env = crate::terminfo::resolve(read_config_value("term").as_deref());
	term_env.extra = crate::shell_integration::inject::prepare(
		&std::env::var("SHELL").unwrap_or_default(),
		read_config_value("shell_integration").as_deref(),
	);
	let mut sessions = Vec::new();
	let (initial_cols, initial_rows) = (100, 30);
	for (index, cwd) in restored_session_dirs().into_iter().enumerate() {
		sessions.push(spawn_session(
			index as u64 + 1,
			initial_cols,
			initial_rows,
			theme,
			proxy.clone(),
			cwd,
			&term_env,
		)?);
	}
	let next_session_id = sessions.len() as u64 + 1;
	let mut app = App {
		theme,
		font_points: preferences.font_size,
		preferences,
		sidebar_shown: None,
		wallpaper,
		wallpaper_strength,
		fonts: FontSet::load(),
		px: 18.0,
		cw: 10,
		ch: 20,
		ascent: 15,
		margin: 8,
		top: 8,
		titlebar_height: 0,
		tab_height: 46,
		status_height: 26,
		border: 1,
		sessions,
		active: 0,
		tab_scroll: 0,
		next_session_id,
		proxy,
		window: None,
		reported_title: String::new(),
		renderer: None,
		settings_window: None,
		_settings_context: None,
		settings_surface: None,
		settings_width: 0,
		settings_height: 0,
		settings_cursor_pos: (0.0, 0.0),
		settings_editor,
		width: 0,
		height: 0,
		mods: Modifiers::default(),
		settings_mods: Modifiers::default(),
		shortcuts: Shortcuts::load(),
		cursor_pos: (0.0, 0.0),
		held_button: None,
		term_env,
		scroll: ScrollAccumulator::default(),
		focused: true,
		occluded: false,
		notifications: notifications::Notifications::default(),
		hovered_link: None,
		pointer_icon: winit::window::CursorIcon::Default,
		pacer: frame_pacer::FramePacer::new(std::time::Duration::from_micros(16_667)),
		frame: Frame::default(),
		canvas: canvas::Canvas::new(),
		full_redraw_only: std::env::var_os("POOPTERM_FULL_REDRAW").is_some(),
		selection: None,
		drag_anchor: None,
		search_active: false,
		search_dirty: false,
		search_query: String::new(),
		search_matches: Vec::new(),
		search_match_index: 0,
		command_palette: false,
		command_query: String::new(),
		renaming_tab: false,
	};

	event_loop.run_app(&mut app)?;
	Ok(())
}
