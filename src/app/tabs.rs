//! Opening, switching, closing and persisting tabs.
use super::*;
use crate::{pty::spawn_session, session_store::save_session_dirs};
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::thread;

impl App {
	pub(super) fn tab_left_reserve(&self) -> usize {
		(if cfg!(target_os = "macos") { 80 } else { 112 }) * self.scale()
	}

	pub(super) fn tab_y(&self) -> usize {
		if self.preferences.show_tabs && self.preferences.vertical_tabs {
			return self.vertical_tab_layout().tab_start_y;
		}
		if cfg!(target_os = "macos") {
			self.titlebar_height.saturating_sub(self.tab_height) / 2
		} else {
			self.top.saturating_sub(self.tab_height)
		}
	}

	pub(super) fn tab_layout(&self) -> TabLayout {
		tab_layout_with_compaction(
			self.width as usize,
			self.sessions.len(),
			self.scale(),
			TabStripOptions {
				left_reserve: self.tab_left_reserve(),
				scroll: self.tab_scroll,
				show_plus: self.preferences.show_new_tab_button,
				show_settings: self.preferences.show_settings_button,
				active_tab: self.active,
				compact_tabs: self.preferences.compact_tabs,
			},
		)
	}

	pub(super) fn vertical_tab_layout(&self) -> VerticalTabLayout {
		vertical_tab_layout(
			self.height as usize,
			self.sessions.len(),
			self.scale(),
			VerticalTabOptions {
				top_reserve: self.titlebar_height,
				status_height: self.status_height + self.border,
				scroll: self.tab_scroll,
				active_tab: self.active,
				compact_tabs: self.preferences.compact_tabs,
				show_plus: self.preferences.show_new_tab_button,
				show_settings: self.preferences.show_settings_button,
			},
		)
	}

	pub(super) fn ensure_active_tab_visible(&mut self) {
		if !self.preferences.show_tabs {
			return;
		}
		let (first_visible, visible_count) = if self.preferences.vertical_tabs {
			let layout = self.vertical_tab_layout();
			(layout.first_visible, layout.visible_count)
		} else {
			let layout = self.tab_layout();
			(layout.first_visible, layout.visible_count)
		};
		let mut first = first_visible;
		if self.active < first {
			first = self.active;
		} else if self.active >= first + visible_count {
			first = self.active + 1 - visible_count;
		}
		self.tab_scroll = first;
	}

	pub(super) fn add_tab(&mut self) {
		self.refresh_directories(true);
		let id = self.next_session_id;
		self.next_session_id += 1;
		let (cols, rows, cwd) = {
			let term = self.active_term();
			(term.cols, term.rows, term.cwd.clone())
		};
		match spawn_session(
			id,
			cols,
			rows,
			self.theme,
			self.proxy.clone(),
			cwd,
			&self.term_env,
		) {
			Ok(session) => {
				self.sessions.push(session);
				self.active = self.sessions.len() - 1;
				self.ensure_active_tab_visible();
				self.selection = None;
				self.end_search();
				self.sessions[self.active].activity = false;
				self.persist_sessions();
				self.update_window_title();
				if let Some(window) = &self.window {
					window.request_redraw();
				}
			}
			Err(error) => eprintln!("poopterm: could not open tab: {error}"),
		}
	}

	pub(super) fn switch_tab(&mut self, offset: isize) {
		let count = self.sessions.len();
		if count == 0 {
			return;
		}
		let index = (self.active as isize + offset).rem_euclid(count as isize) as usize;
		self.activate_tab(index);
	}

	pub(super) fn activate_tab(&mut self, index: usize) {
		if index >= self.sessions.len() {
			return;
		}
		self.active = index;
		self.sessions[self.active].activity = false;
		self.sessions[self.active].term().bell = false;
		self.ensure_active_tab_visible();
		self.selection = None;
		self.update_window_title();
		self.search_matches.clear();
		self.search_match_index = 0;
		if let Some(window) = &self.window {
			window.request_redraw();
		}
		if self.search_active {
			self.search(1, false);
		}
	}

	pub(super) fn close_active_tab(&mut self, event_loop: &ActiveEventLoop) {
		self.close_tab(self.active, event_loop);
	}

	pub(super) fn close_tab(&mut self, index: usize, event_loop: &ActiveEventLoop) {
		if index >= self.sessions.len() {
			return;
		}
		let mut session = self.sessions.remove(index);
		self.notifications.forget(session.id);
		let _ = session._child.kill();
		thread::spawn(move || {
			let _ = session._child.wait();
		});
		self.persist_sessions();
		self.search_matches.clear();
		self.selection = None;
		if self.sessions.is_empty() {
			event_loop.exit();
			return;
		}
		if index < self.active {
			self.active -= 1;
		} else if index == self.active {
			self.active = self.active.min(self.sessions.len() - 1);
		}
		self.tab_scroll = self.tab_scroll.min(self.sessions.len().saturating_sub(1));
		self.ensure_active_tab_visible();
		self.update_window_title();
		if let Some(window) = &self.window {
			window.request_redraw();
		}
	}

	pub(super) fn update_window_title(&mut self) {
		let title = self.sessions[self.active].title();
		if self.reported_title == title {
			return;
		}
		self.reported_title.clone_from(&title);
		if let Some(window) = &self.window {
			window.set_title(&format!("{title} — poopterm"));
		}
	}

	pub(super) fn refresh_directories(&mut self, only_active: bool) {
		for (index, session) in self.sessions.iter().enumerate() {
			if only_active && index != self.active {
				continue;
			}
			if let Some(_pid) = session._child.process_id() {
				#[cfg(target_os = "linux")]
				if let Ok(path) = fs::read_link(format!("/proc/{_pid}/cwd")) {
					session.term().cwd = path;
				}
				#[cfg(target_os = "macos")]
				if let Ok(output) = Command::new("/usr/sbin/lsof")
					.args(["-a", "-p", &_pid.to_string(), "-d", "cwd", "-Fn"])
					.output()
					&& let Some(path) = String::from_utf8_lossy(&output.stdout)
						.lines()
						.find_map(|line| line.strip_prefix('n'))
				{
					session.term().cwd = PathBuf::from(path);
				}
			}
		}
	}

	pub(super) fn persist_sessions(&self) {
		let dirs: Vec<_> = self
			.sessions
			.iter()
			.map(|session| session.term().cwd.clone())
			.collect();
		if let Err(error) = save_session_dirs(&dirs) {
			eprintln!("poopterm: could not save sessions: {error}");
		}
	}
}
