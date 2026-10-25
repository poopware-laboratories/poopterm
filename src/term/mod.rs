//! Terminal grid, scrollback, cursor and screen state.
#[cfg(test)]
mod click_events_tests;
pub(crate) mod clipboard;
#[cfg(test)]
mod clipboard_tests;
#[cfg(test)]
mod color_tests;
mod colors;
#[cfg(test)]
mod conemu_tests;
#[cfg(test)]
mod context_tests;
pub(crate) mod contexts;
#[cfg(test)]
mod cwd_host_tests;
pub(crate) mod dcs;
#[cfg(test)]
mod dcs_tests;
mod extras;
#[cfg(target_arch = "x86_64")]
mod fill;
#[cfg(test)]
mod fill_tests;
mod frame;
mod grid;
#[cfg(test)]
mod grid_extent_tests;
#[cfg(test)]
mod history_tests;
mod images;
pub(crate) mod iterm;
#[cfg(test)]
mod iterm_image_tests;
mod iterm_images;
#[cfg(test)]
mod iterm_tests;
#[cfg(test)]
mod key_options_tests;
#[cfg(test)]
mod keyboard_mode_tests;
mod kitty;
mod kitty_animation;
#[cfg(test)]
mod kitty_animation_tests;
#[cfg(test)]
mod kitty_tests;
#[cfg(test)]
mod memory_lock_tests;
#[cfg(test)]
mod mode_tests;
#[cfg(test)]
mod movement_tests;
pub(crate) mod multicell;
#[cfg(test)]
mod multicell_tests;
#[cfg(test)]
mod notification_tests;
pub(crate) mod notifications;
pub(crate) mod pointer;
#[cfg(test)]
mod pointer_tests;
#[cfg(test)]
mod prompt_tests;
#[cfg(test)]
mod reverse_screen_tests;
#[cfg(test)]
mod reverse_wrap_tests;
mod screen;
mod scrollback;
#[cfg(test)]
mod scrollback_tests;
mod shared;
mod shell;
pub(crate) mod status;
#[cfg(test)]
mod status_tests;
mod sync;
pub(crate) mod tab_style;
#[cfg(test)]
mod tab_style_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod underline_color_tests;
#[cfg(test)]
mod width_tests;
#[cfg(test)]
mod window_ops_tests;

use crate::theme::Theme;
pub(crate) use colors::{Colors, Dynamic, Setting};
pub(crate) use extras::{Extras, NONE as NO_EXTRA};
pub(crate) use frame::Frame;
pub(crate) use grid::Grid;
pub(crate) use shared::{SharedTerm, TermGuard};

use std::{path::PathBuf, sync::Arc, time::Instant};

/// Bit flags stored in [`Cell::attrs`] and [`Term::attrs`].
pub(crate) mod attr {
	pub const DIM: u8 = 1;
	pub const ITALIC: u8 = 2;
	pub const UNDERLINE: u8 = 4;
	pub const CONCEAL: u8 = 8;
	pub const STRIKE: u8 = 16;
	pub const BLINK: u8 = 32;
	pub const OVERLINE: u8 = 64;
	pub const DOUBLE_UNDERLINE: u8 = 128;
}

/// Cursor shape set by DECSCUSR (`CSI Ps SP q`); blinking variants draw steady.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CursorShape {
	#[default]
	Block,
	Underline,
	Bar,
}

/// One grid cell (16 bytes). `width` is 1 for normal, 2 for the left half
/// of a wide character and 0 for its right-half spacer. Combining marks and
/// hyperlinks live in [`Term::extras`], referenced by `extra`.
///
/// Aligned to 16 bytes so cells move and compare as single vector words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, align(16))]
pub(crate) struct Cell {
	pub(crate) ch: char,
	pub(crate) fg: u32,
	pub(crate) bg: u32,
	pub(crate) attrs: u8,
	pub(crate) width: u8,
	pub(crate) extra: u16,
}

impl Cell {
	/// The cell as one 128-bit word; equal exactly when the cells are.
	#[inline(always)]
	pub(crate) fn bits(self) -> u128 {
		// SAFETY: `repr(C)` with 4 + 4 + 4 + 1 + 1 + 2 bytes of fields leaves
		// no padding, so every byte is initialized.
		unsafe { std::mem::transmute(self) }
	}

