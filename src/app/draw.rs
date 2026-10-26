//! Main-window frame composition: background, tab bar, grid, overlays, status bar.
use super::*;
use super::{
	canvas::{DrawKey, Plan},
	renderer::Shown,
	repaint::keeps_contents,
};
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
		self.pacer.drawn(std::time::Instant::now());
		self.refresh_search_if_dirty();
		self.fonts.trim();
		self.ui_fonts.trim();
		let Some(mut renderer) = self.renderer.take() else {
			return;
		};
		let size = (self.width as usize, self.height as usize);
		self.sessions[self.active].term().snapshot(&mut self.frame);
		let key = self.draw_key();
		#[cfg(any(target_os = "linux", target_os = "freebsd"))]
		if let renderer::Renderer::Gpu(gpu) = renderer {
			self.renderer = self.draw_on_gpu(gpu, key, size);
			return;
		}
		let plan = self
			.canvas
			.plan(&self.frame, key, size, self.wallpaper.is_none());
		// Debug aid for visual checks: compare against full redraws.
		let plan = if self.full_redraw_only {
			Plan::Full
		} else {
			plan
		};
		let mut pixels = std::mem::take(&mut self.canvas.pixels);
		let mut painter = Painter::new(&mut pixels, size.0, size.1);
		let bands = match &plan {
			Plan::Full => {
				self.compose(&mut painter);
				None
			}
			Plan::Partial(damage) => Some(self.compose_damaged(&mut painter, damage)),
		};
		self.canvas.pixels = pixels;
		let damage = match &plan {
			Plan::Full => None,
			Plan::Partial(damage) => Some(damage),
		};
		let damage_only = self.window.as_deref().is_some_and(keeps_contents);
		let shown = renderer.present(
			&self.frame,
			damage,
			&self.canvas.pixels,
			size.0,
			bands.as_deref(),
			damage_only,
		);
		self.canvas.remember(&mut self.frame, key);
		self.renderer = self.after_shown(renderer, shown);
	}

	/// The renderer to draw the next frame with, after this one was `shown`.
	fn after_shown(
		&mut self,
		renderer: renderer::Renderer,
		shown: Shown,
	) -> Option<renderer::Renderer> {
		match shown {
			Shown::Done => Some(renderer),
			Shown::Retry => {
				// After the frame interval, as releases take a frame or so.
				self.schedule_redraw();
				Some(renderer)
			}
			Shown::Failed(error) => {
				self.request_redraw();
				renderer
					.after_presenter(&error)
					.inspect_err(|error| eprintln!("poopterm: software renderer unavailable: {error}"))
					.ok()
			}
		}
	}

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

	pub(super) fn compose(&mut self, p: &mut Painter) {
		self.compose_around(p, true);
	}

	/// Composes the frame, with the grid only when `grid`: else the GPU
	/// draws it over the rest.
	pub(super) fn compose_around(&mut self, p: &mut Painter, grid: bool) {
		let scale = self.scale();
		let ui = self.ui_font(18.0 * scale as f32);
		self.restore_background(p, 0, p.height);
		self.draw_tab_bar(p, ui);
		if grid {
			self.draw_terminal(p, scale, None);
		}
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
		let (width, height) = p.size();
		let view = GridView {
			frame: &self.frame,
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
			padding: Some(self.padding_area(width, height)),
		};
		draw_grid(p, &mut self.fonts, &view, rows);
		if rows.is_some() {
			// Partial redraws restore the padding above and below the grid
			// every frame; it takes the first and last rows' backgrounds.
			crate::render::padding::extend_above(p, &view);
			crate::render::padding::extend_below(p, &view);
		}
	}

	fn overlay_place(&self, p: &Painter, scale: usize) -> Place {
		Place {
			window: (p.width, p.height),
			top: self.top,
			scale,
		}
	}

	fn draw_search_box(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let place = self.overlay_place(p, scale);
		// pr-check: window glue: hands the search state to the tested search box model
		let found = (self.search_matches.len(), self.search_match_index);
		let query = (self.search_query.as_str(), found.0, found.1);
		let popover = search_box(place, query, ui.px * 0.86, &self.theme);
		paint(p, &mut self.ui_fonts, |s, f| draw_popover(s, f, &popover));
		// pr-check: end window glue
	}

	fn draw_command_palette(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let place = self.overlay_place(p, scale);
		// pr-check: window glue: hands the palette state to the tested palette model
		let query = (self.command_query.as_str(), self.renaming_tab);
		let popover = command_palette(place, query, ui.px * 0.86, &self.theme);
		paint(p, &mut self.ui_fonts, |s, f| draw_popover(s, f, &popover));
		// pr-check: end window glue
	}

	pub(super) fn draw_status_bar(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		if !self.preferences.show_bottom_bar {
			return;
		}
		let y = p.height.saturating_sub(self.status_height + self.border);
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
		let rect = (0, y, p.width, self.status_height);
		let bar = status_bar(&input, rect, (scale, ui.px * 0.76), &self.theme);
		drop(term);
		paint(p, &mut self.ui_fonts, |s, f| draw_status_bar(s, f, &bar));
		// pr-check: end window glue
	}
}
