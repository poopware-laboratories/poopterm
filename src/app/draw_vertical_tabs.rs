//! Drawing for the optional vertical tab sidebar.
use super::{draw::UiFont, draw_tabs::tab_info, *};
use crate::{
	render::{painter::Painter, scene::paint, sidebar::draw_sidebar},
	ui::{
		sidebar::{SidebarInput, SidebarState, sidebar, sidebar_target},
		tabs::VerticalTabLayout,
	},
};

impl App {
	fn sidebar_state(&self, layout: &VerticalTabLayout) -> SidebarState {
		let count = self.sessions.len();
		let shown = layout.first_visible..count.min(layout.first_visible + layout.visible_count);
		// pr-check: window glue: copies the live sessions into the tested sidebar model
		SidebarState {
			tabs: self.sessions[shown].iter().map(tab_info).collect(),
			first_visible: layout.first_visible,
			count,
			active: self.active,
			target: sidebar_target(layout, count, self.cursor_pos),
		}
		// pr-check: end window glue
	}

	pub(super) fn draw_vertical_tab_bar(&mut self, p: &mut Painter, ui: UiFont) {
		let layout = self.vertical_tab_layout();
		let state = self.sidebar_state(&layout);
		self.paint_sidebar(p, ui, &layout, &state);
		self.sidebar_shown = Some(state);
	}

	/// Repaints the sidebar if what it shows changed since it was last drawn,
	/// and returns the pixel rows it covers when it did.
	pub(super) fn redraw_sidebar_if_changed(
		&mut self,
		p: &mut Painter,
		ui: UiFont,
	) -> Option<(usize, usize)> {
		let layout = self.vertical_tab_layout();
		let state = self.sidebar_state(&layout);
		if self.sidebar_shown.as_ref() == Some(&state) {
			return None;
		}
		self.paint_sidebar(p, ui, &layout, &state);
		self.sidebar_shown = Some(state);
		Some((self.sidebar_top(), p.height))
	}

	fn sidebar_top(&self) -> usize {
		if cfg!(target_os = "macos") {
			self.titlebar_height
		} else {
			self.border
		}
	}

	fn paint_sidebar(
		&mut self,
		p: &mut Painter,
		ui: UiFont,
		layout: &VerticalTabLayout,
		state: &SidebarState,
	) {
		let top = self.sidebar_top();
		// pr-check: window glue: hands the window's sizes to the tested sidebar model
		let sidebar = sidebar(&SidebarInput {
			layout,
			state,
			bounds: (top, p.height.saturating_sub(top)),
			px: ui.px * 0.82,
			theme: &self.theme,
		});
		paint(p, &mut self.ui_fonts, |scene, fonts| {
			draw_sidebar(scene, fonts, &sidebar)
		});
		// pr-check: end window glue
	}
}