	/// The cell as four 32-bit lanes, the character first, for building cells
	/// in vector registers.
	#[inline(always)]
	pub(crate) fn words(self) -> std::simd::u32x4 {
		// SAFETY: as in `bits`.
		unsafe { std::mem::transmute(self) }
	}

	/// `words` from [`Cell::words`], with width 0, given `width` instead.
	#[inline(always)]
	pub(crate) fn with_width(mut words: std::simd::u32x4, width: usize) -> std::simd::u32x4 {
		// `width` is byte 13: the second byte of the last lane, as every
		// target we build for is little-endian.
		words[3] |= (width as u32) << 8;
		words
	}

	/// `words` from [`Cell::words`] with the character cleared.
	#[inline(always)]
	pub(crate) fn without_char(words: std::simd::u32x4) -> std::simd::u32x4 {
		words & std::simd::u32x4::from_array([0, !0, !0, !0])
	}

	/// `words` from [`Cell::without_char`] holding `ch`, which must be a
	/// valid `char`; an OR, where replacing the lane would shuffle.
	#[inline(always)]
	pub(crate) fn with_char(words: std::simd::u32x4, ch: u32) -> std::simd::u32x4 {
		words | std::simd::u32x4::from_array([ch, 0, 0, 0])
	}

	/// `cells` as words, to store whole cells with one vector store each. Lane
	/// 0 must hold a `char`.
	#[inline(always)]
	pub(crate) fn words_mut(cells: &mut [Cell]) -> &mut [std::simd::u32x4] {
		// SAFETY: as in `fill`; callers keep lane 0 a valid `char`.
		unsafe { std::slice::from_raw_parts_mut(cells.as_mut_ptr().cast(), cells.len()) }
	}

	/// Sets every cell of `cells` to `cell` with whole-cell vector stores;
	/// `slice::fill` writes cells field by field.
	#[inline(always)]
	pub(crate) fn fill(cells: &mut [Cell], cell: Cell) {
		#[cfg(target_arch = "x86_64")]
		if cells.len() >= 4 && std::arch::is_x86_feature_detected!("avx2") {
			// SAFETY: the CPU supports AVX2.
			return unsafe { fill::fill(cells, cell) };
		}
		use std::simd::u64x2;
		// SAFETY: `Cell` is 16 bytes with no padding and 16-byte alignment, so
		// the same memory is a valid `[u64x2]`.
		let words =
			unsafe { std::slice::from_raw_parts_mut(cells.as_mut_ptr().cast::<u64x2>(), cells.len()) };
		let bits = cell.bits();
		words.fill(u64x2::from_array([bits as u64, (bits >> 64) as u64]));
	}
}

impl Default for Cell {
	fn default() -> Self {
		Self {
			attrs: 0,
			ch: ' ',
			fg: 0,
			bg: 0,
			width: 1,
			extra: extras::NONE,
		}
	}
}

#[derive(Clone, Copy)]
pub(crate) struct SavedCursor {
	x: usize,
	y: usize,
	fg: u32,
	fg_id: u32,
	bg: u32,
	attrs: u8,
	bold: bool,
	reverse: bool,
	underline_color: Option<u32>,
	origin: bool,
	wrap: bool,
	charsets: [bool; 2],
	active_charset: usize,
}

