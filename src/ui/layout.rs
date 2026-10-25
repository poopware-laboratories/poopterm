//! Where the grid goes in the main window: the margins, tab bar and status
//! bar around it, and how many cells fit.
use super::scale::Chrome;
use crate::config::Preferences;

/// The grid's place in the window, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
	/// Border plus padding, on every side of the grid.
	pub(crate) margin: usize,
	/// The grid's first pixel row.
	pub(crate) top: usize,
	/// The first pixel row of the padding above the grid: below the border,
	/// the title bar and a tab bar, which ends where the grid starts.
	pub(crate) padding_top: usize,
	pub(crate) status_height: usize,
	/// The grid's first pixel column, right of a vertical tab `sidebar`.
	pub(crate) left: usize,
}

/// Lays out the window for `chrome`, drawn at the whole-number `scale`.
pub(crate) fn layout(chrome: &Chrome, scale: usize, prefs: &Preferences, sidebar: usize) -> Layout {
	let margin = chrome.border + prefs.padding * scale;
	// With a title bar (macOS), the tab strip sits in it.
	let tab_bar = prefs.show_tabs && !prefs.vertical_tabs && chrome.titlebar_height == 0;
	let top = chrome.titlebar_height + if tab_bar { chrome.tab_height } else { 0 } + margin;
	Layout {
		margin,
		top,
		padding_top: if tab_bar {
			top
		} else {
			chrome.titlebar_height + chrome.border
		},
		status_height: if prefs.show_bottom_bar { 26 * scale } else { 0 },
		left: margin + sidebar,
	}
}

impl Layout {
	/// How many `cell`-sized cells fit in a `window`, at least 2 by 2.
	pub(crate) fn grid_size(&self, window: (usize, usize), cell: (usize, usize)) -> (usize, usize) {
		let width = window.0.saturating_sub(self.left + self.margin);
		let height = window
			.1
			.saturating_sub(self.top + self.margin + self.status_height);
		(
			(width / cell.0.max(1)).max(2),
			(height / cell.1.max(1)).max(2),
		)
	}

