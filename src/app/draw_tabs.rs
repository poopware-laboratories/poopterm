//! The tab strip: tabs with status markers, progress and the colors and
//! status programs set, scroll arrows, new-tab and settings buttons.
use super::{draw::UiFont, *};
use crate::{
	render::painter::Painter,
	term::status::{Progress, ProgressState},
	ui::tabs::{Marker, tab_marker},
};

pub(super) fn hit(cursor: (f64, f64), (x, y, w, h): (usize, usize, usize, usize)) -> bool {
	cursor.0 >= x as f64
		&& cursor.0 < (x + w) as f64
		&& cursor.1 >= y as f64
		&& cursor.1 < (y + h) as f64
}

impl App {
	pub(super) fn draw_tab_bar(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		if !self.preferences.show_tabs {
			return;
		}
		if self.preferences.vertical_tabs {
			self.draw_vertical_tab_bar(p, scale, ui);
			return;
		}
		let tab_y = self.tab_y();
		let layout = self.tab_layout();
		let (bg, border, fg) = (self.theme.bg, self.theme.border, self.theme.fg);
		let (dim, accent) = tab_text_colors(&self.theme);
		let text_y = tab_y + self.tab_height.saturating_sub(ui.ch) / 2;
		let small_ascent = (ui.ascent as f32 * 0.82) as i32;
		p.rect(0, tab_y, p.width, self.tab_height, bg);
		if !cfg!(target_os = "macos") {
			p.text(
				&mut self.fonts,
				16 * scale,
				text_y,
				"poopterm",
				ui.px * 0.82,
				ui.ascent,
				fg,
			);
		}
		p.rect(
			0,
			tab_y + self.tab_height.saturating_sub(scale),
			p.width,
			scale,
			blend(bg, border, 90),
		);

		let visible = layout.first_visible
			..self
				.sessions
				.len()
				.min(layout.first_visible + layout.visible_count);
		for index in visible {
			let x = layout.tab_x(index);
			let tab_width = layout.tab_width(index);
			let active = index == self.active;
			let hovered = hit(
				self.cursor_pos,
				(x, tab_y, tab_width, self.top.saturating_sub(tab_y)),
			);
			let session = &self.sessions[index];
			let style = session.term().tab_style.clone();
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
					tab_y + 4 * scale,
					tab_width,
					self.tab_height.saturating_sub(6 * scale),
					5 * scale,
					blend(bg, color, shade),
				);
			} else if active || hovered {
				let shade = if active { 70 } else { 38 };
				p.round_rect(
					x,
					tab_y + 4 * scale,
					tab_width,
					self.tab_height.saturating_sub(6 * scale),
					5 * scale,
					blend(bg, border, shade),
				);
			}
			if active {
				p.round_rect(
					x + 8 * scale,
					tab_y + self.tab_height.saturating_sub(5 * scale),
					tab_width.saturating_sub(16 * scale),
					3 * scale,
					scale,
					accent,
				);
			}
			let (marker, progress) = {
				let term = session.term();
				let marker = tab_marker(term.bell, term.status.summary(), session.activity);
				(marker, term.status.shown_progress())
			};
			// The terminal's own markers say more than a program's indicator.
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
			if let Some((glyph, color)) = dot {
				p.text(
					&mut self.fonts,
					x + 10 * scale,
					text_y,
					glyph,
					ui.px * 0.62,
					ui.ascent,
					color,
				);
			}
			if let Some(progress) = progress {
				self.draw_tab_progress(p, (x, tab_y, tab_width), scale, progress);
			}
			// Labels are drawn smaller than the UI font, up to the close button.
			let small_cw = ui.cw as f32 * 0.82;
			let left = if dot.is_some() { 22 } else { 12 } * scale;
			let right = if self.sessions.len() > 1 { 32 } else { 12 } * scale;
			let room = tab_width.saturating_sub(left + right);
			let max_chars = ((room as f32 / small_cw.max(1.0)) as usize).max(1);
			let (label, status) = tab_label(&session.title(), style.status.as_deref(), max_chars);
			let label_x = x + left;
			p.text_clipped(
				&mut self.fonts,
				label_x,
				text_y,
				&label,
				ui.px * 0.82,
				small_ascent,
				fg,
				(
					label_x,
					tab_y + 4 * scale,
					room,
					self.tab_height.saturating_sub(6 * scale),
				),
			);
			if !status.is_empty() {
				let status_x = label_x + ((label.chars().count() + 2) as f32 * small_cw) as usize;
				p.text_clipped(
					&mut self.fonts,
					status_x,
					text_y,
					&status,
					ui.px * 0.82,
					small_ascent,
					style.status_color.unwrap_or(dim),
					(
						label_x,
						tab_y + 4 * scale,
						room,
						self.tab_height.saturating_sub(6 * scale),
					),
				);
			}
			if self.sessions.len() > 1 {
				p.text(
					&mut self.fonts,
					x + tab_width.saturating_sub(28 * scale),
					text_y,
					"×",
					ui.px * 0.82,
					small_ascent,
					fg,
				);
			}
		}

		let style = self.button_style(scale);
		let arrow_h = 30 * scale;
		let arrow_y = tab_y + self.tab_height.saturating_sub(arrow_h) / 2;
		let arrows = [
			(layout.left_arrow_x, "‹", layout.first_visible > 0),
			(
				layout.right_arrow_x,
				"›",
				layout.first_visible + layout.visible_count < self.sessions.len(),
			),
		];
		for (x, label, enabled) in arrows {
			if !enabled {
				continue;
			}
			let rect = (x, arrow_y, layout.arrow_w, arrow_h);
			p.button(rect, style, hit(self.cursor_pos, rect));
			p.text(
				&mut self.fonts,
				x + layout.arrow_w.saturating_sub(ui.cw) / 2,
				arrow_y + arrow_h.saturating_sub(ui.ch) / 2,
				label,
				ui.px * 0.82,
				small_ascent,
				fg,
			);
		}

		let button_h = 34 * scale;
		let button_y = tab_y + self.tab_height.saturating_sub(button_h) / 2;
		for (x, w, label, shown) in [
			(
				layout.plus_x,
				layout.plus_w,
				"+",
				self.preferences.show_new_tab_button,
			),
			(
				layout.settings_x,
				layout.settings_w,
				"⚙",
				self.preferences.show_settings_button,
			),
		] {
			if !shown {
				continue;
			}
			let rect = (x, button_y, w, button_h);
			p.button(rect, style, hit(self.cursor_pos, rect));
			p.text(
				&mut self.fonts,
				x + w.saturating_sub(ui.cw) / 2,
				button_y + button_h.saturating_sub(ui.ch) / 2,
				label,
				ui.px,
				ui.ascent,
				fg,
			);
		}
	}

	/// A thin bar under a tab's title: the share done, or a dim full bar
	/// when the program cannot say.
	fn draw_tab_progress(
		&self,
		p: &mut Painter,
		(x, tab_y, width): (usize, usize, usize),
		scale: usize,
		progress: Progress,
	) {
		let color = match progress.state {
			ProgressState::Error => self.theme.palette[1],
			ProgressState::Paused => self.theme.palette[3],
			_ => self.theme.palette[4],
		};
		let (left, track) = (x + 12 * scale, width.saturating_sub(24 * scale));
		let y = tab_y + self.tab_height.saturating_sub(10 * scale);
		let bg = self.theme.bg;
		p.rect(left, y, track, 2 * scale, blend(bg, color, 60));
		let done = match progress.percent {
			Some(percent) => track * usize::from(percent) / 100,
			None => track,
		};
		let shade = if progress.percent.is_some() { 255 } else { 140 };
		p.rect(left, y, done, 2 * scale, blend(bg, color, shade));
	}
}