#[derive(Clone)]
pub(crate) struct Term {
	pub(crate) images: Vec<crate::graphics::Placement>,
	pub(crate) primary_images: Vec<crate::graphics::Placement>,
	pub(crate) sixel_transfer: Option<crate::sixel::Transfer>,
	/// A DECRQSS or XTGETTCAP request being received.
	pub(crate) dcs_request: Option<dcs::Request>,
	pub(crate) sixel_scrolling: bool,
	/// Receives kitty graphics APC strings; always the live terminal's.
	pub(crate) kitty_receiver: crate::graphics::kitty::Receiver,
	/// Transmitted kitty images, shared by both screens.
	pub(crate) kitty_images: crate::graphics::kitty::Store,
	pub(crate) cell_width: usize,
	pub(crate) cell_height: usize,
	/// Physical pixels per logical point of the window showing the terminal.
	pub(crate) scale: f32,
	/// The user asked for reduced motion: animations hold their frame.
	pub(crate) reduce_motion: bool,
	/// What frames show during synchronized output.
	pub(crate) synchronized: Option<Box<Frame>>,
	/// The last block's frame, kept so the next one reuses its buffers.
	pub(crate) spare_frame: Option<Box<Frame>>,
	pub(crate) sync_deadline: Option<Instant>,
	pub(crate) shell: crate::shell_integration::ShellIntegration,
	pub(crate) history_base: usize,
	pub(crate) cols: usize,
	pub(crate) rows: usize,
	/// The visible screen.
	pub(crate) grid: Grid,
	pub(crate) scrollback: scrollback::Scrollback,
	pub(crate) scrollback_limit: usize,
	pub(crate) scroll_offset: usize,
	pub(crate) primary_screen: Option<(Grid, usize, usize, u32, u32)>,
	pub(crate) primary_modes: Option<(usize, usize, bool, bool)>,
	pub(crate) cursor_visible: bool,
	pub(crate) default_fg: u32,
	pub(crate) default_bg: u32,
	/// The theme's default colors, which OSC 110 and 111 restore.
	pub(crate) theme_fg: u32,
	pub(crate) theme_bg: u32,
	pub(crate) palette: [u32; 8],
	pub(crate) color_overrides: [Option<u32>; 256],
	pub(crate) colors: Colors,
	pub(crate) color_stack: Vec<colors::Saved>,
	pub(crate) cx: usize,
	pub(crate) cy: usize,
	pub(crate) fg: u32,
	/// The foreground as the client set it: an RGB value, a 256-color index,
	/// or 0 for the default. Kitty Unicode placeholders take their image id
	/// from it.
	pub(crate) fg_id: u32,
	/// The foreground SGR chose, before special colors (OSC 5/6) replace it.
	pub(crate) sgr_fg: u32,
	pub(crate) bg: u32,
	pub(crate) scroll_top: usize,
	pub(crate) scroll_bottom: usize,
	/// The top margin came from an HP memory lock (`ESC l`).
	pub(crate) memory_locked: bool,
	pub(crate) saved_cursor: Option<SavedCursor>,
	pub(crate) bracketed_paste: bool,
	pub(crate) mouse_tracking: u16,
	pub(crate) mouse_sgr: bool,
	pub(crate) application_cursor: bool,
	/// DECKPAM / DECNKM: the numeric keypad sends application sequences.
	pub(crate) application_keypad: bool,
	pub(crate) origin_mode: bool,
	pub(crate) auto_wrap: bool,
	/// DECSET 45: backspace in the first column goes to the line above.
	pub(crate) reverse_wrap: bool,
	/// DECSCNM: frames show every cell with its colors swapped.
	pub(crate) reverse_screen: bool,
	pub(crate) insert_mode: bool,
	/// KAM (`CSI 2 h`): keyboard input is discarded.
	pub(crate) keyboard_locked: bool,
	/// SRM reset (`CSI 12 l`): typed input is also shown locally.
	pub(crate) local_echo: bool,
	/// LNM (`CSI 20 h`): line feeds return the carriage; Enter sends CR LF.
	pub(crate) newline_mode: bool,
	/// Private modes saved by XTSAVE, one value per mode.
	pub(crate) saved_modes: Vec<(u16, bool)>,
	pub(crate) kitty_keyboard: bool,
	/// XTMODKEYS and XTFMTKEYS settings.
	pub(crate) key_options: crate::input::modify::KeyOptions,
	pub(crate) keyboard: crate::input::keyboard_modes::KeyboardModes,
	pub(crate) cursor_shape: CursorShape,
	/// `?1004`: report window focus changes as `CSI I` / `CSI O`.
	pub(crate) focus_reporting: bool,
	/// The last printed graphic character, repeated by REP (`CSI Ps b`).
	pub(crate) last_printed: Option<char>,
	pub(crate) active_hyperlink: u16,
	/// SGR 58: the color underlines are drawn in, instead of the text's.
	pub(crate) underline_color: Option<u32>,
	/// `extra` id applied to printed cells: the active hyperlink and
	/// underline color.
	pub(crate) active_extra: u16,
	/// Combining marks and hyperlinks referenced by [`Cell::extra`], shared
	/// copy-on-write between clones.
	pub(crate) extras: Arc<extras::Extras>,
	/// OSC 8 targets by index; index 0 is no link.
	pub(crate) hyperlinks: Vec<String>,
	/// The index of each target in `hyperlinks`, so a repeated target reuses
	/// its slot instead of filling the bounded table.
	pub(crate) hyperlink_ids: std::collections::HashMap<String, u16>,
	pub(crate) wrap_pending: bool,
	/// Whether blocks of scaled text (OSC 66) may be on screen; printing
	/// and erasing take the slower paths that respect them.
	pub(crate) has_multicells: bool,
	pub(crate) charsets: [bool; 2],
	pub(crate) active_charset: usize,
	pub(crate) tab_stops: Vec<bool>,
	pub(crate) attrs: u8,
	pub(crate) bold: bool,
	pub(crate) reverse: bool,
	pub(crate) responses: Vec<Vec<u8>>,
	/// Clipboard writes for the UI thread to perform.
	pub(crate) clipboard_writes: Vec<clipboard::ClipboardWrite>,
	pub(crate) clipboard_upload: Option<clipboard::Upload>,
	/// Desktop notifications: requests for the UI thread and live state.
	pub(crate) notifications: notifications::Notifications,
	/// What programs report they are doing (OSC 7501, OSC 9;4).
	pub(crate) status: status::Status,
	/// iTerm2's OSC 1337 state.
	pub(crate) iterm: iterm::Iterm,
	/// Focus and attention requests for the UI thread.
	pub(crate) window_requests: Vec<iterm::WindowRequest>,
	/// OSC 3008 contexts, kept across resets.
	pub(crate) contexts: contexts::Contexts,
	/// OSC 22 pointer shapes of the main and alternate screens.
	pub(crate) pointer_shapes: pointer::PointerShapes,
	pub(crate) title: String,
	pub(crate) title_generation: u64,
	/// xterm's icon name (OSC 0 and 1).
	pub(crate) icon_name: String,
	/// Titles and icon names saved by XTWINOPS 22, newest last.
	pub(crate) title_stack: Vec<String>,
	pub(crate) icon_stack: Vec<String>,
	/// ConEmu's tab name (OSC 9;3), shown instead of the title.
	pub(crate) tab_title: Option<String>,
	/// iTerm2's tab color and session status.
	pub(crate) tab_style: tab_style::TabStyle,
	pub(crate) cwd: PathBuf,
	pub(crate) cwd_generation: u64,
	pub(crate) bell: bool,
}

