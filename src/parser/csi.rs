//! Control Sequence Introducer (`CSI`) dispatch: cursor, erase, scroll and modes.
use super::params::ParamList;
use crate::input::mouse::MouseEncoding;
use crate::term::{CursorShape, Term};

impl Term {
	#[inline(always)]
	pub(super) fn apply_csi_dispatch(
		&mut self,
		params: &(impl ParamList + ?Sized),
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		if ignore {
			return;
		}
		let private = intermediates == b"?";
		if !intermediates.is_empty() && !private {
			if action == 'u' && intermediates == b">" {
				self.kitty_keyboard = params.iter().any(|p| p.first() == Some(&1));
				return;
			}
			if action == 'c' && intermediates == b">" {
				// Secondary DA: terminal type 0, version as MMmmpp, ROM cartridge 1.
				self
					.responses
					.push(format!("\x1b[>0;{};1c", version_number()).into_bytes());
				return;
			}
			if action == 'u' && intermediates == b"<" {
				self.kitty_keyboard = false;
				return;
			}
			match (intermediates, action) {
				// DECSCUSR: 0-2 block, 3-4 underline, 5-6 bar.
				(b" ", 'q') => {
					self.cursor_shape = match params.iter().next().and_then(|p| p.first().copied()) {
						Some(3 | 4) => CursorShape::Underline,
						Some(5 | 6) => CursorShape::Bar,
						_ => CursorShape::Block,
					}
				}
				// XTVERSION
				(b">", 'q') => self
					.responses
					.push(format!("\x1bP>|poopterm {}\x1b\\", env!("CARGO_PKG_VERSION")).into_bytes()),
				// DECSTR
				(b"!", 'p') => self.soft_reset(),
				// XTMODKEYS: a resource (with an optional mask subparameter)
				// and its value; no parameters reset them all.
				(b">", 'm') => match params.get(0) {
					Some(resource) if !params.is_default() => {
						let value = params.get(1).and_then(|p| p.first().copied());
						let options = &mut self.key_options;
						options.set_modify(resource[0], resource.get(1).copied(), value);
					}
					_ => self.key_options.reset_modify(),
				},
				(b">", 'n') => {
					let index = params.get(0).filter(|_| !params.is_default());
					self.key_options.disable(index.map(|p| p[0]));
				}
				// XTFMTKEYS
				(b">", 'f') => {
					let [index, value] = [0, 1].map(|i| params.get(i).map(|p| p[0]));
					let index = index.filter(|_| !params.is_default());
					self.key_options.set_format(index, value);
				}
				// XTPUSHCOLORS, XTPOPCOLORS: each parameter is a slot; 0 or
				// none pushes or pops.
				(b"#", 'P' | 'Q') => {
					let slots: Vec<usize> = params
						.iter()
						.map(|p| usize::from(p.first().copied().unwrap_or(0)))
						.collect();
					for slot in if slots.is_empty() { vec![0] } else { slots } {
						if action == 'P' {
							self.push_colors(slot);
						} else {
							self.pop_colors(slot);
						}
					}
				}
				// XTREPORTCOLORS: the current entry and the number saved.
				(b"#", 'R') => {
					let saved = self.color_stack.len();
					self
						.responses
						.push(format!("\x1b[{saved};{saved}#Q").into_bytes());
				}
				_ => {}
			}
			return;
		}
		if !matches!(action, 'm' | 'n' | 'c' | 'h' | 'l' | 's' | 'u' | 't') {
			self.wrap_pending = false;
		}

		// These sequences read at most two parameters; 0 and a missing value
		// both mean the default `d`.
		let leading = [0, 1].map(|i| params.get(i).and_then(|s| s.first().copied()).unwrap_or(0));
		let p = |i: usize, d: u16| usize::from(if leading[i] == 0 { d } else { leading[i] });
		let raw0 = leading[0];

		if private && matches!(action, 'h' | 'l') {
			let enabled = action == 'h';
			for p in params.iter().filter_map(|s| s.first().copied()) {
				match p {
					1 => self.application_cursor = enabled,
					66 => self.application_keypad = enabled,
					6 => {
						self.origin_mode = enabled;
						self.cx = 0;
						self.cy = if enabled { self.scroll_top } else { 0 };
						self.wrap_pending = false;
					}
					5 => self.reverse_screen = enabled,
					7 => self.auto_wrap = enabled,
					45 => self.reverse_wrap = enabled,
					25 => self.cursor_visible = enabled,
					80 => self.sixel_scrolling = enabled,
					1048 => {
						if enabled {
							self.save_cursor();
						} else {
							self.restore_cursor();
						}
					}
					1049 | 47 | 1047 => self.alternate_screen(enabled),
					9 | 1000 | 1002 | 1003 => self.mouse_tracking = if enabled { p } else { 0 },
					1004 => self.focus_reporting = enabled,
					1007 => self.alternate_scroll = enabled,
					1005 | 1006 | 1015 | 1016 => {
						let encoding = MouseEncoding::from_mode(p).unwrap_or_default();
						if enabled {
							self.mouse_encoding = encoding;
						} else if self.mouse_encoding == encoding {
							self.mouse_encoding = MouseEncoding::X10;
						}
					}
					2004 => self.bracketed_paste = enabled,
					_ => self
						.keyboard
						.mode(p)
						.into_iter()
						.for_each(|flag| *flag = enabled),
				}
			}
			return;
		}
		// XTSAVE and XTRESTORE.
		if private && matches!(action, 's' | 'r') {
			let modes = params.iter().filter_map(|s| s.first().copied());
			if action == 's' {
				self.save_private_modes(modes);
			} else {
				self.restore_private_modes(modes);
			}
			return;
		}
		// XTQMODKEYS and XTQFMTKEYS.
		if private && matches!(action, 'm' | 'g') {
			let options = &self.key_options;
			let reply = params.get(0).and_then(|p| match action {
				'm' => options.modify_reply(p[0]),
				_ => options.format_reply(p[0]),
			});
			self.responses.extend(reply.map(String::into_bytes));
			return;
		}
		if private && action == 'u' {
			self
				.responses
				.push(format!("\x1b[?{}u", if self.kitty_keyboard { 1 } else { 0 }).into_bytes());
			return;
		}

		if private {
			return;
		}
		if matches!(action, 'h' | 'l') {
			let enabled = action == 'h';
			for mode in params.iter().filter_map(|param| param.first().copied()) {
				self.set_ansi_mode(mode, enabled);
			}
			return;
		}
		match action {
			'g' => match raw0 {
				0 => self.tab_stops[self.cx] = false,
				3 => self.tab_stops.fill(false),
				_ => {}
			},
			'I' | 'Z' => {
				for _ in 0..p(0, 1).min(self.cols) {
					self.cx = if action == 'I' {
						(self.cx + 1..self.cols)
							.find(|&col| self.tab_stops[col])
							.unwrap_or(self.cols - 1)
					} else {
						(0..self.cx)
							.rev()
							.find(|&col| self.tab_stops[col])
							.unwrap_or(0)
					};
				}
			}
			'd' => {
				self.cy = (p(0, 1) - 1 + if self.origin_mode { self.scroll_top } else { 0 }).min(
					if self.origin_mode {
						self.scroll_bottom
					} else {
						self.rows - 1
					},
				);
			}
			'E' | 'F' => {
				let top = if self.origin_mode { self.scroll_top } else { 0 };
				let bottom = if self.origin_mode {
					self.scroll_bottom
				} else {
					self.rows - 1
				};
				self.cy = if action == 'E' {
					(self.cy + p(0, 1)).min(bottom)
				} else {
					self.cy.saturating_sub(p(0, 1)).max(top)
				};
				self.cx = 0;
			}
			// CUU, and VPB from ECMA-48.
			'A' | 'k' => {
				let top = if self.origin_mode { self.scroll_top } else { 0 };
				self.cy = self.cy.saturating_sub(p(0, 1)).max(top);
			}
			// CUD and VPR.
			'B' | 'e' => {
				self.cy = (self.cy + p(0, 1)).min(if self.origin_mode {
					self.scroll_bottom
				} else {
					self.rows - 1
				})
			}
			// CUF and HPR.
			'C' | 'a' => self.cx = (self.cx + p(0, 1)).min(self.cols - 1),
			// CUB, and HPB from ECMA-48.
			'D' | 'j' => self.cx = self.cx.saturating_sub(p(0, 1)),
			'G' | '`' => self.cx = (p(0, 1) - 1).min(self.cols - 1),
			'H' | 'f' => {
				let row = p(0, 1) - 1 + if self.origin_mode { self.scroll_top } else { 0 };
				self.cy = row.min(if self.origin_mode {
					self.scroll_bottom
				} else {
					self.rows - 1
				});
				self.cx = (p(1, 1) - 1).min(self.cols - 1);
			}
			'r' => {
				let top = (p(0, 1) - 1).min(self.rows - 1);
				let bottom = (p(1, self.rows as u16) - 1).min(self.rows - 1);
				if top < bottom {
					self.scroll_top = top;
					self.scroll_bottom = bottom;
					self.memory_locked = false;
					self.cx = 0;
					self.cy = if self.origin_mode { top } else { 0 };
				}
			}
			'S' => self.scroll_region_up(p(0, 1)),
			'T' => self.scroll_region_down(p(0, 1)),
			's' => self.save_cursor(),
			'u' => self.restore_cursor(),
			'J' => match raw0 {
				0 => {
					let from = self.idx(self.cx, self.cy);
					self.clear_range(from, self.rows * self.cols);
				}
				1 => self.clear_range(0, self.idx(self.cx + 1, self.cy)),
				2 => {
					self.clear_range(0, self.rows * self.cols);
					self.clear_kitty_placements(false);
				}
				3 => self.clear_scrollback(),
				_ => {}
			},
			'K' => self.erase_line(raw0),
			'@' => {
				self.erase_images(self.cy * self.cols, (self.cy + 1) * self.cols);
				let count = p(0, 1).min(self.cols - self.cx);
				let (x, cols) = (self.cx, self.cols);
				self.erase_multicells_for_shift(x, self.cy, count, true);
				self.grid[self.cy].copy_within(x..cols - count, x + count);
				self.clear_row(self.cy, x, x + count);
			}
			'P' => {
				self.erase_images(self.cy * self.cols, (self.cy + 1) * self.cols);
				let count = p(0, 1).min(self.cols - self.cx);
				let (x, cols) = (self.cx, self.cols);
				self.erase_multicells_for_shift(x, self.cy, count, false);
				self.grid[self.cy].copy_within(x + count..cols, x);
				self.clear_row(self.cy, cols - count, cols);
			}
			'X' => {
				let count = p(0, 1).min(self.cols - self.cx);
				self.clear_row(self.cy, self.cx, self.cx + count);
			}
			'L' | 'M' if self.cy >= self.scroll_top && self.cy <= self.scroll_bottom => {
				self.erase_images(self.cy * self.cols, (self.scroll_bottom + 1) * self.cols);
				let bottom = self.scroll_bottom.max(self.cy).min(self.rows - 1);
				let count = p(0, 1).min(bottom + 1 - self.cy);
				// Blocks that lines moving apart would cut are erased.
				let (y, edge) = if action == 'L' {
					(self.cy, bottom + 1 - count)
				} else {
					(self.cy, self.cy + count)
				};
				self.split_multicells_above(&[y, edge, bottom + 1]);
				let blank = self.blank();
				if action == 'L' {
					self.grid.scroll_down(self.cy, bottom, count, blank);
				} else {
					self
						.grid
						.scroll_up(self.cy, bottom, count, blank, |row, _| row.len());
				}
			}
			'm' => self.sgr(params),
			'n' => match raw0 {
				5 => self.responses.push(b"\x1b[0n".to_vec()),
				6 => self
					.responses
					.push(format!("\x1b[{};{}R", self.cy + 1, self.cx.min(self.cols - 1) + 1).into_bytes()),
				_ => {}
			},
			'c' => self.responses.push(b"\x1b[?62;4;6c".to_vec()),
			't' if !private => self.window_op(leading[0], leading[1]),
			'b' => {
				if let Some(c) = self.last_printed {
					for _ in 0..p(0, 1).min(self.cols * self.rows) {
						self.apply_print(c);
					}
				}
			}
			_ => {}
		}
		// Erasing or shifting can split a wide character from its spacer.
		match action {
			'@' | 'P' | 'X' => self.repair_wide_rows(self.cy, self.cy + 1),
			// Rows other than the cursor's are erased completely.
			'J' if raw0 <= 1 => self.repair_wide_rows(self.cy, self.cy + 1),
			_ => {}
		}
	}
}

