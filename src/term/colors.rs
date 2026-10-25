//! Colors programs set on top of the theme: xterm's dynamic colors (OSC
//! 10–19) and special colors (OSC 5/6), kitty's extra keys (OSC 21), and the
//! color stack (XTPUSHCOLORS, kitty OSC 30001/30101).
use super::Term;

/// Saved palettes kept by the color stack, as in xterm.
pub(crate) const STACK_DEPTH: usize = 10;

/// How a color that the theme or the renderer can also choose is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Setting {
	/// Never set: the terminal's own choice.
	#[default]
	Default,
	/// Follows the text (kitty's empty value): reverse video for
	/// selections, the cell's colors for the cursor.
	Dynamic,
	Color(u32),
}

impl Setting {
	pub(crate) fn or(self, default: u32, dynamic: u32) -> u32 {
		match self {
			Self::Default => default,
			Self::Dynamic => dynamic,
			Self::Color(color) => color,
		}
	}
}

/// The dynamic colors xterm numbers OSC 10 to 19, in that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dynamic {
	Foreground,
	Background,
	Cursor,
	PointerForeground,
	PointerBackground,
	TektronixForeground,
	TektronixBackground,
	HighlightBackground,
	TektronixCursor,
	HighlightForeground,
}

impl Dynamic {
	/// The color OSC `10 + index` sets.
	pub(crate) fn from_index(index: usize) -> Option<Self> {
		use Dynamic::*;
		[
			Foreground,
			Background,
			Cursor,
			PointerForeground,
			PointerBackground,
			TektronixForeground,
			TektronixBackground,
			HighlightBackground,
			TektronixCursor,
			HighlightForeground,
		]
		.get(index)
		.copied()
	}
}

/// Colors besides the default foreground, background and the 256-color
/// table, which live on [`Term`] directly.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Colors {
	pub cursor: Setting,
	pub cursor_text: Setting,
	pub selection_background: Setting,
	pub selection_foreground: Setting,
	pub visual_bell: Setting,
	pub pointer_foreground: Option<u32>,
	pub pointer_background: Option<u32>,
	/// Tektronix foreground, background and cursor; `None` follows the
	/// VT100 colors.
	pub tektronix: [Option<u32>; 3],
	/// Colors for bold, underline, blink, reverse and italic text (OSC 5).
	pub special: [Option<u32>; 5],
	/// Whether each special color is used, and (last) whether they also
	/// replace colors set by SGR (OSC 6/106).
	pub special_modes: [bool; 6],
	/// kitty's `transparent_background_color1..7`, with their opacity.
	pub transparent: [Option<(u32, f32)>; 7],
}

/// Everything the color stack saves and restores.
#[derive(Clone)]
pub(crate) struct Saved {
	default_fg: u32,
	default_bg: u32,
	overrides: [Option<u32>; 256],
	colors: Colors,
}

impl Term {
	pub(crate) fn save_colors(&self) -> Saved {
		Saved {
			default_fg: self.default_fg,
			default_bg: self.default_bg,
			overrides: self.color_overrides,
			colors: self.colors,
		}
	}

	pub(crate) fn restore_colors(&mut self, saved: &Saved) {
		self.set_default_colors(saved.default_fg, saved.default_bg);
		self.color_overrides = saved.overrides;
		self.colors = saved.colors;
	}

	/// XTPUSHCOLORS: `slot` 0 pushes; 1 to 10 stores into that slot.
	pub(crate) fn push_colors(&mut self, slot: usize) {
		let saved = self.save_colors();
		match slot {
			0 if self.color_stack.len() < STACK_DEPTH => self.color_stack.push(saved),
			1..=STACK_DEPTH if slot <= self.color_stack.len() => self.color_stack[slot - 1] = saved,
			1..=STACK_DEPTH if slot == self.color_stack.len() + 1 => self.color_stack.push(saved),
			_ => {}
		}
	}

	/// XTPOPCOLORS: `slot` 0 pops; 1 to 10 restores that slot and keeps it.
	pub(crate) fn pop_colors(&mut self, slot: usize) {
		let saved = match slot {
			0 => self.color_stack.pop(),
			1..=STACK_DEPTH => self.color_stack.get(slot - 1).cloned(),
			_ => None,
		};
		if let Some(saved) = saved {
			self.restore_colors(&saved);
		}
	}

	pub(crate) fn special_colors_active(&self) -> bool {
		self.colors.special_modes[..5].contains(&true)
	}

