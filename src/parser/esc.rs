//! Two-byte `ESC` sequences: charsets, index, reset and cursor save.
use crate::term::{Cell, Term};

impl Term {
	pub(super) fn apply_esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		if ignore {
			return;
		}
		if matches!(intermediates, b"(" | b")") {
			if matches!(byte, b'0' | b'B') {
				self.charsets[usize::from(intermediates == b")")] = byte == b'0';
			}
			return;
		}
		if intermediates == b"#" {
			if byte == b'8' {
				self.screen_alignment();
			}
			return;
		}
		if !intermediates.is_empty() {
			return;
		}
		if !matches!(byte, b'7' | b'8' | b'=' | b'>') {
			self.wrap_pending = false;
		}
		match byte {
			b'H' => self.tab_stops[self.cx] = true,
			b'c' => {
				let theme = self.theme();
				let (cw, ch, scale) = (self.cell_width, self.cell_height, self.scale);
				let limit = self.scrollback_limit;
				let contexts = std::mem::take(&mut self.contexts);
				*self = Self::new(self.cols, self.rows, theme, self.cwd.clone());
				self.contexts = contexts;
				self.cell_width = cw;
				self.cell_height = ch;
				self.scale = scale;
				self.scrollback_limit = limit;
			}
			b'M' => {
				if self.cy == self.scroll_top {
					self.scroll_region_down(1);
				} else {
					self.cy = self.cy.saturating_sub(1);
				}
			}
			// HP memory lock: the lines above the cursor stop scrolling.
			b'l' if self.cy < self.scroll_bottom => {
				self.scroll_top = self.cy;
				self.memory_locked = true;
			}
			b'm' if self.memory_locked => {
				self.scroll_top = 0;
				self.memory_locked = false;
			}
			b'D' => self.linefeed(),
			b'E' => {
				self.cx = 0;
				self.linefeed();
			}
			// DECKPAM and DECKPNM.
			b'=' => self.application_keypad = true,
			b'>' => self.application_keypad = false,
			b'7' => self.save_cursor(),
			b'8' => self.restore_cursor(),
			_ => {}
		}
	}
}

impl Term {
	/// DECALN: fills the screen with `E` in the default rendition, resets the
	/// margins and homes the cursor.
	fn screen_alignment(&mut self) {
		self.erase_images(0, self.rows * self.cols);
		self.clear_kitty_placements(false);
		let cell = Cell {
			ch: 'E',
			fg: self.default_fg,
			bg: self.default_bg,
			..Cell::default()
		};
		for row in self.grid.iter_mut() {
			Cell::fill(row, cell);
		}
		self.scroll_top = 0;
		self.scroll_bottom = self.rows - 1;
		self.cx = 0;
		self.cy = 0;
		self.wrap_pending = false;
	}
}