impl Term {
	/// The state of a DEC private mode for DECRQM, or `None` if unsupported.
	pub(crate) fn private_mode(&self, mode: u16) -> Option<bool> {
		Some(match mode {
			1 => self.application_cursor,
			66 => self.application_keypad,
			6 => self.origin_mode,
			5 => self.reverse_screen,
			7 => self.auto_wrap,
			45 => self.reverse_wrap,
			25 => self.cursor_visible,
			47 | 1047 | 1049 => self.primary_screen.is_some(),
			80 => self.sixel_scrolling,
			9 | 1000 | 1002 | 1003 => self.mouse_tracking == mode,
			1004 => self.focus_reporting,
			1007 => self.alternate_scroll,
			1005 | 1006 | 1015 | 1016 => self.mouse_encoding.mode() == Some(mode),
			2004 => self.bracketed_paste,
			_ => return self.keyboard.clone().mode(mode).copied(),
		})
	}
}

/// The crate version encoded as `major * 10000 + minor * 100 + patch`.
fn version_number() -> u32 {
	let mut parts = env!("CARGO_PKG_VERSION")
		.split(['.', '-'])
		.map(|part| part.parse::<u32>().unwrap_or(0));
	let mut next = || parts.next().unwrap_or(0);
	next() * 10_000 + next() * 100 + next()
}

impl Term {
	/// DECSTR: resets modes, margins, character sets and attributes without
	/// clearing the screen or moving the cursor.
	fn soft_reset(&mut self) {
		self.cursor_visible = true;
		self.insert_mode = false;
		self.keyboard_locked = false;
		self.origin_mode = false;
		self.auto_wrap = true;
		self.application_cursor = false;
		self.application_keypad = false;
		self.scroll_top = 0;
		self.scroll_bottom = self.rows - 1;
		self.charsets = [false; 2];
		self.active_charset = 0;
		self.reset_style();
		self.saved_cursor = None;
		self.wrap_pending = false;
	}
}
