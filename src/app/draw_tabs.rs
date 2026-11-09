//! The horizontal tab strip, drawn as the header bar from each tab's state.
use super::{draw::UiFont, *};
use crate::{
	render::{header::draw_header, pen::Pen, scene::paint},
	ui::{
		header::{HeaderInput, TabInfo, WindowState, add_window_buttons, header},
		tabs::tab_marker,
	},
};

impl App {
	pub(super) fn draw_tab_bar(&mut self, p: &mut impl Pen, ui: UiFont) {
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
		let tabs: Vec<_> = shown
			.map(|index| {
				tab_info(
					&self.sessions[index],
					self.splits[index].layout().ids().len(),
				)
			})
			.collect();
		// pr-check: window glue: hands the window's tab state to the tested header model
		let mut header = header(&HeaderInput {
			layout: &layout,
			tabs: &tabs,
			count: self.sessions.len(),
			active: self.active,
			cursor: self.cursor_pos,
			bounds: (self.tab_y(), self.tab_height),
			px: ui.px * 0.82,
			theme: &self.theme,
		});
		// pr-check: end window glue
		// pr-check: window glue: reads the live window's focus and maximized state
		let maximized = self.window_maximized();
		let window = WindowState {
			maximized,
			focused: self.focused,
			pressed: self.window_presses.held(),
		};
		let controls = self.window_controls();
		add_window_buttons(&mut header, &controls, window, self.cursor_pos);
		paint(p, &mut self.ui_fonts, |s, f| draw_header(s, f, &header));
		// pr-check: end window glue
	}
}

impl App {
	/// What the header shows of each tab in `shown`.
	pub(super) fn tab_infos(&self, shown: std::ops::Range<usize>) -> Vec<TabInfo> {
		// pr-check: window glue: pairs each live session with its tab's pane count
		let panes = |index: usize| self.splits[index].layout().ids().len();
		shown
			.map(|index| tab_info(&self.sessions[index], panes(index)))
			.collect()
		// pr-check: end window glue
	}
}

/// What the header shows of `session`.
pub(super) fn tab_info(session: &Session, panes: usize) -> TabInfo {
	// pr-check: window glue: copies a live session's state, which needs a running shell
	// The title locks the terminal itself, so it comes first.
	let title = session.title();
	let term = session.term();
	TabInfo {
		title,
		style: term.tab_style.clone(),
		marker: tab_marker(term.bell, term.status.summary(), session.activity),
		progress: term.status.shown_progress(),
		panes,
	}
	// pr-check: end window glue
}
