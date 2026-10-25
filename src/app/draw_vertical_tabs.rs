//! Drawing for the optional vertical tab sidebar.
use super::{draw::UiFont, draw_tabs::hit, *};
use crate::{
	render::{
		blend::blend,
		label::{Label, draw_label},
		painter::Painter,
	},
	term::{
		status::{Progress, ProgressState},
		tab_style::TabStyle,
	},
	ui::{
		tabs::{Marker, VerticalTabLayout, tab_marker},
		ui_font::text_width,
	},
};

/// What one sidebar row shows.
#[derive(Clone, PartialEq)]
struct SidebarTab {
	title: String,
	style: TabStyle,
	marker: Option<Marker>,
	progress: Option<Progress>,
	active: bool,
	hovered: bool,
}

/// Everything the sidebar shows. Partial redraws repaint the sidebar only
/// when this differs from what is on screen; layout changes (resize, scale,
/// theme, preferences) invalidate the canvas and force a full frame anyway.
#[derive(Clone, PartialEq)]
pub(super) struct SidebarView {
	tabs: Vec<SidebarTab>,
	closable: bool,
	hovered_buttons: (bool, bool),
}

impl App {
	fn sidebar_view(&self, layout: &VerticalTabLayout) -> SidebarView {
		let scale = layout.scale;
		let visible_end = self
			.sessions
			.len()
			.min(layout.first_visible + layout.visible_count);
		let tabs = (layout.first_visible..visible_end)
			.map(|index| {
				let session = &self.sessions[index];
				let y = layout.row_y(index);
				let row_h = layout.row_height_for(index).saturating_sub(4 * scale);
				// `title` locks the terminal itself, so it comes first.
				let title = session.title();
				let term = session.term();
				SidebarTab {
					title,
					style: term.tab_style.clone(),
					marker: tab_marker(term.bell, term.status.summary(), session.activity),
					progress: term.status.shown_progress(),
					active: index == self.active,
					hovered: hit(self.cursor_pos, (6 * scale, y, layout.tab_width, row_h)),
				}
			})
			.collect();
		let button_hovered = |x, y: Option<usize>| {
			y.is_some_and(|y| {
				hit(
					self.cursor_pos,
					(x, y, layout.button_width, layout.button_height),
				)
			})
		};
		SidebarView {
			tabs,
			closable: self.sessions.len() > 1,
			hovered_buttons: (
				button_hovered(layout.plus_x, layout.plus_y),
				button_hovered(layout.settings_x, layout.settings_y),
			),
		}
	}

	pub(super) fn draw_vertical_tab_bar(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let layout = self.vertical_tab_layout();
		let view = self.sidebar_view(&layout);
		self.paint_sidebar(p, scale, ui, &layout, &view);
		self.sidebar_shown = Some(view);
	}

	/// Repaints the sidebar if what it shows changed since it was last drawn,
	/// and returns the pixel rows it covers when it did.
	pub(super) fn redraw_sidebar_if_changed(
		&mut self,
		p: &mut Painter,
		scale: usize,
		ui: UiFont,
	) -> Option<(usize, usize)> {
		let layout = self.vertical_tab_layout();
		let view = self.sidebar_view(&layout);
		if self.sidebar_shown.as_ref() == Some(&view) {
			return None;
		}
		self.paint_sidebar(p, scale, ui, &layout, &view);
		self.sidebar_shown = Some(view);
		Some((self.sidebar_top(), p.height))
	}

	fn sidebar_top(&self) -> usize {
		if cfg!(target_os = "macos") {
			self.titlebar_height
		} else {
			self.border
		}
	}

