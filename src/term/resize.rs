//! What a resize does to the rows: a screen made shorter than the cursor's
//! row sends the lines above it into history, as a line feed at the
//! bottom would, and a taller screen takes them back from history, so the
//! cursor stays on its line and turning a phone (or dragging a window)
//! shorter and back leaves the screen as it was. While a full-screen
//! program runs, the same is done to the shell's saved screen.
use super::Term;

impl Term {
	/// Before the screen shrinks to `rows`: the lines above the cursor that
	/// would not fit go to history.
	pub(super) fn keep_cursor_line(&mut self, rows: usize) {
		self.on_shell_screen(|term| {
			let push = (term.cy + 1).saturating_sub(rows);
			if push > 0 {
				term.scroll_region_up(push);
				term.cy -= push;
			}
		});
	}

	/// After the screen grew from `old` rows: as many lines as there is
	/// room for come back from history, above what is on screen.
	pub(super) fn take_back_history(&mut self, old: usize) {
		self.on_shell_screen(|term| {
			let pull = term.rows.saturating_sub(old).min(term.scrollback.len());
			if pull == 0 {
				return;
			}
			let pad = term.padding();
			term.grid.scroll_down(0, term.rows - 1, pull, pad);
			for y in (0..pull).rev() {
				let Some(line) = term.scrollback.pop_back() else {
					break;
				};
				term.grid.row_upto(y, line.len())[..line.len()].copy_from_slice(&line);
			}
			term.cy += pull;
			term.scroll_images(pull as isize);
			term.scroll_offset = term.scroll_offset.min(term.scrollback.len());
		});
	}

	/// Runs `change` on the shell's screen: the one shown, or, while a
	/// full-screen program has the alternate screen, the one saved for its
	/// return, swapped in with its cursor and images for the call.
	fn on_shell_screen(&mut self, change: impl FnOnce(&mut Term)) {
		let Some((grid, cx, cy, saved_a, saved_b)) = self.primary_screen.take() else {
			return change(self);
		};
		let alternate = std::mem::replace(&mut self.grid, grid);
		let cursor = (self.cx, self.cy);
		(self.cx, self.cy) = (cx, cy);
		std::mem::swap(&mut self.images, &mut self.primary_images);
		change(self);
		std::mem::swap(&mut self.images, &mut self.primary_images);
		let grid = std::mem::replace(&mut self.grid, alternate);
		self.primary_screen = Some((grid, self.cx, self.cy, saved_a, saved_b));
		(self.cx, self.cy) = cursor;
	}
}
