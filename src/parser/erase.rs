//! Line erasure, repairing only wide characters cut by the erased boundary.
use crate::term::Term;

impl Term {
	pub(super) fn erase_line(&mut self, mode: u16) {
		let (y, cols) = (self.cy, self.cols);
		match mode {
			0 => {
				self.clear_row(y, self.cx, cols);
				if self.cx > 0 && self.grid[self.cy][self.cx - 1].width == 2 {
					let cell = &mut self.grid[self.cy][self.cx - 1];
					cell.ch = ' ';
					cell.width = 1;
				}
			}
			1 => {
				self.clear_row(y, 0, self.cx + 1);
				if self.cx + 1 < self.cols && self.grid[self.cy][self.cx + 1].width == 0 {
					let cell = &mut self.grid[self.cy][self.cx + 1];
					cell.ch = ' ';
					cell.width = 1;
				}
			}
			2 => self.clear_row(y, 0, cols),
			_ => {}
		}
	}
}
