//! Scrollback search state for the active tab.
use super::*;
use crate::ui::{
	overlays::Place,
	search_controls::{SearchControl, control_at, inside_panel},
};

impl App {
	pub(super) fn search(&mut self, direction: isize, advance: bool) {
		self.search_dirty = false;
		let matches = find_matches(
			&self.active_term(),
			&self.search_query,
			self.search_case_sensitive,
		);
		self.search_matches = matches;
		if self.search_matches.is_empty() {
			self.search_match_index = 0;
			if let Some(window) = &self.window {
				window.request_redraw();
			}
			return;
		}
		let len = self.search_matches.len();
		if advance {
			self.search_match_index =
				(self.search_match_index as isize + direction).rem_euclid(len as isize) as usize;
		} else {
			self.search_match_index = 0;
		}
		let match_range = self.search_matches[self.search_match_index];
		let mut term = self.active_term();
		let visible_top = match_range.start.row.saturating_sub(term.rows / 2);
		term.scroll_offset = term
			.scrollback
			.len()
			.saturating_sub(visible_top.min(term.scrollback.len()));
		drop(term);
		if let Some(window) = &self.window {
			window.request_redraw();
		}
	}

	/// Re-runs the search after new output, keeping the selected match if it still exists.
	pub(super) fn refresh_search_if_dirty(&mut self) {
		if !(self.search_active && self.search_dirty) {
			return;
		}
		let previous = self
			.search_matches
			.get(self.search_match_index)
			.map(|m| m.start);
		let matches = find_matches(
			&self.active_term(),
			&self.search_query,
			self.search_case_sensitive,
		);
		self.search_matches = matches;
		self.search_match_index = previous
			.and_then(|point| self.search_matches.iter().position(|m| m.start == point))
			.unwrap_or(0);
		self.search_dirty = false;
	}

	pub(super) fn begin_search(&mut self) {
		self.search_active = true;
		self.search_query.clear();
		self.search_matches.clear();
		self.search_match_index = 0;
		self.search(1, false);
	}

	pub(super) fn end_search(&mut self) {
		self.search_active = false;
		self.search_matches.clear();
		self.search_query.clear();
		if let Some(window) = &self.window {
			window.request_redraw();
		}
	}

	/// Where the search box floats, in the window's current size.
	fn search_place(&self) -> Option<Place> {
		// pr-check: window glue: reads the live window's size for the tested geometry
		let size = self.window.as_ref()?.inner_size();
		Some(Place {
			window: (size.width as usize, size.height as usize),
			top: self.top,
			scale: self.scale(),
		})
		// pr-check: end window glue
	}

	/// Follows the pointer over the search box's buttons, redrawing when the
	/// lit one changes.
	pub(super) fn hover_search(&mut self) {
		// pr-check: window glue: stores the tested hit-test's answer and asks for a redraw
		let hover = (self.search_active)
			.then(|| self.search_place())
			.flatten()
			.and_then(|place| control_at(place, self.cursor_pos));
		if hover != self.search_hover {
			self.search_hover = hover;
			self.request_redraw();
		}
		// pr-check: end window glue
	}

	/// Handles a left press on the search box: a button does its work, and
	/// the rest of the box swallows the press. Whether it was on the box.
	pub(super) fn press_search(&mut self) -> bool {
		let Some(place) = (self.search_active).then(|| self.search_place()).flatten() else {
			return false;
		};
		if let Some(control) = control_at(place, self.cursor_pos) {
			// pr-check: window glue: each button does what its glyph says, through the tested search
			match control {
				SearchControl::Previous => self.search(-1, true),
				SearchControl::Next => self.search(1, true),
				SearchControl::Case => {
					self.search_case_sensitive = !self.search_case_sensitive;
					self.search(1, false);
				}
				SearchControl::Close => self.end_search(),
			}
			// pr-check: end window glue
			self.request_redraw();
			return true;
		}
		inside_panel(place, self.cursor_pos)
	}
}
