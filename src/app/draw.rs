//! Main-window frame composition: background, tab bar, grid, overlays, status bar.
use super::*;
use super::{
	canvas::{DrawKey, Plan, pane_keys},
	renderer::after_shown,
	repaint::keeps_contents,
};
use std::time::{Duration, Instant};

use crate::{
	render::{
		grid::{GridView, draw_grid},
		painter::{ButtonStyle, Painter},
		pen::Pen,
		popover::draw_popover,
		scene::paint,
		status_bar::draw_status_bar,
	},
	ui::{
		overlays::{Place, command_palette, search_box},
		status_bar::{StatusInput, status_bar},
	},
};

/// Toolbar text, which keeps its size regardless of the terminal font.
#[derive(Clone, Copy)]
pub(super) struct UiFont {
	pub px: f32,
}

impl App {
	/// The whole-number scale the window chrome is drawn at.
	pub(super) fn scale(&self) -> usize {
		chrome_scale(self.chrome_factor())
	}

	/// The display factor grown with large fonts, which the chrome follows.
	pub(super) fn chrome_factor(&self) -> f64 {
		chrome_factor(self.factor(), self.font_points)
	}

	/// The window's display scale factor.
	// pr-check: window glue: only a real window has a scale factor
	pub(super) fn factor(&self) -> f64 {
		self.window.as_ref().map_or(1.0, |w| w.scale_factor())
	}
	// pr-check: end window glue

	pub(super) fn ui_font(&self, px: f32) -> UiFont {
		UiFont { px }
	}

	pub(super) fn button_style(&self, scale: usize) -> ButtonStyle {
		ButtonStyle {
			scale,
			background: self.theme.bg,
			border: self.theme.border,
			accent: self.theme.palette[4],
		}
	}

	pub(super) fn draw(&mut self) {
		let started = Instant::now();
		self.pacer.drawn(started);
		// pr-check: window glue: once a frame, not once an output wake (thousands a second in a flood)
		self.update_window_title();
		// pr-check: end window glue
		self.refresh_search_if_dirty();
		self.fonts.trim();
		self.ui_fonts.trim();
		let Some(mut renderer) = self.renderer.take() else {
			return;
		};
		let size = (self.width as usize, self.height as usize);
		self.sessions[self.active].term().snapshot(&mut self.frame);
		let places = self.is_split().then(|| self.tab_places(self.active));
		if let Some(places) = &places {
			self.snapshot_panes(places);
		}
		let key = self.draw_key();
		#[cfg(any(target_os = "linux", target_os = "freebsd"))]
		if let renderer::Renderer::Gpu(gpu) = renderer {
			let showing = Instant::now();
			self.renderer = self.draw_on_gpu(gpu, key, size);
			self.timed(true, showing.elapsed(), started);
			return;
		}
		let plan = match &places {
			Some(places) => {
				let keys = pane_keys(places, key);
				(self.canvas).plan_panes(&self.pane_frames, &keys, places, size)
			}
			None => (self.canvas).plan(&self.frame, key, size, self.wallpaper.is_none()),
		};
		// Debug aid for visual checks: compare against full redraws.
		let plan = if self.full_redraw_only {
			Plan::Full
		} else {
			plan
		};
		let mut pixels = std::mem::take(&mut self.canvas.pixels);
		let mut painter = Painter::new(&mut pixels, size.0, size.1);
		let painting = Instant::now();
		let bands = match &plan {
			Plan::Full => {
				self.compose(&mut painter, None);
				None
			}
			Plan::Partial(damage) => Some(self.compose_damaged(&mut painter, damage)),
			Plan::Panes(rows) => Some(self.compose_panes_damaged(&mut painter, rows)),
		};
		self.painted(painting.elapsed());
		self.canvas.pixels = pixels;
		let damage = match &plan {
			Plan::Full | Plan::Panes(_) => None,
			Plan::Partial(damage) => Some(damage),
		};
		let damage_only = self.window.as_deref().is_some_and(keeps_contents);
		let showing = Instant::now();
		let shown = renderer.present(
			&self.frame,
			damage,
			&self.canvas.pixels,
			size.0,
			bands.as_deref(),
			damage_only,
		);
		self.timed(false, showing.elapsed(), started);
		match places {
			Some(places) => {
				let keys = pane_keys(&places, key);
				(self.canvas).remember_panes(&mut self.pane_frames, keys, places);
			}
			None => self.canvas.remember(&mut self.frame, key),
		}
		let again = || self.schedule_redraw();
		self.renderer = after_shown(renderer, shown, again, renderer::Renderer::after_presenter);
	}

	// pr-check: window glue: hands a real window's frame times to the tested frame_stats::Recorder
	/// Software painted for `took` this frame (the whole of it if `whole`),
	/// for the frame statistics.
	pub(super) fn painted(&mut self, took: Duration) {
		if let Some(recorder) = &mut self.frame_stats {
			recorder.painted(took);
		}
	}

	/// The frame begun at `started` is done, shown in `showing`.
	fn timed(&mut self, gpu: bool, showing: Duration, started: Instant) {
		if let Some(recorder) = &mut self.frame_stats {
			recorder.finish(gpu, showing, started, Instant::now());
		}
	}
	// pr-check: end window glue

	/// The selection in rows of the frame being drawn; it is kept in lines.
	fn selection_rows(&self) -> Option<Selection> {
		let history_base = self.frame.top_line - self.frame.first_row;
		self.selection?.in_rows(history_base)
	}

