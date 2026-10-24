//! The main window's persistent back buffer, and deciding how much of it a
//! new frame has to redraw.
use crate::{
	render::damage::{GridDamage, grid_damage},
	term::Frame,
	ui::selection::Selection,
};

/// Drawing inputs besides the grid. Session, focus, and overlays need a full
/// frame; highlights only damage rows where their coverage changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DrawKey {
	pub session: u64,
	pub focused: bool,
	pub selection: Option<Selection>,
	pub search_match: Option<Selection>,
	/// Search box or command palette, which cover part of the grid.
	pub overlay: bool,
}

/// How to produce the next frame.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Plan {
	Full,
	Partial(GridDamage),
}

pub(super) struct Canvas {
	pub pixels: Vec<u32>,
	width: usize,
	height: usize,
	/// The grid and inputs the pixels show; `None` after [`invalidate`](Self::invalidate).
	shown: Option<(Frame, DrawKey)>,
}

impl Canvas {
	pub(super) fn new() -> Self {
		Self {
			pixels: Vec::new(),
			width: 0,
			height: 0,
			shown: None,
		}
	}

	/// Forces the next frame to be drawn in full, e.g. after a theme, font
	/// or wallpaper change.
	pub(super) fn invalidate(&mut self) {
		self.shown = None;
	}

	/// Decides how to draw `frame` at `width`×`height`, resizing the buffer
	/// when needed. `allow_scroll` permits shifting pixels (not with a
	/// wallpaper, which stays put while text scrolls).
	pub(super) fn plan(
		&mut self,
		frame: &Frame,
		key: DrawKey,
		(width, height): (usize, usize),
		allow_scroll: bool,
	) -> Plan {
		if (self.width, self.height) != (width, height) {
			self.width = width;
			self.height = height;
			self.pixels.clear();
			self.pixels.resize(width * height, 0);
			self.shown = None;
		}
		let partial = match &self.shown {
			Some((shown, shown_key))
				if shown_key.session == key.session
					&& shown_key.focused == key.focused
					&& !shown_key.overlay
					&& !key.overlay =>
			{
				grid_damage(shown, frame, allow_scroll).map(|mut damage| {
					for (row, damaged) in damage.rows.iter_mut().enumerate() {
						let old = shown.first_row + row + damage.scroll;
						let new = frame.first_row + row;
						*damaged |= coverage(shown_key.selection, old, frame.cols)
							!= coverage(key.selection, new, frame.cols)
							|| coverage(shown_key.search_match, old, frame.cols)
								!= coverage(key.search_match, new, frame.cols);
					}
					damage
				})
			}
			_ => None,
		};
		partial.map_or(Plan::Full, Plan::Partial)
	}

	/// Records that the pixels now show `frame`; hands back the previous
	/// frame's buffers for reuse.
	pub(super) fn remember(&mut self, frame: &mut Frame, key: DrawKey) {
		match &mut self.shown {
			Some((shown, shown_key)) => {
				std::mem::swap(shown, frame);
				*shown_key = key;
			}
			None => self.shown = Some((std::mem::take(frame), key)),
		}
	}
}

/// Inclusive highlighted columns of a visible row, clipped to the grid.
fn coverage(range: Option<Selection>, row: usize, cols: usize) -> Option<(usize, usize)> {
	range?.span(row, cols)
}

/// Merges sorted, possibly adjacent pixel-row ranges.
/// Moves pixel rows `top + shift..bottom` up by `shift` rows. Columns left of
/// `left` (the vertical tab sidebar) stay where they are.
pub(super) fn scroll_pixels(
	pixels: &mut [u32],
	width: usize,
	(top, bottom): (usize, usize),
	shift: usize,
	left: usize,
) {
	if left == 0 {
		pixels.copy_within((top + shift) * width..bottom * width, top * width);
		return;
	}
	for row in top..bottom.saturating_sub(shift) {
		let source = (row + shift) * width;
		pixels.copy_within(source + left..source + width, row * width + left);
	}
}

