//! Split panes in the main window: each tab's focused pane is its session
//! in `App::sessions`, so input, titles and the tab strip follow it, and
//! the tab's other panes wait in its [`TabPanes`]. Where they all go is
//! `ui::panes`'s to say; these methods route events to it.
use super::*;
use crate::{
	graphics::Rect,
	input::pane_shortcuts::PaneCommand,
	pty::spawn_session,
	ui::panes::{Panes, Place, Split, divider_width},
};

/// A tab's panes, and the sessions of those it does not focus.
pub(super) struct TabPanes {
	pub layout: Panes,
	pub others: Vec<Session>,
}

impl TabPanes {
	/// The panes of a tab showing session `id` alone.
	pub(super) fn new(id: u64) -> Self {
		Self {
			layout: Panes::new(id),
			others: Vec::new(),
		}
	}
}

impl App {
	/// The pixels the active tab's panes share.
	pub(super) fn grid_area(&self) -> Rect {
		let window = (self.width as usize, self.height as usize);
		self.layout().grid_area(window)
	}

	/// Each of the active tab's panes, in slot order.
	pub(super) fn pane_places(&self) -> Vec<Place> {
		self.tab_places(self.active)
	}

	fn tab_places(&self, tab: usize) -> Vec<Place> {
		let divider = divider_width(self.scale());
		let layout = &self.splits[tab].layout;
		layout.places(self.grid_area(), (self.cw, self.ch), divider)
	}

	/// Where the active tab's focused pane sits.
	pub(super) fn focused_rect(&self) -> Rect {
		let focused = self.splits[self.active].layout.focused();
		let places = self.pane_places().into_iter();
		(places
			.filter(|place| place.id == focused)
			.map(|place| place.rect))
		.next()
		.unwrap_or_else(|| self.grid_area())
	}

	/// Whether the active tab shows more than one pane.
	pub(super) fn is_split(&self) -> bool {
		self.splits[self.active].layout.is_split()
	}

	/// Every session, focused or not.
	pub(super) fn all_sessions(&self) -> impl Iterator<Item = &Session> {
		let others = self.splits.iter().flat_map(|tab| tab.others.iter());
		self.sessions.iter().chain(others)
	}

	/// The tab showing session `id`.
	pub(super) fn tab_of(&self, id: u64) -> Option<usize> {
		(self.splits.iter().enumerate())
			.find(|(_, tab)| tab.layout.ids().contains(&id))
			.map(|(index, _)| index)
	}

	/// Session `id` in tab `tab`.
	fn session_in(&self, tab: usize, id: u64) -> Option<&Session> {
		let mut sessions = std::iter::once(&self.sessions[tab]).chain(&self.splits[tab].others);
		sessions.find(|session| session.id == id)
	}

	/// Sizes every pane's terminal and PTY for the window.
	pub(super) fn fit_panes(&self) {
		for tab in 0..self.sessions.len() {
			for place in self.tab_places(tab) {
				if let Some(session) = self.session_in(tab, place.id) {
					self.fit_session(session, place.grid);
				}
			}
		}
	}

	/// Sizes `session`'s terminal and PTY to `grid` columns and rows.
	pub(super) fn fit_session(&self, session: &Session, (cols, rows): (usize, usize)) {
		let mut term = session.term();
		self.describe_window(&mut term);
		// pr-check: window glue: a real PTY takes the size the tested layout gave
		if (cols, rows) != (term.cols, term.rows) {
			term.resize(cols, rows);
			let size = PtySize {
				rows: rows as u16,
				cols: cols as u16,
				pixel_width: 0,
				pixel_height: 0,
			};
			let _ = session.master.resize(size);
		}
		// pr-check: end window glue
	}

