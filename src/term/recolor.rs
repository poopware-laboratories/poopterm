//! Recoloring what is already on screen when the theme or the default colors
//! change. Cells hold RGB values, so a color is replaced wherever it shows:
//! text a program drew in palette red follows the new theme's red.
use super::{Cell, Colors, Setting, Term};
use crate::theme::{Theme, ThemeKind};

/// Pairs of an old color and its replacement; the first pair naming a color
/// decides it, so colors listed first (the defaults) win ties.
type Remap = [(u32, u32)];

fn lookup(map: &Remap, color: u32) -> u32 {
	map
		.iter()
		.find(|&&(old, _)| old == color)
		.map_or(color, |&(_, new)| new)
}

impl Term {
	/// Replaces foregrounds through `fg` and backgrounds through `bg` in both
	/// screens, the scrollback and the pen.
	pub(crate) fn remap_colors(&mut self, fg: &Remap, bg: &Remap) {
		if fg.iter().chain(bg).all(|(old, new)| old == new) {
			return;
		}
		let recolor = |cells: &mut [Cell]| {
			for cell in cells {
				cell.fg = lookup(fg, cell.fg);
				cell.bg = lookup(bg, cell.bg);
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
			*saved_fg = lookup(fg, *saved_fg);
			*saved_bg = lookup(bg, *saved_bg);
		}
		self.fg = lookup(fg, self.fg);
		self.sgr_fg = lookup(fg, self.sgr_fg);
		self.bg = lookup(bg, self.bg);
	}

	/// Makes `fg` and `bg` the default colors, without recoloring cells.
	pub(crate) fn adopt_default_colors(&mut self, fg: u32, bg: u32) {
		self.default_fg = fg;
		self.default_bg = bg;
		// Rows are measured against the padding, which just changed.
		let pad = self.padding();
		self.grid.sync_pad(pad);
		if let Some((grid, ..)) = &mut self.primary_screen {
			grid.sync_pad(pad);
		}
	}

	/// The 16 ANSI colors as programs see them: the theme's, under OSC 4.
	fn ansi_colors(&self) -> [u32; 16] {
		std::array::from_fn(|index| self.indexed_color(index as u8))
	}

	/// Switches to `theme`, recoloring text already drawn in the old theme's
	/// defaults or 16 ANSI colors. Colors programs set (OSC 4, 10–19, 21)
	/// stay on top of the new theme.
	pub(crate) fn set_theme(&mut self, theme: Theme) {
		self.finish_sync();
		let old = self.ansi_colors();
		let old_theme = (self.theme_fg, self.theme_bg);
		// A default a program changed (OSC 10, 11) keeps its color.
		let follow = |current: u32, theme_default: u32, new: u32| {
			if current == theme_default {
				new
			} else {
				current
			}
		};
		let fg = follow(self.default_fg, old_theme.0, theme.fg);
		let bg = follow(self.default_bg, old_theme.1, theme.bg);
		self.palette = theme.palette;
		self.bright = theme.bright_colors();
		let mut fg_map = [(self.default_fg, fg); 17];
		let mut bg_map = [(self.default_bg, bg); 17];
		for (index, pair) in old.into_iter().zip(self.ansi_colors()).enumerate() {
			fg_map[index + 1] = pair;
			bg_map[index + 1] = pair;
		}
		self.remap_colors(&fg_map, &bg_map);
		self.adopt_default_colors(fg, bg);
		for saved in &mut self.color_stack {
			saved.default_fg = follow(saved.default_fg, old_theme.0, theme.fg);
			saved.default_bg = follow(saved.default_bg, old_theme.1, theme.bg);
		}
		self.theme_fg = theme.fg;
		self.theme_bg = theme.bg;
		self.theme_cursor = theme.cursor;
		self.theme_selection = theme.selection;
	}

	/// The theme the terminal draws with, as a reset (RIS) restores it.
	pub(crate) fn theme(&self) -> Theme {
		Theme {
			fg: self.theme_fg,
			bg: self.theme_bg,
			palette: self.palette,
			bright: Some(self.bright),
			cursor: self.theme_cursor,
			selection: self.theme_selection,
			..Theme::from_kind(ThemeKind::Poop)
		}
	}

	/// The cursor and selection colors to draw: those programs set, or else
	/// the theme's.
	pub(crate) fn drawn_colors(&self) -> Colors {
		let mut colors = self.colors;
		let theme = |setting: &mut Setting, color: Option<u32>| {
			if let (Setting::Default, Some(color)) = (*setting, color) {
				*setting = Setting::Color(color);
			}
		};
		theme(&mut colors.cursor, self.theme_cursor);
		let (selection_fg, selection_bg) = self.theme_selection.unzip();
		theme(&mut colors.selection_foreground, selection_fg);
		theme(&mut colors.selection_background, selection_bg);
		colors
	}
}
