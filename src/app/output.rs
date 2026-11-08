//! Parsing PTY output and deciding when the screen needs a new frame.
use super::*;
use crate::term::iterm::{Attention, WindowRequest};
use std::time::Instant;

impl App {
	/// Reacts to output the parser thread applied to session `id`: clipboard
	/// writes, notifications, tab markers, and a paced redraw if anything
	/// visible changed.
	pub(super) fn process_output(&mut self, id: u64) {
		// Every pane of the active tab is on screen.
		let is_active = self.tab_of(id) == Some(self.active);
		let others = self
			.splits
			.iter_mut()
			.flat_map(|tab| tab.others_mut().iter_mut());
		let Some(session) = self.sessions.iter_mut().chain(others).find(|s| s.id == id) else {
			return;
		};
		session.acknowledge_output();
		let unseen = !is_active || !self.focused;
		let (clipboard, notifications, window_requests, long_command_finished, now, synchronized) = {
			let mut term = session.term();
			let clipboard = std::mem::take(&mut term.clipboard_writes);
			let window_requests = std::mem::take(&mut term.window_requests);
			let notifications = std::mem::take(&mut term.notifications.requests);
			// A long command finished where the user may not be looking: mark
			// the tab and ask the window manager for attention.
			let long_command_finished = term.shell.take_long_command_finished() && unseen;
			if long_command_finished {
				term.bell = true;
			}
			(
				clipboard,
				notifications,
				window_requests,
				long_command_finished,
				Seen::of(&term),
				term.synchronized.is_some(),
			)
		};
		for write in clipboard {
			crate::clipboard::write_from_program(write);
		}
		if long_command_finished
			&& !self.focused
			&& let Some(window) = &self.window
		{
			window.request_user_attention(Some(winit::window::UserAttentionType::Informational));
		}
		let seen = std::mem::replace(&mut session.seen, now);
		let cwd_changed = now.cwd != seen.cwd;
		let mut tab_changed =
			now.bell != seen.bell || now.title != seen.title || now.status != seen.status;
		let visible = is_active && !synchronized;
		if is_active {
			self.pacer.set_flooding(session.flooding());
			session.activity = false;
		} else if !session.activity {
			session.activity = true;
			tab_changed = true;
		}
		if cwd_changed {
			self.persist_sessions();
		}
		if visible {
			if let Some(recorder) = &mut self.frame_stats {
				recorder.stats.output();
			}
			self.update_hovered_link();
			self.search_dirty |= self.search_active;
		}
		if visible || tab_changed {
			self.schedule_redraw();
		}
		if !notifications.is_empty() {
			self.deliver_notifications(id, notifications);
		}
		if !window_requests.is_empty() {
			self.apply_window_requests(id, &window_requests);
		}
	}

	/// Requests a frame now, or at the next frame boundary if one was just drawn.
	pub(super) fn schedule_redraw(&mut self) {
		if self.pacer.request(Instant::now()) {
			self.request_redraw();
		}
	}
}

impl App {
	/// OSC 1337 StealFocus brings session `id` to the front;
	/// RequestAttention asks the window manager to flag the window.
	fn apply_window_requests(&mut self, id: u64, requests: &[WindowRequest]) {
		use winit::window::UserAttentionType;
		for request in requests {
			match request {
				WindowRequest::Focus => {
					if let Some(index) = self.tab_of(id) {
						self.activate_tab(index);
						self.focus_pane(id);
					}
					if let Some(window) = &self.window {
						window.focus_window();
					}
				}
				WindowRequest::Attention(attention) => {
					let attention = attention.map(|attention| match attention {
						Attention::Critical => UserAttentionType::Critical,
						Attention::Informational => UserAttentionType::Informational,
					});
					if let Some(window) = &self.window {
						window.request_user_attention(attention);
					}
				}
			}
		}
	}
}