/// Truncates `title` to `max_chars` characters, ending in `…` when shortened.
pub(super) fn ellipsize(title: &str, max_chars: usize) -> String {
	let max_chars = max_chars.max(1);
	if title.chars().count() <= max_chars {
		return title.to_owned();
	}
	let mut label: String = title.chars().take(max_chars).collect();
	if max_chars > 1 {
		label.pop();
		label.push('…');
	}
	label
}

/// Fits a tab's title and status into `max_chars`: after a two-character
/// gap, the status gets at least half, or all the title leaves, and the
/// title the rest.
pub(super) fn tab_label(title: &str, status: Option<&str>, max_chars: usize) -> (String, String) {
	let room = (max_chars / 2).max(max_chars.saturating_sub(title.chars().count() + 2));
	let status_chars = status.map_or(0, |status| status.chars().count().min(room));
	if status_chars == 0 {
		return (ellipsize(title, max_chars), String::new());
	}
	(
		ellipsize(title, max_chars.saturating_sub(status_chars + 2)),
		ellipsize(status.unwrap_or_default(), status_chars),
	)
}

#[cfg(test)]
mod tests {
	use super::{ellipsize, tab_label};

	#[test]
	fn statuses_share_the_tab_with_the_title() {
		let label = |status| tab_label("cargo build", status, 20);
		assert_eq!(label(None), ("cargo build".into(), String::new()));
		assert_eq!(label(Some("ok")), ("cargo build".into(), "ok".into()));
		assert_eq!(
			label(Some("compiling poopterm")),
			("cargo b…".into(), "compiling…".into())
		);
		assert_eq!(
			tab_label("a very long title indeed", Some("ok"), 20),
			("a very long tit…".into(), "ok".into())
		);
		assert_eq!(tab_label("vim", Some("x"), 1), ("v".into(), String::new()));
		// A short title leaves the status more room.
		assert_eq!(
			tab_label("vim", Some("compiling poopterm"), 20),
			("vim".into(), "compiling poop…".into())
		);
	}

	#[test]
	fn long_titles_are_ellipsized_to_fit() {
		assert_eq!(ellipsize("vim main.rs", 20), "vim main.rs");
		assert_eq!(ellipsize("vim main.rs", 5), "vim …");
		assert_eq!(ellipsize("vim", 1), "v");
	}
}