impl Term {
	pub(crate) fn save_cursor(&mut self) {
		self.saved_cursor = Some(SavedCursor {
			x: self.cx,
			y: self.cy,
			fg: self.fg,
			fg_id: self.fg_id,
			bg: self.bg,
			attrs: self.attrs,
			bold: self.bold,
			reverse: self.reverse,
			underline_color: self.underline_color,
			origin: self.origin_mode,
			wrap: self.wrap_pending,
			charsets: self.charsets,
			active_charset: self.active_charset,
		});
	}

	pub(crate) fn restore_cursor(&mut self) {
		if let Some(saved) = self.saved_cursor {
			self.cx = saved.x.min(self.cols - 1);
			self.cy = saved.y.min(self.rows - 1);
			self.fg = saved.fg;
			self.fg_id = saved.fg_id;
			self.bg = saved.bg;
			self.attrs = saved.attrs;
			self.bold = saved.bold;
			self.reverse = saved.reverse;
			self.set_underline_color(saved.underline_color);
			self.origin_mode = saved.origin;
			self.wrap_pending = saved.wrap && self.cx == self.cols - 1;
			self.charsets = saved.charsets;
			self.active_charset = saved.active_charset;
		} else {
			self.cx = 0;
			self.cy = 0;
			self.wrap_pending = false;
		}
	}

	#[inline(always)]
	pub(crate) fn row_cells(&self, row: usize) -> Option<&[Cell]> {
		if row < self.scrollback.len() {
			self.scrollback.get(row)
		} else {
			let row = row - self.scrollback.len();
			(row < self.rows).then(|| self.grid[row].as_slice())
		}
	}

	pub(crate) fn set_theme(&mut self, theme: Theme) {
		self.finish_sync();
		self.set_default_colors(theme.fg, theme.bg);
		self.theme_fg = theme.fg;
		self.theme_bg = theme.bg;
		self.palette = theme.palette;
		self.color_overrides.fill(None);
		self.colors = Colors::default();
	}

	#[inline(always)]
	/// The cell trimmed from the end of scrollback rows: a blank in the
	/// default colors.
	pub(crate) fn padding(&self) -> Cell {
		Cell {
			fg: self.default_fg,
			bg: self.default_bg,
			..Cell::default()
		}
	}