pub(super) fn merge_bands(bands: &mut Vec<(usize, usize)>) {
	bands.sort_unstable();
	let mut merged: Vec<(usize, usize)> = Vec::with_capacity(bands.len());
	for &(start, end) in bands.iter() {
		match merged.last_mut() {
			Some(last) if start <= last.1 => last.1 = last.1.max(end),
			_ => merged.push((start, end)),
		}
	}
	*bands = merged;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ui::selection::CellPoint;
	use crate::{
		parser::Parser,
		term::Term,
		theme::{Theme, ThemeKind},
	};

	#[test]
	fn scrolling_moves_pixels_right_of_the_sidebar_only() {
		// 4 columns, 5 rows; each pixel is 10 * row + column.
		let mut pixels: Vec<u32> = (0..5)
			.flat_map(|row| (0..4).map(move |col| 10 * row + col))
			.collect();
		// Rows 1..5 scroll up by one row; column 0 is a sidebar.
		scroll_pixels(&mut pixels, 4, (1, 5), 1, 1);
		assert_eq!(
			pixels,
			[
				0, 1, 2, 3, 10, 21, 22, 23, 20, 31, 32, 33, 30, 41, 42, 43, 40, 41, 42, 43
			]
		);
		let mut full: Vec<u32> = (0..6).collect();
		scroll_pixels(&mut full, 2, (0, 3), 1, 0);
		assert_eq!(full, [2, 3, 4, 5, 4, 5]);
	}

	const KEY: DrawKey = DrawKey {
		session: 1,
		focused: true,
		selection: None,
		search_match: None,
		overlay: false,
	};

	fn frame(input: &str) -> Frame {
		let mut term = Term::new(4, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
		Parser::new().advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		frame
	}

	#[test]
	fn the_first_frame_and_frames_after_invalidation_are_full() {
		let mut canvas = Canvas::new();
		let mut shown = frame("a");
		assert_eq!(canvas.plan(&shown, KEY, (10, 10), true), Plan::Full);
		assert_eq!(canvas.pixels.len(), 100);
		canvas.remember(&mut shown, KEY);
		assert!(matches!(
			canvas.plan(&frame("a"), KEY, (10, 10), true),
			Plan::Partial(damage) if damage.is_empty()
		));
		canvas.invalidate();
		assert_eq!(canvas.plan(&frame("a"), KEY, (10, 10), true), Plan::Full);
	}

	#[test]
	fn other_inputs_or_sizes_force_a_full_frame() {
		let mut canvas = Canvas::new();
		canvas.plan(&frame("a"), KEY, (10, 10), true);
		canvas.remember(&mut frame("a"), KEY);
		let unfocused = DrawKey {
			focused: false,
			..KEY
		};
		assert_eq!(
			canvas.plan(&frame("a"), unfocused, (10, 10), true),
			Plan::Full
		);
		let overlay = DrawKey {
			overlay: true,
			..KEY
		};
		canvas.remember(&mut frame("a"), overlay);
		assert_eq!(
			canvas.plan(&frame("a"), overlay, (10, 10), true),
			Plan::Full
		);
		canvas.remember(&mut frame("a"), KEY);
		assert_eq!(canvas.plan(&frame("a"), KEY, (11, 10), true), Plan::Full);
	}

	#[test]
	fn highlight_changes_damage_only_rows_with_different_coverage() {
		let mut canvas = Canvas::new();
		let first = frame("\x1b[?25l");
		canvas.plan(&first, KEY, (10, 10), true);
		canvas.remember(&mut first.clone(), KEY);
		let selected = DrawKey {
			selection: Some(Selection {
				start: CellPoint { row: 1, col: 0 },
				end: CellPoint { row: 1, col: 1 },
				block: false,
			}),
			..KEY
		};
		assert_eq!(
			canvas.plan(&first, selected, (10, 10), true),
			Plan::Partial(GridDamage {
				scroll: 0,
				rows: vec![false, true]
			})
		);
		canvas.remember(&mut first.clone(), selected);
		assert!(
			matches!(canvas.plan(&first, selected, (10, 10), true), Plan::Partial(d) if d.is_empty())
		);
		assert_eq!(
			canvas.plan(&first, KEY, (10, 10), true),
			Plan::Partial(GridDamage {
				scroll: 0,
				rows: vec![false, true]
			})
		);
		let found = DrawKey {
			search_match: selected.selection,
			..KEY
		};
		assert_eq!(
			canvas.plan(&first, found, (10, 10), true),
			Plan::Partial(GridDamage {
				scroll: 0,
				rows: vec![false, true]
			})
		);
	}

	#[test]
	fn switching_to_a_block_damages_only_rows_whose_columns_change() {
		let lines = frame("abcd\r\nefgh");
		let select = |block| DrawKey {
			selection: Some(Selection {
				start: CellPoint { row: 0, col: 3 },
				end: CellPoint { row: 1, col: 3 },
				block,
			}),
			..KEY
		};
		let mut canvas = Canvas::new();
		canvas.plan(&lines, select(false), (10, 10), true);
		canvas.remember(&mut lines.clone(), select(false));
		// Row 0 is column 3 alone either way; row 1 shrinks from 0..=3 to 3.
		let plan = canvas.plan(&lines, select(true), (10, 10), true);
		let expected = GridDamage {
			scroll: 0,
			rows: vec![false, true],
		};
		assert_eq!(plan, Plan::Partial(expected));
	}

	#[test]
	fn bands_are_merged() {
		let mut bands = vec![(10, 20), (0, 5), (20, 30), (25, 26), (40, 41)];
		merge_bands(&mut bands);
		assert_eq!(bands, [(0, 5), (10, 30), (40, 41)]);
	}
}

#[cfg(test)]
#[path = "canvas_pixels_tests.rs"]
mod pixels;