	/// The window size holding exactly a `grid` of `cell`-sized cells.
	pub(crate) fn window_size(&self, grid: (usize, usize), cell: (usize, usize)) -> (usize, usize) {
		(
			self.left + grid.0 * cell.0 + self.margin,
			self.top + grid.1 * cell.1 + self.margin + self.status_height,
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::ui::scale::{chrome, chrome_scale};

	/// Horizontal tabs, hidden tabs and vertical tabs.
	fn tab_modes() -> [Preferences; 3] {
		let base = Preferences::default();
		let hidden = Preferences {
			show_tabs: false,
			..base
		};
		let vertical = Preferences {
			vertical_tabs: true,
			..base
		};
		[base, hidden, vertical]
	}

	fn linux(prefs: &Preferences) -> Layout {
		layout(&chrome(1.0, false), 1, prefs, 0)
	}

	#[test]
	fn the_default_linux_layout_sits_below_the_tab_bar() {
		let expected = Layout {
			margin: 0,
			top: 44,
			padding_top: 44,
			status_height: 26,
			left: 0,
		};
		assert_eq!(linux(&Preferences::default()), expected);
	}

	#[test]
	fn padding_and_the_status_bar_follow_the_chrome_scale() {
		let prefs = Preferences {
			padding: 6,
			..Preferences::default()
		};
		let layout = layout(&chrome(2.0, false), 2, &prefs, 0);
		assert_eq!(layout.margin, 12);
		assert_eq!(layout.status_height, 52);
		assert_eq!(layout.top, 88 + 12);
	}

	#[test]
	fn a_hidden_bottom_bar_takes_no_space() {
		let prefs = Preferences {
			show_bottom_bar: false,
			..Preferences::default()
		};
		assert_eq!(linux(&prefs).status_height, 0);
	}

	#[test]
	fn without_a_tab_strip_on_top_the_grid_starts_at_the_margin() {
		let [_, hidden, vertical] = tab_modes();
		assert_eq!(linux(&hidden).top, 0);
		assert_eq!(linux(&vertical).top, 0);
	}

	#[test]
	fn a_vertical_tab_sidebar_moves_the_grid_right() {
		let [_, _, vertical] = tab_modes();
		assert_eq!(layout(&chrome(1.0, false), 1, &vertical, 180).left, 180);
		assert_eq!(linux(&vertical).left, 0);
	}

	#[test]
	fn on_macos_the_tabs_live_in_the_title_bar() {
		for prefs in tab_modes() {
			let layout = layout(&chrome(1.0, true), 1, &prefs, 0);
			assert_eq!(layout.top, 44, "under the title bar, with no border");
			assert_eq!(layout.margin, 0);
		}
	}

	#[test]
	fn cells_fill_the_space_left_inside_the_chrome() {
		let layout = linux(&Preferences::default());
		// 80 * 10 by 44 + 24 * 20 + 26.
		assert_eq!(layout.grid_size((800, 550), (10, 20)), (80, 24));
		assert_eq!(
			layout.grid_size((799, 549), (10, 20)),
			(79, 23),
			"a pixel short"
		);
		assert_eq!(
			layout.grid_size((809, 569), (10, 20)),
			(80, 24),
			"a pixel under the next"
		);
	}

	#[test]
	fn tiny_windows_and_empty_cells_still_give_two_by_two() {
		let layout = linux(&Preferences::default());
		assert_eq!(layout.grid_size((1, 1), (10, 20)), (2, 2));
		assert_eq!(layout.grid_size((0, 0), (0, 0)), (2, 2));
		assert_eq!(
			layout.grid_size((800, 550), (0, 0)),
			(800, 480),
			"no division by zero"
		);
	}

	#[test]
	fn the_window_asked_for_holds_exactly_the_grid_it_was_asked_for() {
		// The window is opened at the size for the previous grid; laying it
		// out must give that grid back at every scale, font and chrome.
		for macos in [false, true] {
			for factor in [1.0, 1.25, 1.5, 1.75, 2.0, 3.0] {
				let scale = chrome_scale(factor);
				for (prefs, sidebar) in tab_modes().into_iter().zip([0, 0, 180]) {
					for padding in [0, 3] {
						let prefs = Preferences { padding, ..prefs };
						let layout = layout(&chrome(factor, macos), scale, &prefs, sidebar);
						for cell in [(7, 15), (10, 20), (13, 27)] {
							for grid in [(2, 2), (80, 24), (211, 57)] {
								let window = layout.window_size(grid, cell);
								assert_eq!(layout.grid_size(window, cell), grid, "{layout:?} {cell:?}");
							}
						}
					}
				}
			}
		}
	}

	#[test]
	fn the_grid_never_reaches_into_the_border_or_the_status_bar() {
		for factor in [1.0, 1.5, 2.0] {
			let prefs = Preferences::default();
			let layout = layout(&chrome(factor, false), chrome_scale(factor), &prefs, 0);
			let cell = (9, 19);
			for width in (200..1200).step_by(37) {
				for height in (200..900).step_by(41) {
					let (cols, rows) = layout.grid_size((width, height), cell);
					assert!(
						layout.left + cols * cell.0 + layout.margin <= width,
						"{width}x{height}"
					);
					let bottom = layout.top + rows * cell.1 + layout.margin + layout.status_height;
					assert!(bottom <= height, "{width}x{height} at {factor}");
				}
			}
		}
	}

	#[test]
	fn fractional_factors_lay_out_like_their_whole_chrome_scale() {
		// Chrome follows the rounded scale, so 1.25 keeps the 1x layout and
		// 1.5 takes the 2x one, padding and status bar included.
		let prefs = Preferences {
			padding: 4,
			..Preferences::default()
		};
		let at = |factor: f64| layout(&chrome(factor, false), chrome_scale(factor), &prefs, 0);
		assert_eq!(at(1.25), at(1.0));
		assert_eq!(at(1.5), at(2.0));
		assert_eq!(at(1.5).margin, 8);
		assert_eq!(at(1.5).status_height, 52);
	}

	#[test]
	fn macos_padding_sits_below_the_title_bar() {
		let prefs = Preferences {
			padding: 4,
			..Preferences::default()
		};
		let layout = layout(&chrome(2.0, true), 2, &prefs, 0);
		assert_eq!((layout.margin, layout.top, layout.left), (8, 88 + 8, 8));
	}

	#[test]
	fn the_padding_above_the_grid_starts_below_the_tab_bar() {
		// The tab bar ends where the grid starts; its bottom border must not
		// be painted over with the padding.
		let prefs = Preferences {
			padding: 3,
			..Preferences::default()
		};
		let tabs = linux(&prefs);
		assert_eq!(tabs.top, 44 + 3);
		assert_eq!(tabs.padding_top, tabs.top);
		let scaled = layout(&chrome(2.0, false), 2, &prefs, 0);
		assert_eq!(scaled.padding_top, scaled.top);
	}

	#[test]
	fn without_a_tab_bar_the_padding_starts_at_the_top() {
		for padding in [0, 3] {
			let [_, hidden, vertical] = tab_modes().map(|prefs| Preferences { padding, ..prefs });
			for prefs in [hidden, vertical] {
				let layout = linux(&prefs);
				assert_eq!(layout.padding_top, 0, "vertical: {}", prefs.vertical_tabs);
				assert_eq!(layout.top - layout.padding_top, padding);
			}
		}
	}

	#[test]
	fn on_macos_the_padding_starts_below_the_title_bar() {
		for prefs in tab_modes() {
			let prefs = Preferences {
				padding: 4,
				..prefs
			};
			let layout = layout(&chrome(2.0, true), 2, &prefs, 0);
			assert_eq!(layout.padding_top, 88, "vertical: {}", prefs.vertical_tabs);
			assert_eq!(layout.top, 88 + 8);
		}
	}

	#[test]
	fn without_a_rim_the_grid_starts_at_the_padding_at_every_scale() {
		for factor in [1.0, 1.5, 2.0, 3.0] {
			let scale = chrome_scale(factor);
			for padding in [0, 5] {
				for (prefs, sidebar) in tab_modes().into_iter().zip([0, 0, 180]) {
					let prefs = Preferences { padding, ..prefs };
					let layout = layout(&chrome(factor, false), scale, &prefs, sidebar);
					assert_eq!(layout.margin, padding * scale, "{factor}");
					assert_eq!(layout.left, sidebar + padding * scale, "{factor}");
					let bar = if prefs.show_tabs && !prefs.vertical_tabs {
						44
					} else {
						0
					};
					assert_eq!(layout.top, (bar + padding) * scale, "{factor}");
				}
			}
		}
	}

	#[test]
	fn the_window_for_a_grid_adds_the_sidebar_and_every_bar() {
		let [_, _, vertical] = tab_modes();
		let layout = layout(&chrome(1.0, false), 1, &vertical, 180);
		// 180 + 80 * 10 across, 24 * 20 + 26 down.
		assert_eq!(layout.window_size((80, 24), (10, 20)), (980, 506));
		let horizontal = linux(&Preferences::default());
		assert_eq!(horizontal.window_size((80, 24), (10, 20)), (800, 550));
	}
}
