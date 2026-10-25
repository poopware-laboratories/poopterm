//! The horizontal tab strip, drawn as the header bar from each tab's state.
use super::{draw::UiFont, *};
use crate::{
	render::{header::draw_header, painter::Painter},
	ui::{
		header::{HeaderInput, TabInfo, header},
		tabs::tab_marker,
	},
};

pub(super) fn hit(cursor: (f64, f64), (x, y, w, h): (usize, usize, usize, usize)) -> bool {
	cursor.0 >= x as f64
		&& cursor.0 < (x + w) as f64
		&& cursor.1 >= y as f64
		&& cursor.1 < (y + h) as f64
}

impl App {
	pub(super) fn draw_tab_bar(&mut self, p: &mut Painter, ui: UiFont) {
		if !self.preferences.show_tabs {
			return;
		}
		if self.preferences.vertical_tabs {
			self.draw_vertical_tab_bar(p, ui);
			return;
		}
		let layout = self.tab_layout();
		let shown = layout.first_visible
			..self
				.sessions
				.len()
				.min(layout.first_visible + layout.visible_count);
		let tabs: Vec<_> = self.sessions[shown].iter().map(tab_info).collect();
		// pr-check: window glue: hands the window's tab state to the tested header model
		let header = header(&HeaderInput {
			layout: &layout,
			tabs: &tabs,
			count: self.sessions.len(),
			active: self.active,
			cursor: self.cursor_pos,
			bounds: (self.tab_y(), self.tab_height),
			px: ui.px * 0.82,
			theme: &self.theme,
		});
		draw_header(p, &mut self.ui_fonts, &header);
		// pr-check: end window glue
	}
}

/// What the header shows of `session`.
pub(super) fn tab_info(session: &Session) -> TabInfo {
	// pr-check: window glue: copies a live session's state, which needs a running shell
	// The title locks the terminal itself, so it comes first.
	let title = session.title();
	let term = session.term();
	TabInfo {
		title,
		style: term.tab_style.clone(),
		marker: tab_marker(term.bell, term.status.summary(), session.activity),
		progress: term.status.shown_progress(),
	}
	// pr-check: end window glue
}