	pub(crate) fn blank(&self) -> Cell {
		Cell {
			attrs: self.attrs,
			ch: ' ',
			fg: self.fg,
			bg: self.bg,
			width: 1,
			extra: extras::NONE,
		}
	}

	pub(crate) fn new(cols: usize, rows: usize, theme: Theme, cwd: PathBuf) -> Self {
		let cols = cols.max(1);
		let rows = rows.max(1);
		Self {
			images: Vec::new(),
			primary_images: Vec::new(),
			sixel_transfer: None,
			dcs_request: None,
			sixel_scrolling: true,
			kitty_receiver: Default::default(),
			kitty_images: Default::default(),
			cell_width: 10,
			cell_height: 20,
			scale: 1.0,
			reduce_motion: false,
			synchronized: None,
			spare_frame: None,
			sync_deadline: None,
			shell: Default::default(),
			history_base: 0,
			cols,
			rows,
			grid: Grid::new(
				cols,
				rows,
				Cell {
					fg: theme.fg,
					bg: theme.bg,
					..Cell::default()
				},
			),
			scrollback: scrollback::Scrollback::default(),
			scrollback_limit: 5000,
			scroll_offset: 0,
			primary_screen: None,
			primary_modes: None,
			cursor_visible: true,
			cx: 0,
			cy: 0,
			default_fg: theme.fg,
			default_bg: theme.bg,
			theme_fg: theme.fg,
			theme_bg: theme.bg,
			palette: theme.palette,
			color_overrides: [None; 256],
			colors: Colors::default(),
			color_stack: Vec::new(),
			fg: theme.fg,
			fg_id: 0,
			sgr_fg: theme.fg,
			bg: theme.bg,
			scroll_top: 0,
			scroll_bottom: rows.saturating_sub(1),
			memory_locked: false,
			saved_cursor: None,
			bracketed_paste: false,
			mouse_tracking: 0,
			mouse_sgr: false,
			application_cursor: false,
			application_keypad: false,
			origin_mode: false,
			auto_wrap: true,
			reverse_wrap: true,
			reverse_screen: false,
			insert_mode: false,
			keyboard_locked: false,
			local_echo: false,
			newline_mode: false,
			saved_modes: Vec::new(),
			kitty_keyboard: false,
			key_options: Default::default(),
			keyboard: Default::default(),
			cursor_shape: CursorShape::Block,
			focus_reporting: false,
			last_printed: None,
			active_hyperlink: 0,
			underline_color: None,
			active_extra: extras::NONE,
			extras: Arc::default(),
			hyperlinks: vec![String::new()],
			hyperlink_ids: std::collections::HashMap::new(),
			wrap_pending: false,
			has_multicells: false,
			charsets: [false; 2],
			active_charset: 0,
			tab_stops: (0..cols).map(|col| col > 0 && col % 8 == 0).collect(),
			attrs: 0,
			bold: false,
			reverse: false,
			responses: Vec::new(),
			clipboard_writes: Vec::new(),
			clipboard_upload: None,
			notifications: notifications::Notifications::default(),
			status: status::Status::default(),
			contexts: contexts::Contexts::default(),
			iterm: iterm::Iterm::default(),
			window_requests: Vec::new(),
			pointer_shapes: pointer::PointerShapes::default(),
			title: "Shell".into(),
			title_generation: 0,
			icon_name: String::new(),
			title_stack: Vec::new(),
			icon_stack: Vec::new(),
			tab_title: None,
			tab_style: tab_style::TabStyle::default(),
			cwd,
			cwd_generation: 0,
			bell: false,
		}
	}

	/// Shows typed input as if the program had echoed it (SRM reset).
	pub(crate) fn echo_input(&mut self, bytes: &[u8]) {
		crate::parser::Parser::new().advance(self, bytes);
	}

	#[inline(always)]
	pub(crate) fn idx(&self, x: usize, y: usize) -> usize {
		y * self.cols + x
	}

	/// Combining marks attached to `cell`.
	pub(crate) fn marks(&self, cell: &Cell) -> &[char] {
		self.extras.marks(cell.extra)
	}

	/// The OSC 8 target of `cell`, if any.
	pub(crate) fn hyperlink_uri(&self, cell: &Cell) -> Option<&str> {
		let link = self.extras.hyperlink(cell.extra);
		(link != 0).then(|| self.hyperlinks.get(link as usize).map(String::as_str))?
	}
}