	/// Runs a pane shortcut in the active tab.
	pub(super) fn run_pane_command(&mut self, command: PaneCommand) {
		let tab = self.active;
		let (area, cell, divider) = (self.grid_area(), (self.cw, self.ch), self.divider());
		let mut layout = self.splits[tab].layout.clone();
		let moved = match command {
			PaneCommand::Split(split) => return self.split_pane(split),
			PaneCommand::Focus(direction) => layout.focus_toward(direction, area, cell, divider),
			PaneCommand::Cycle(offset) => layout.cycle(offset),
		};
		if moved {
			self.focus_pane(layout.focused());
		}
	}

	pub(super) fn divider(&self) -> usize {
		divider_width(self.scale())
	}

	/// Splits the active tab's focused pane, starting a shell in the new
	/// pane where the focused one is.
	pub(super) fn split_pane(&mut self, split: Split) {
		let tab = self.active;
		if !self.splits[tab].layout.can_split(split) {
			return;
		}
		self.refresh_directories(true);
		let id = self.next_session_id;
		let mut layout = self.splits[tab].layout.clone();
		layout.split(split, id);
		let (area, cell) = (self.grid_area(), (self.cw, self.ch));
		let places = layout.places(area, cell, self.divider());
		let Some(new) = places.iter().find(|place| place.id == id) else {
			return;
		};
		let cwd = self.active_term().cwd.clone();
		let (cols, rows) = new.grid;
		let proxy = self.proxy.clone();
		match spawn_session(id, cols, rows, self.theme, proxy, cwd, &self.term_env) {
			Ok(mut session) => {
				self.next_session_id += 1;
				self.describe_window(&mut session.term());
				std::mem::swap(&mut self.sessions[tab], &mut session);
				self.splits[tab].others.push(session);
				self.splits[tab].layout = layout;
				self.after_focus_moved();
			}
			Err(error) => crate::warnings::warning!("could not split the pane: {error}"),
		}
	}

	/// Focuses pane `id` of the active tab.
	pub(super) fn focus_pane(&mut self, id: u64) {
		let tab = &mut self.splits[self.active];
		let Some(index) = tab.others.iter().position(|session| session.id == id) else {
			return;
		};
		tab.layout.focus(id);
		std::mem::swap(&mut self.sessions[self.active], &mut tab.others[index]);
		self.after_focus_moved();
	}

	/// Input, the selection and search now go to another pane, which may
	/// have another size.
	fn after_focus_moved(&mut self) {
		self.selection = None;
		self.drag_anchor = None;
		self.search_matches.clear();
		self.search_match_index = 0;
		self.sessions[self.active].activity = false;
		self.resize(self.width, self.height);
		self.update_window_title();
		if self.search_active {
			self.search(1, false);
		}
	}

	/// Closes pane `id`, its room going to the panes beside it; closing a
	/// tab's last pane closes the tab.
	pub(super) fn close_pane(&mut self, id: u64, event_loop: &ActiveEventLoop) {
		let Some(tab) = self.tab_of(id) else {
			return;
		};
		let panes = &mut self.splits[tab];
		if !panes.layout.close(id) {
			return self.close_tab(tab, event_loop);
		}
		// The pane taking the closed one's room may take its focus too.
		let focused = panes.layout.focused();
		if let Some(index) = panes.others.iter().position(|s| s.id == focused) {
			std::mem::swap(&mut self.sessions[tab], &mut panes.others[index]);
		}
		let Some(index) = panes.others.iter().position(|s| s.id == id) else {
			return;
		};
		let session = panes.others.remove(index);
		self.end_session(session);
		if tab == self.active {
			self.after_focus_moved();
		} else {
			self.fit_panes();
		}
	}

	/// Focuses the pane under a press at `(x, y)` in the window, if it is
	/// another than the focused one. Returns whether it did, so the press
	/// goes no further.
	pub(super) fn press_pane(&mut self, (x, y): (f64, f64)) -> bool {
		let layout = &self.splits[self.active].layout;
		let point = (x.max(0.0) as usize, y.max(0.0) as usize);
		let hit = layout.hit(self.grid_area(), (self.cw, self.ch), self.divider(), point);
		match hit {
			Some(id) if id != layout.focused() => {
				self.focus_pane(id);
				true
			}
			_ => false,
		}
	}
}