	/// Draws the whole sidebar. Its background is opaque, so nothing under
	/// it needs restoring first.
	fn paint_sidebar(
		&mut self,
		p: &mut Painter,
		scale: usize,
		ui: UiFont,
		layout: &VerticalTabLayout,
		view: &SidebarView,
	) {
		let (bg, border, fg) = (self.theme.bg, self.theme.border, self.theme.fg);
		let (dim, accent) = tab_text_colors(&self.theme);
		let sidebar_top = self.sidebar_top();
		let sidebar_bg = blend(bg, border, 24);
		p.rect(
			self.border,
			sidebar_top,
			layout.sidebar_width.saturating_sub(self.border),
			p.height
				.saturating_sub(sidebar_top)
				.saturating_sub(self.border),
			sidebar_bg,
		);
		p.rect(
			layout.sidebar_width.saturating_sub(self.border + scale),
			sidebar_top,
			scale,
			p.height
				.saturating_sub(sidebar_top)
				.saturating_sub(self.border),
			blend(bg, border, 100),
		);
		for (local_index, tab) in view.tabs.iter().enumerate() {
			let x = 6 * scale;
			let index = layout.first_visible + local_index;
			let y = layout.row_y(index);
			let row_height = layout.row_height_for(index);
			let row_h = row_height.saturating_sub(4 * scale);
			let (active, hovered, style) = (tab.active, tab.hovered, &tab.style);
			let show_close = view.closable
				&& (if layout.compact_rail {
					hovered
				} else {
					active || hovered
				});
			if let Some(color) = style.color {
				let shade = if active {
					150
				} else if hovered {
					110
				} else {
					80
				};
				p.round_rect(
					x,
					y,
					layout.tab_width,
					row_h,
					5 * scale,
					blend(bg, color, shade),
				);
			} else if active || hovered {
				let shade = if active { 74 } else { 40 };
				p.round_rect(
					x,
					y,
					layout.tab_width,
					row_h,
					5 * scale,
					blend(bg, border, shade),
				);
			}
			if active {
				p.round_rect(
					x,
					y + 7 * scale,
					3 * scale,
					row_h.saturating_sub(14 * scale),
					scale,
					accent,
				);
			}
			let (marker, progress) = (tab.marker, tab.progress);
			let dot = marker
				.map(|marker| match marker {
					Marker::Bell => ("!", accent),
					Marker::Blocked => ("•", self.theme.palette[3]),
					Marker::Error => ("•", self.theme.palette[1]),
					Marker::Done => ("•", self.theme.palette[2]),
					Marker::Working => ("•", accent),
					Marker::Activity => ("•", dim),
				})
				.or(style.indicator.map(|color| ("•", color)));
			if layout.compact_rail {
				let label = (index + 1).to_string();
				let glyph_width = text_width(&label, |ch| self.ui_fonts.advance(ch, ui.px * 0.9));
				p.text(
					&mut self.ui_fonts,
					x + layout.tab_width.saturating_sub(glyph_width) / 2,
					y + row_h.saturating_sub(ui.ch) / 2,
					&label,
					ui.px * 0.9,
					ui.ascent,
					fg,
				);
				if let Some((_, color)) = dot.filter(|_| !hovered) {
					p.round_rect(
						x + layout.tab_width.saturating_sub(10 * scale),
						y + 3 * scale,
						6 * scale,
						6 * scale,
						3 * scale,
						color,
					);
				}
				if show_close {
					p.text(
						&mut self.ui_fonts,
						x + layout.tab_width.saturating_sub(17 * scale),
						y + row_h.saturating_sub(ui.ch) / 2,
						"×",
						ui.px * 0.7,
						(ui.ascent as f32 * 0.82) as i32,
						fg,
					);
				}
				if let Some(progress) = progress {
					self.draw_vertical_tab_progress(p, (x, y, layout.tab_width, row_height), scale, progress);
				}
				continue;
			}
			let text_y = y + row_h.saturating_sub(ui.ch) / 2;
			let small_ascent = (ui.ascent as f32 * 0.82) as i32;
			if let Some((glyph, color)) = dot {
				p.text(
					&mut self.ui_fonts,
					x + 8 * scale,
					text_y,
					glyph,
					ui.px * 0.62,
					ui.ascent,
					color,
				);
			}
			let small_px = ui.px * 0.76;
			let left = if dot.is_some() { 20 } else { 8 } * scale;
			let right = if show_close { 24 } else { 8 } * scale;
			let room = layout.tab_width.saturating_sub(left + right);
			let label = Label {
				title: &tab.title,
				status: style.status.as_deref(),
				colors: (fg, style.status_color.unwrap_or(dim)),
			};
			let font = (small_px, small_ascent);
			let clip = (x + left, y, room, row_h);
			draw_label(p, &mut self.ui_fonts, &label, text_y, font, clip);
			if show_close {
				p.text(
					&mut self.ui_fonts,
					x + layout.tab_width.saturating_sub(20 * scale),
					text_y,
					"×",
					ui.px * 0.82,
					small_ascent,
					fg,
				);
			}
			if let Some(progress) = progress {
				self.draw_vertical_tab_progress(p, (x, y, layout.tab_width, row_height), scale, progress);
			}
		}

		let style = self.button_style(scale);
		let button_text_y = |y: usize| y + layout.button_height.saturating_sub(ui.ch) / 2;
		for (x, y, label, hovered) in [
			(layout.plus_x, layout.plus_y, "+", view.hovered_buttons.0),
			(
				layout.settings_x,
				layout.settings_y,
				"⚙",
				view.hovered_buttons.1,
			),
		] {
			let Some(y) = y else {
				continue;
			};
			let rect = (x, y, layout.button_width, layout.button_height);
			p.button(rect, style, hovered);
			p.text(
				&mut self.ui_fonts,
				x + layout.button_width.saturating_sub(ui.cw) / 2,
				button_text_y(y),
				label,
				ui.px,
				ui.ascent,
				fg,
			);
		}
	}

	fn draw_vertical_tab_progress(
		&self,
		p: &mut Painter,
		(x, y, width, row_height): (usize, usize, usize, usize),
		scale: usize,
		progress: Progress,
	) {
		let color = match progress.state {
			ProgressState::Error => self.theme.palette[1],
			ProgressState::Paused => self.theme.palette[3],
			_ => self.theme.palette[4],
		};
		let left = x + 8 * scale;
		let track = width.saturating_sub(16 * scale);
		let bar_y = y + row_height.saturating_sub(6 * scale);
		p.rect(
			left,
			bar_y,
			track,
			2 * scale,
			blend(self.theme.bg, color, 60),
		);
		let done = progress
			.percent
			.map_or(track, |percent| track * usize::from(percent) / 100);
		let shade = if progress.percent.is_some() { 255 } else { 140 };
		p.rect(
			left,
			bar_y,
			done,
			2 * scale,
			blend(self.theme.bg, color, shade),
		);
	}
}