	/// Shows text with the special color of its first enabled attribute:
	/// only default-colored text, unless colorAttrMode (the sixth mode) is on.
	pub(crate) fn apply_special_colors(&mut self) {
		use super::attr::{BLINK, ITALIC, UNDERLINE};
		self.fg = self.sgr_fg;
		let modes = self.colors.special_modes;
		if self.fg_id != 0 && !modes[5] {
			return;
		}
		let active = [
			self.bold,
			self.attrs & UNDERLINE != 0,
			self.attrs & BLINK != 0,
			self.reverse,
			self.attrs & ITALIC != 0,
		];
		if let Some(color) = (0..5)
			.filter(|&index| modes[index] && active[index])
			.find_map(|index| self.colors.special[index])
		{
			self.fg = color;
		}
	}

	/// The current dynamic color `which`, as a query reports it.
	pub(crate) fn dynamic_color(&self, which: Dynamic) -> u32 {
		let colors = &self.colors;
		let (selection_fg, selection_bg) = crate::ui::contrast::selection_colors(self.default_bg);
		match which {
			Dynamic::Foreground => self.default_fg,
			Dynamic::Background => self.default_bg,
			Dynamic::Cursor => colors.cursor.or(self.default_fg, self.default_fg),
			Dynamic::PointerForeground => colors.pointer_foreground.unwrap_or(self.default_fg),
			Dynamic::PointerBackground => colors.pointer_background.unwrap_or(self.default_bg),
			Dynamic::TektronixForeground => colors.tektronix[0].unwrap_or(self.default_fg),
			Dynamic::TektronixBackground => colors.tektronix[1].unwrap_or(self.default_bg),
			Dynamic::TektronixCursor => colors.tektronix[2].unwrap_or(self.default_fg),
			Dynamic::HighlightBackground => colors
				.selection_background
				.or(selection_bg, self.default_fg),
			Dynamic::HighlightForeground => colors
				.selection_foreground
				.or(selection_fg, self.default_bg),
		}
	}

	/// Sets dynamic color `which`; `None` resets it (OSC 110–119).
	pub(crate) fn set_dynamic_color(&mut self, which: Dynamic, color: Option<u32>) {
		let setting = color.map_or(Setting::Default, Setting::Color);
		let colors = &mut self.colors;
		match which {
			Dynamic::Foreground => {
				let fg = color.unwrap_or(self.theme_fg);
				self.set_default_colors(fg, self.default_bg);
			}
			Dynamic::Background => {
				let bg = color.unwrap_or(self.theme_bg);
				self.set_default_colors(self.default_fg, bg);
			}
			Dynamic::Cursor => colors.cursor = setting,
			Dynamic::PointerForeground => colors.pointer_foreground = color,
			Dynamic::PointerBackground => colors.pointer_background = color,
			Dynamic::TektronixForeground => colors.tektronix[0] = color,
			Dynamic::TektronixBackground => colors.tektronix[1] = color,
			Dynamic::TektronixCursor => colors.tektronix[2] = color,
			Dynamic::HighlightBackground => colors.selection_background = setting,
			Dynamic::HighlightForeground => colors.selection_foreground = setting,
		}
	}

	/// Changes the default colors, recoloring cells that show the old ones.
	pub(crate) fn set_default_colors(&mut self, fg: u32, bg: u32) {
		let (old_fg, old_bg) = (self.default_fg, self.default_bg);
		if (old_fg, old_bg) == (fg, bg) {
			return;
		}
		let recolor = |cells: &mut [super::Cell]| {
			for cell in cells {
				if cell.fg == old_fg {
					cell.fg = fg;
				}
				if cell.bg == old_bg {
					cell.bg = bg;
				}
			}
		};
		for row in self.grid.iter_mut() {
			recolor(row);
		}
		recolor(self.scrollback.cells_mut());
		if let Some((grid, _, _, saved_fg, saved_bg)) = &mut self.primary_screen {
			for row in grid.iter_mut() {
				recolor(row);
			}
			if *saved_fg == old_fg {
				*saved_fg = fg;
			}
			if *saved_bg == old_bg {
				*saved_bg = bg;
			}
		}
		if self.fg == old_fg {
			self.fg = fg;
		}
		if self.bg == old_bg {
			self.bg = bg;
		}
		self.default_fg = fg;
		self.default_bg = bg;
		// Rows are measured against the padding, which just changed.
		let pad = self.padding();
		self.grid.sync_pad(pad);
		if let Some((grid, ..)) = &mut self.primary_screen {
			grid.sync_pad(pad);
		}
	}
}
