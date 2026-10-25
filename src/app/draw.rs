//! Main-window frame composition: background, tab bar, grid, overlays, status bar.
use super::*;
use super::{
	canvas::{DrawKey, Plan},
	repaint::keeps_contents,
};
use crate::{
	render::{
		grid::{GridView, draw_grid},
		painter::{ButtonStyle, Painter},
	},
	shell_integration::Phase,
};

/// Commands offered by the command palette, in display order.
pub(super) const PALETTE_COMMANDS: [&str; 7] = [
	"New tab",
	"Settings",
	"About poopterm",
	"Cycle theme",
	"Copy last command output",
	"Close tab",
	"Rename active tab",
];

const PALETTE_COMMANDS_TOP: usize = 54;
const PALETTE_COMMAND_ROW_HEIGHT: usize = 28;
const PALETTE_BOTTOM_PADDING: usize = 16;

fn command_palette_height(command_count: usize, scale: usize) -> usize {
	(PALETTE_COMMANDS_TOP + command_count * PALETTE_COMMAND_ROW_HEIGHT + PALETTE_BOTTOM_PADDING)
		* scale
}

/// Metrics for toolbar text, which keeps its size regardless of the terminal font.
#[derive(Clone, Copy)]
pub(super) struct UiFont {
	pub px: f32,
	pub cw: usize,
	pub ch: usize,
	pub ascent: i32,
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
		let font = self.fonts.primary();
		let metrics = font.horizontal_line_metrics(px).expect("font metrics");
		UiFont {
			px,
			cw: font.metrics('M', px).advance_width.ceil() as usize,
			ch: metrics.new_line_size.ceil() as usize,
			ascent: metrics.ascent.round() as i32,
		}
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
		let Some(mut renderer) = self.renderer.take() else {
			return;
		};
		let size = (self.width as usize, self.height as usize);
		self.sessions[self.active].term().snapshot(&mut self.frame);
		let key = self.draw_key();
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
		renderer.present(
			&self.frame,
			damage,
			&self.canvas.pixels,
			size.0,
			bands.as_deref(),
			damage_only,
		);
		self.canvas.remember(&mut self.frame, key);
		self.renderer = Some(renderer);
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
		let scale = self.scale();
		let ui = self.ui_font(18.0 * scale as f32);
		self.restore_background(p, 0, p.height);
		self.draw_tab_bar(p, scale, ui);
		self.draw_terminal(p, scale, None);
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
		let y = self.top.saturating_sub(self.margin);
		let bottom = height.saturating_sub(self.status_height + self.border);
		crate::graphics::Rect {
			x,
			y,
			width: width.saturating_sub(x + self.border),
			height: bottom.saturating_sub(y),
		}
	}

	pub(super) fn draw_terminal(&mut self, p: &mut Painter, scale: usize, rows: Option<&[bool]>) {
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
			padding: Some(self.padding_area(p.width, p.height)),
		};
		draw_grid(p, &mut self.fonts, &view, rows);
		if rows.is_some() {
			// Partial redraws restore the padding above and below the grid
			// every frame; it takes the first and last rows' backgrounds.
			crate::render::padding::extend_above(p, &view);
			crate::render::padding::extend_below(p, &view);
		}
	}

	/// Draws a bordered, rounded panel and returns nothing; used by overlays.
	fn panel(&self, p: &mut Painter, (x, y, w, h): (usize, usize, usize, usize), radius: usize) {
		let scale = self.scale();
		p.round_rect(x, y, w, h, radius, self.theme.border);
		p.round_rect(
			x + scale,
			y + scale,
			w.saturating_sub(2 * scale),
			h.saturating_sub(2 * scale),
			radius.saturating_sub(scale),
			self.theme.bg,
		);
	}

	fn draw_search_box(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let box_w = (500 * scale).min(p.width.saturating_sub(24 * scale));
		let box_h = 42 * scale;
		let x = p.width.saturating_sub(box_w + 16 * scale);
		let y = self.top + 12 * scale;
		self.panel(p, (x, y, box_w, box_h), 8 * scale);
		let status = if self.search_matches.is_empty() {
			"0 matches".to_owned()
		} else {
			format!(
				"{}/{}",
				self.search_match_index + 1,
				self.search_matches.len()
			)
		};
		let available = (box_w / ui.cw.max(1)).saturating_sub(25);
		let skip = self.search_query.chars().count().saturating_sub(available);
		let query: String = self.search_query.chars().skip(skip).collect();
		let label = format!("Find: {query}  {status}  Enter / Esc");
		let text_y = y + box_h.saturating_sub(ui.ch) / 2;
		let color = self.theme.fg;
		p.text(
			&mut self.fonts,
			x + 12 * scale,
			text_y,
			&label,
			ui.px * 0.82,
			ui.ascent,
			color,
		);
	}

	fn draw_command_palette(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let box_w = (520 * scale).min(p.width.saturating_sub(24 * scale));
		let x = p.width.saturating_sub(box_w) / 2;
		let y = self.top + 20 * scale;
		let query = self.command_query.to_ascii_lowercase();
		let matches: Vec<_> = PALETTE_COMMANDS
			.into_iter()
			.filter(|command| command.to_ascii_lowercase().contains(&query))
			.collect();
		let requested_height = if self.renaming_tab {
			210 * scale
		} else {
			command_palette_height(matches.len(), scale)
		};
		let box_h = requested_height.min(p.height.saturating_sub(y));
		self.panel(p, (x, y, box_w, box_h), 10 * scale);
		let color = self.theme.fg;
		let label = if self.renaming_tab {
			format!("Rename active tab: {}", self.command_query)
		} else {
			format!("Run command: {}", self.command_query)
		};
		p.text_clipped(
			&mut self.fonts,
			x + 16 * scale,
			y + 14 * scale,
			&label,
			ui.px * 0.86,
			ui.ascent,
			color,
			(x + 16 * scale, y, box_w.saturating_sub(32 * scale), box_h),
		);
		if self.renaming_tab {
			return;
		}
		let row_clip = (
			x + 20 * scale,
			y + PALETTE_COMMANDS_TOP * scale,
			box_w.saturating_sub(40 * scale),
			box_h.saturating_sub(PALETTE_COMMANDS_TOP * scale),
		);
		for (index, command) in matches.into_iter().enumerate() {
			p.text_clipped(
				&mut self.fonts,
				x + 20 * scale,
				y + (PALETTE_COMMANDS_TOP + index * PALETTE_COMMAND_ROW_HEIGHT) * scale,
				command,
				ui.px * 0.78,
				ui.ascent,
				color,
				row_clip,
			);
		}
	}

	pub(super) fn draw_status_bar(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		if !self.preferences.show_bottom_bar {
			return;
		}
		let status_y = p.height.saturating_sub(self.status_height + self.border);
		let (bg, border) = (self.theme.bg, self.theme.border);
		p.rect(
			0,
			status_y,
			p.width,
			self.status_height,
			blend(bg, border, 28),
		);
		p.rect(0, status_y, p.width, scale, blend(bg, border, 100));
		let term = self.sessions[self.active].term();
		let cwd = term
			.cwd
			.file_name()
			.and_then(|name| name.to_str())
			.unwrap_or("~");
		let left: String = match &self.hovered_link {
			Some(link) => format!("→ {link}")
				.chars()
				.take(p.width / ui.cw.max(1) / 2)
				.collect(),
			// Say when the user acts as someone else or somewhere else.
			None => match term.contexts.label().or_else(|| {
				let local = crate::term::iterm::hostname();
				term.iterm.remote_label(local).map(str::to_owned)
			}) {
				Some(place) => format!("{place}  ·  {cwd}"),
				None => cwd.to_owned(),
			}
			.chars()
			.filter(|c| !c.is_control())
			.take(p.width / ui.cw.max(1) / 3)
			.collect(),
		};
		let shell = &term.shell;
		let command = match (shell.phase, &shell.command_line, &shell.last_error) {
			(Phase::Output, Some(line), _) => {
				let line: String = line.chars().filter(|c| !c.is_control()).collect();
				let short: String = line.chars().take(32).collect();
				let more = if short.len() < line.len() { "…" } else { "" };
				format!("   ·   running {short}{more}")
			}
			(Phase::Output, None, _) => "   ·   running".to_owned(),
			(_, _, Some(error)) => format!("   ·   error {error}"),
			_ => shell
				.last_exit_code
				.map(|code| format!("   ·   exit {code}"))
				.unwrap_or_default(),
		};
		let right = format!(
			"{} × {}   ·   {}{command}",
			term.cols,
			term.rows,
			self.theme.kind.name()
		);
		drop(term);
		let text_y = status_y + self.status_height.saturating_sub(ui.ch) / 2;
		let right_x = p
			.width
			.saturating_sub(right.chars().count() * (ui.cw * 3 / 4) + 16 * scale);
		let color = self.theme.fg;
		let px = ui.px * 0.72;
		p.text(
			&mut self.fonts,
			16 * scale,
			text_y,
			&left,
			px,
			ui.ascent,
			color,
		);
		p.text(
			&mut self.fonts,
			right_x,
			text_y,
			&right,
			px,
			ui.ascent,
			color,
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn command_palette_height_contains_every_command_row() {
		let scale = 2;
		let final_row_bottom = (PALETTE_COMMANDS_TOP
			+ (PALETTE_COMMANDS.len() - 1) * PALETTE_COMMAND_ROW_HEIGHT
			+ PALETTE_COMMAND_ROW_HEIGHT)
			* scale;
		assert!(command_palette_height(PALETTE_COMMANDS.len(), scale) >= final_row_bottom);
	}
}
