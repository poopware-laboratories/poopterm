//! Closing panes and tabs through the prompt that asks first when a
//! program other than a shell is running in them. Every decision is in
//! `ui::close_prompt`; these methods hand it the live sessions and keys.
use super::*;
use crate::ui::close_prompt::{Answer, CloseTarget, answer, close_request, ends};

impl App {
	/// Closes `target` at once, or asks first when a program runs there.
	pub(super) fn request_close(&mut self, target: CloseTarget, event_loop: &ActiveEventLoop) {
		// pr-check: window glue: asks the live sessions the tested model needs
		let ended = |session: &&Session| ends(target, session.id, |id| self.tab_of(id));
		let sessions = self.all_sessions().filter(ended);
		let running: Vec<String> = sessions.filter_map(Session::running_program).collect();
		let Some(request) = close_request(target, &running) else {
			return self.close_now(target, event_loop);
		};
		// pr-check: end window glue
		// pr-check: window glue: shows the prompt the tested model asked for
		self.end_search();
		(self.command_palette, self.renaming_tab) = (true, false);
		self.closing = Some(request);
		self.request_redraw();
		// pr-check: end window glue
	}

	/// Closes tab `index` with all its panes, asking first as for a pane.
	pub(super) fn request_close_tab(&mut self, index: usize, event_loop: &ActiveEventLoop) {
		// pr-check: window glue: a tab button names its tab by a session in it
		if let Some(session) = self.sessions.get(index) {
			self.request_close(CloseTarget::Tab(session.id), event_loop);
		}
		// pr-check: end window glue
	}

	fn close_now(&mut self, target: CloseTarget, event_loop: &ActiveEventLoop) {
		// pr-check: window glue: ends the sessions the tested target names
		match target {
			CloseTarget::Pane(id) => self.close_pane(id, event_loop),
			CloseTarget::Tab(id) => {
				if let Some(tab) = self.tab_of(id) {
					self.close_tab(tab, event_loop);
				}
			}
		}
		// pr-check: end window glue
	}

	/// Answers an open close prompt with `event`'s key; false when none is
	/// open, so the key goes on.
	pub(super) fn answer_close(&mut self, key: &Key, event_loop: &ActiveEventLoop) -> bool {
		// pr-check: window glue: hands a real key to the tested answer
		let Some(target) = self.closing.as_ref().map(|request| request.target) else {
			return false;
		};
		let chord = (self.shortcuts.close_tab).matches(key, self.mods);
		match answer(key, chord) {
			Answer::Wait => return true,
			Answer::Close => self.close_now(target, event_loop),
			Answer::Keep => {}
		}
		(self.closing, self.command_palette) = (None, false);
		self.request_redraw();
		true
		// pr-check: end window glue
	}
}