	/// Inputs besides the grid's content that change how it is drawn.
	fn draw_key(&self) -> DrawKey {
		DrawKey {
			session: self.sessions[self.active].id,
			focused: self.focused,
			selection: self.selection_rows(),
			search_match: self.search_matches.get(self.search_match_index).copied(),
			overlay: self.search_active || self.command_palette,
		}
	}

	/// The whole frame with any pen, software's painter or the GPU's
	/// quads, with the grid's `rows` (all of them for `None`).
	pub(super) fn compose(&mut self, p: &mut impl Pen, rows: Option<&[bool]>) {
		let height = p.size().1;
		self.restore_background(p, 0, height);
		let scale = self.scale();
		let ui = self.ui_font(18.0 * scale as f32);
		self.draw_tab_bar(p, ui);
		self.draw_terminal(p, scale, rows);
		if self.search_active {
			self.draw_search_box(p, scale, ui);
		}
		if self.command_palette {
			self.draw_command_palette(p, scale, ui);
		}
		self.draw_status_bar(p, scale, ui);
	}

	/// The space around the grid, inside the border and between the chrome,
	/// that edge cell backgrounds extend into.
	pub(super) fn padding_area(&self, width: usize, height: usize) -> crate::graphics::Rect {
		let x = self.border + self.sidebar_width();
		let y = self.padding_top;
		let bottom = height.saturating_sub(self.status_height + self.border);
		crate::graphics::Rect {
			x,
			y,
			width: width.saturating_sub(x + self.border),
			height: bottom.saturating_sub(y),
		}
	}

	pub(super) fn draw_terminal(&mut self, p: &mut impl Pen, scale: usize, rows: Option<&[bool]>) {
		if self.is_split() {
			// Only whole frames: a split tab's partial ones draw its panes.
			return self.draw_panes(p, scale, None);
		}
		let (width, height) = p.size();
		let padding = self.padding_area(width, height);
		let view = self.grid_view(&self.frame, scale, padding);
		draw_grid(p, &mut self.fonts, &view, rows);
		if rows.is_some() {
			// Partial redraws restore the padding above and below the grid
			// every frame; it takes the first and last rows' backgrounds.
			crate::render::padding::extend_above(p, &view);
			crate::render::padding::extend_below(p, &view);
		}
	}

	/// How the window draws `frame` as its grid, edge backgrounds reaching
	/// into `padding`.
	pub(super) fn grid_view<'a>(
		&self,
		frame: &'a Frame,
		scale: usize,
		padding: crate::graphics::Rect,
	) -> GridView<'a> {
		GridView {
			frame,
			origin: (self.grid_left(), self.top),
			cell: (self.cw, self.ch),
			px: self.px,
			ascent: self.ascent,
			scale,
			default_bg: self.theme.bg,
			link_color: self.theme.palette[4],
			selection_colors: selection_colors(self.theme.bg),
			match_colors: (self.theme.bg, self.theme.palette[3]),
			selection: self.selection_rows(),
			search_match: self.search_matches.get(self.search_match_index).copied(),
			focused: self.focused,
			padding: Some(padding),
		}
	}

	fn overlay_place(&self, p: &impl Pen, scale: usize) -> Place {
		Place {
			window: p.size(),
			top: self.top,
			scale,
		}
	}

	fn draw_search_box(&mut self, p: &mut impl Pen, scale: usize, ui: UiFont) {
		let place = self.overlay_place(p, scale);
		// pr-check: window glue: hands the search state to the tested search box model
		let found = (self.search_matches.len(), self.search_match_index);
		let query = (self.search_query.as_str(), found.0, found.1);
		let popover = search_box(place, query, ui.px * 0.86, &self.theme);
		paint(p, &mut self.ui_fonts, |s, f| draw_popover(s, f, &popover));
		// pr-check: end window glue
	}

	fn draw_command_palette(&mut self, p: &mut impl Pen, scale: usize, ui: UiFont) {
		let place = self.overlay_place(p, scale);
		// pr-check: window glue: hands the palette state to the tested palette model
		let query = (self.command_query.as_str(), self.renaming_tab);
		let popover = command_palette(place, query, ui.px * 0.86, &self.theme);
		paint(p, &mut self.ui_fonts, |s, f| draw_popover(s, f, &popover));
		// pr-check: end window glue
	}

	pub(super) fn draw_status_bar(&mut self, p: &mut impl Pen, scale: usize, ui: UiFont) {
		if !self.preferences.show_bottom_bar {
			return;
		}
		let (width, height) = p.size();
		let y = height.saturating_sub(self.status_height + self.border);
		let term = self.sessions[self.active].term();
		let cwd = term.cwd.file_name().and_then(|name| name.to_str());
		// Say when the user acts as someone else or somewhere else.
		let place = term.contexts.label().or_else(|| {
			let local = crate::term::iterm::hostname();
			term.iterm.remote_label(local).map(str::to_owned)
		});
		// pr-check: window glue: hands the live terminal's state to the tested status bar model
		let input = StatusInput {
			link: self.hovered_link.as_deref(),
			place: place.as_deref(),
			cwd: cwd.unwrap_or("~"),
			shell: &term.shell,
			size: (term.cols, term.rows),
			theme: self.theme.kind.name(),
		};
		let rect = (0, y, width, self.status_height);
		let bar = status_bar(&input, rect, (scale, ui.px * 0.76), &self.theme);
		drop(term);
		paint(p, &mut self.ui_fonts, |s, f| draw_status_bar(s, f, &bar));
		// pr-check: end window glue
	}
}
