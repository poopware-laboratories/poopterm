//! Split panes in the main window: each tab's focused pane is its session
//! in `App::sessions`, so input, titles and the tab strip follow it, and
//! the tab's other panes wait in its `ui::pane_sessions::TabPanes`, which
//! with `ui::panes` decides everything; these methods route events to them.
use super::*;
use crate::{
	graphics::Rect,
	input::pane_shortcuts::PaneCommand,
	pty::spawn_session,
	ui::{
		pane_sessions::{Closed, Shown},
		panes::{Place, Split, divider_width},
	},
};

pub(super) type TabPanes = crate::ui::pane_sessions::TabPanes<Session>;

impl Shown for Session {
	fn id(&self) -> u64 {
		self.id
	}
}

impl App {
	/// The active tab's grid area, cell size and divider width.
	pub(super) fn pane_geometry(&self) -> (Rect, (usize, usize), usize) {
		let window = (self.width as usize, self.height as usize);
		let area = self.layout().grid_area(window);
		(area, (self.cw, self.ch), divider_width(self.scale()))
	}

	/// Each of tab `tab`'s panes, in slot order.
	pub(super) fn tab_places(&self, tab: usize) -> Vec<Place> {
		let (area, cell, divider) = self.pane_geometry();
		self.splits[tab].layout().places(area, cell, divider)
	}

	/// Where the active tab's focused pane sits.
	pub(super) fn focused_rect(&self) -> Rect {
		let focused = self.sessions[self.active].id;
		let places = self.tab_places(self.active).into_iter();
		(places
			.filter(|place| place.id == focused)
			.map(|place| place.rect))
		.next()
		.unwrap_or(self.pane_geometry().0)
	}

	/// Whether the active tab shows more than one pane.
	pub(super) fn is_split(&self) -> bool {
		self.splits[self.active].layout().is_split()
	}

	/// Every session, focused or not.
	pub(super) fn all_sessions(&self) -> impl Iterator<Item = &Session> {
		let others = self.splits.iter().flat_map(|tab| tab.others().iter());
		self.sessions.iter().chain(others)
	}

	/// The tab showing session `id`.
	pub(super) fn tab_of(&self, id: u64) -> Option<usize> {
		(self.splits.iter()).position(|tab| tab.layout().ids().contains(&id))
	}

	/// Sizes every pane's terminal and PTY for the window.
	pub(super) fn fit_panes(&self) {
		for (tab, panes) in self.splits.iter().enumerate() {
			for place in self.tab_places(tab) {
				if let Some(session) = panes.find(&self.sessions[tab], place.id) {
					self.fit_session(session, place.grid);
				}
			}
		}
	}

	/// Sizes `session`'s terminal and PTY to `grid` columns and rows.
	fn fit_session(&self, session: &Session, (cols, rows): (usize, usize)) {
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
		let (tab, (area, cell, divider)) = (self.active, self.pane_geometry());
		let focused = &mut self.sessions[tab];
		// pr-check: window glue: hands the shortcut to the tested pane model
		let moved = match command {
			PaneCommand::Split(split) => return self.split_pane(split),
			PaneCommand::Focus(direction) => (self.splits[tab]).focus_by(focused, |panes| {
				panes.focus_toward(direction, area, cell, divider)
			}),
			PaneCommand::Cycle(offset) => self.splits[tab].focus_by(focused, |p| p.cycle(offset)),
		};
		if moved {
			self.after_focus_moved();
		}
		// pr-check: end window glue
	}

	/// Splits the active tab's focused pane, starting a shell in the new
	/// pane where the focused one is.
	fn split_pane(&mut self, split: Split) {
		let (tab, id) = (self.active, self.next_session_id);
		let Some(place) = self.splits[tab].split_place(split, id, self.pane_geometry()) else {
			return;
		};
		self.refresh_directories(true);
		let cwd = self.active_term().cwd.clone();
		let ((cols, rows), proxy) = (place.grid, self.proxy.clone());
		// pr-check: window glue: a real shell starts in the place the tested model gave
		match spawn_session(id, cols, rows, self.theme, proxy, cwd, &self.term_env) {
			Ok(session) => {
				self.next_session_id += 1;
				self.describe_window(&mut session.term());
				let split = self.splits[tab].split(&mut self.sessions[tab], split, session);
				if let Err(session) = split {
					self.end_session(session);
				}
				self.after_focus_moved();
			}
			Err(error) => crate::warnings::warning!("could not split the pane: {error}"),
		}
		// pr-check: end window glue
	}

	/// Focuses pane `id` of the active tab.
	pub(super) fn focus_pane(&mut self, id: u64) {
		let tab = self.active;
		if self.splits[tab].focus(&mut self.sessions[tab], id) {
			self.after_focus_moved();
		}
	}

	/// Input, the selection and search now go to another pane, which may
	/// have another size.
	fn after_focus_moved(&mut self) {
		// pr-check: window glue: forgets what the old pane showed and redraws
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
		// pr-check: end window glue
	}

	/// Closes pane `id`, its room going to the panes beside it; closing a
	/// tab's last pane closes the tab.
	pub(super) fn close_pane(&mut self, id: u64, event_loop: &ActiveEventLoop) {
		let Some(tab) = self.tab_of(id) else {
			return;
		};
		// pr-check: window glue: acts on what the tested pane model decided
		match self.splits[tab].close(&mut self.sessions[tab], id) {
			Closed::Missing => {}
			Closed::Last => self.close_tab(tab, event_loop),
			Closed::Pane(session) => {
				self.end_session(session);
				match tab == self.active {
					true => self.after_focus_moved(),
					false => self.fit_panes(),
				}
			}
		}
		// pr-check: end window glue
	}

	/// Focuses the pane under a press at `(x, y)` in the window, if it is
	/// another than the focused one. Returns whether it did, so the press
	/// goes no further.
	pub(super) fn press_pane(&mut self, (x, y): (f64, f64)) -> bool {
		let point = (x.max(0.0) as usize, y.max(0.0) as usize);
		let hit = self.splits[self.active].other_at(self.pane_geometry(), point);
		hit.inspect(|&id| self.focus_pane(id)).is_some()
	}
}
