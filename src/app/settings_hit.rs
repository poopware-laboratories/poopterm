//! Hit-testing for the Settings window: which control a click lands on.
use super::{
	settings_diagnostics::diagnostics_target,
	settings_pen::{segment_at, switch_hit},
	settings_view::*,
};
use crate::{input::shortcuts::ShortcutAction, ui::diagnostics_page::Section};

/// The page the Settings window shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SettingsPage {
	#[default]
	Main,
	Layout,
	Shortcuts,
	/// The split pane shortcuts, apart so each page fits the smallest window.
	Panes,
	About,
	Fonts,
	Diagnostics,
}

/// A clickable control in the Settings window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsTarget {
	/// Opens a page from the sidebar.
	Open(SettingsPage),
	Save,
	ChooseWallpaper,
	ClearWallpaper,
	ToggleRestore,
	ToggleTabs,
	/// The Layout page's tab position: `true` for the side.
	SetVerticalTabs(bool),
	ToggleCompactTabs,
	ToggleNewTabButton,
	ToggleSettingsButton,
	ToggleBottomBar,
	SetShortcut(ShortcutAction),
	/// A visible row of the font list, counted from the top.
	FontRow(usize),
	/// A stepper button: the row constant and -1 or +1.
	Step(usize, isize),
	/// A theme arrow: -1 or +1.
	Theme(isize),
	/// A Diagnostics section's switch button.
	Section(Section),
	CopyReport,
}

/// How many font list rows fit above the footer of a window `height`
/// physical pixels tall.
pub(super) fn font_list_rows(height: usize, scale: usize) -> usize {
	(height / scale.max(1)).saturating_sub(64 + FONT_LIST_TOP) / FONT_LIST_STRIDE
}

/// Toggles on the Layout page.
pub(super) const LAYOUT_TOGGLES: [(usize, SettingsTarget); 5] = [
	(SHOW_TABS_ROW, SettingsTarget::ToggleTabs),
	(COMPACT_TABS_ROW, SettingsTarget::ToggleCompactTabs),
	(NEW_TAB_BUTTON_ROW, SettingsTarget::ToggleNewTabButton),
	(SETTINGS_BUTTON_ROW, SettingsTarget::ToggleSettingsButton),
	(BOTTOM_BAR_ROW, SettingsTarget::ToggleBottomBar),
];

/// Maps a click at physical `(x, y)` to the control under it.
pub(super) fn settings_target(
	(x, y): (usize, usize),
	(width, height): (usize, usize),
	scale: usize,
	page: SettingsPage,
) -> Option<SettingsTarget> {
	let s = scale;
	let in_row = |row: usize| (row * s..(row + ROW_HEIGHT) * s).contains(&y);
	let right = |offset: usize| width.saturating_sub(offset * s);
	let (save_x, save_y) = (right(140), height.saturating_sub(52 * s));
	let on_save = (save_x..save_x + 120 * s).contains(&x) && (save_y..save_y + 36 * s).contains(&y);
	if page == SettingsPage::Diagnostics {
		return diagnostics_target((x, y), (width, height), s);
	}
	if page != SettingsPage::Main {
		if page == SettingsPage::Fonts {
			let top = FONT_LIST_TOP * s;
			let (row, within) = (
				y.saturating_sub(top) / (FONT_LIST_STRIDE * s),
				y.saturating_sub(top) % (FONT_LIST_STRIDE * s),
			);
			if y >= top
				&& within < ROW_HEIGHT * s
				&& row < font_list_rows(height, s)
				&& (24 * s..right(24)).contains(&x)
			{
				return Some(SettingsTarget::FontRow(row));
			}
		}
		let rows: &[(usize, SettingsTarget)] = match page {
			SettingsPage::Layout => &LAYOUT_TOGGLES,
			SettingsPage::Shortcuts => &SHORTCUT_TARGETS,
			SettingsPage::Panes => &PANE_TARGETS,
			SettingsPage::Fonts => &[],
			_ => return None,
		};
		if page == SettingsPage::Layout && in_row(PADDING_ROW) {
			if (right(240)..right(192)).contains(&x) {
				return Some(SettingsTarget::Step(PADDING_ROW, -1));
			}
			if (right(72)..right(24)).contains(&x) {
				return Some(SettingsTarget::Step(PADDING_ROW, 1));
			}
		}
		if page == SettingsPage::Layout
			&& let Some(side) = segment_at(width, VERTICAL_TABS_ROW, s, (x, y), 2)
		{
			return Some(SettingsTarget::SetVerticalTabs(side == 1));
		}
		let on_switch = |row: usize| {
			let (sx, sy, sw, sh) = switch_hit(width, row, s);
			(sx..sx + sw).contains(&x) && (sy..sy + sh).contains(&y)
		};
		let wide = |row: usize| in_row(row) && (right(240)..right(24)).contains(&x);
		// The Layout page's rows are switches; the Shortcuts page's are wide buttons.
		let hit = rows.iter().find(|&&(row, _)| {
			if page == SettingsPage::Layout {
				on_switch(row)
			} else {
				wide(row)
			}
		});
		if let Some(&(_, target)) = hit {
			return Some(target);
		}
		return on_save.then_some(SettingsTarget::Save);
	}
	if on_save {
		return Some(SettingsTarget::Save);
	}
	if (WALLPAPER_ROW * s..(WALLPAPER_ROW + 32) * s).contains(&y) {
		if (24 * s..176 * s).contains(&x) {
			return Some(SettingsTarget::ChooseWallpaper);
		}
		if (188 * s..284 * s).contains(&x) {
			return Some(SettingsTarget::ClearWallpaper);
		}
	}
	let (rx, _, rw, _) = switch_hit(width, RESTORE_ROW, s);
	if in_row(RESTORE_ROW) && (rx..rx + rw).contains(&x) {
		return Some(SettingsTarget::ToggleRestore);
	}
	let step = if (right(240)..right(192)).contains(&x) {
		-1
	} else if (right(72)..right(24)).contains(&x) {
		1
	} else {
		0
	};
	if step != 0 {
		let mut rows = vec![FONT_ROW, SCROLLBACK_ROW, STRENGTH_ROW];
		if cfg!(target_os = "macos") {
			rows.push(OPACITY_ROW);
		}
		if let Some(row) = rows.into_iter().find(|&row| in_row(row)) {
			return Some(SettingsTarget::Step(row, step));
		}
	}
	if in_row(THEME_ROW) {
		let (content_x, button_w) = (24 * s, 48 * s);
		let content_end = content_x + width.saturating_sub(48 * s);
		if x < content_x + button_w {
			return Some(SettingsTarget::Theme(-1));
		}
		if x >= content_end - button_w {
			return Some(SettingsTarget::Theme(1));
		}
	}
	None
}

/// [`SHORTCUT_ROWS`] as click targets.
const SHORTCUT_TARGETS: [(usize, SettingsTarget); 10] = targets(SHORTCUT_ROWS);
/// [`PANE_ROWS`] as click targets.
const PANE_TARGETS: [(usize, SettingsTarget); 8] = targets(PANE_ROWS);

/// Shortcut `rows` as click targets that record a new binding.
const fn targets<const N: usize>(
	rows: [(ShortcutAction, usize); N],
) -> [(usize, SettingsTarget); N] {
	let mut targets = [(0, SettingsTarget::Save); N];
	let mut i = 0;
	while i < N {
		targets[i] = (rows[i].1, SettingsTarget::SetShortcut(rows[i].0));
		i += 1;
	}
	targets
}

#[cfg(test)]
mod tests {
	use super::*;

	const SIZE: (usize, usize) = (760, 640);

	#[test]
	fn clicks_map_to_main_page_controls() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Main);
		// The page buttons now live in the sidebar: nothing at the old places.
		for x in [224, 380, 480, 536, 640, 735] {
			assert_eq!(target(x, 30), None, "x {x}");
		}
		assert_eq!(target(630, 600), Some(SettingsTarget::Save));
		assert_eq!(
			target(30, WALLPAPER_ROW + 10),
			Some(SettingsTarget::ChooseWallpaper)
		);
		assert_eq!(
			target(200, WALLPAPER_ROW + 10),
			Some(SettingsTarget::ClearWallpaper)
		);
		assert_eq!(
			target(720, RESTORE_ROW + 10),
			Some(SettingsTarget::ToggleRestore)
		);
		assert_eq!(
			target(530, FONT_ROW + 10),
			Some(SettingsTarget::Step(FONT_ROW, -1))
		);
		assert_eq!(
			target(700, SCROLLBACK_ROW + 10),
			Some(SettingsTarget::Step(SCROLLBACK_ROW, 1))
		);
		assert_eq!(target(30, THEME_ROW + 10), Some(SettingsTarget::Theme(-1)));
		assert_eq!(target(720, THEME_ROW + 10), Some(SettingsTarget::Theme(1)));
		assert_eq!(target(300, 100), None);
	}

	#[test]
	fn the_main_page_fits_a_small_window() {
		// Every row and the Save button stay inside the minimum window size.
		let last_row = [OPACITY_ROW, STRENGTH_ROW, RESTORE_ROW]
			.into_iter()
			.max()
			.unwrap();
		assert!(last_row + ROW_HEIGHT <= MIN_SIZE.1 - 52);
	}

	#[test]
	fn layout_page_toggles_are_clickable() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Layout);
		assert_eq!(
			target(690, 30),
			None,
			"no back button: the sidebar navigates"
		);
		assert_eq!(
			target(720, SHOW_TABS_ROW + 10),
			Some(SettingsTarget::ToggleTabs)
		);
		assert_eq!(
			target(700, VERTICAL_TABS_ROW + 10),
			Some(SettingsTarget::SetVerticalTabs(true))
		);
		assert_eq!(
			target(720, BOTTOM_BAR_ROW + 10),
			Some(SettingsTarget::ToggleBottomBar)
		);
		assert_eq!(
			target(530, PADDING_ROW + 10),
			Some(SettingsTarget::Step(PADDING_ROW, -1))
		);
		assert_eq!(
			target(700, PADDING_ROW + 10),
			Some(SettingsTarget::Step(PADDING_ROW, 1))
		);
		assert_eq!(target(600, PADDING_ROW + 10), None);
		assert!(PADDING_ROW + ROW_HEIGHT <= MIN_SIZE.1 - 52);
		assert_eq!(target(630, 600), Some(SettingsTarget::Save));
		// Main-page controls are not reachable from here.
		assert_eq!(target(30, 240), None);
		assert!(BOTTOM_BAR_ROW + ROW_HEIGHT <= MIN_SIZE.1 - 52);
	}

	#[test]
	fn shortcut_bindings_are_clickable() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Shortcuts);
		assert_eq!(
			target(690, 30),
			None,
			"no back button: the sidebar navigates"
		);
		assert_eq!(
			target(600, 112),
			Some(SettingsTarget::SetShortcut(ShortcutAction::NewTab))
		);
		assert_eq!(
			target(600, 164),
			Some(SettingsTarget::SetShortcut(ShortcutAction::CommandPalette))
		);
	}

	#[test]
	fn all_ten_shortcuts_are_clickable_above_the_footer() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Shortcuts);
		for (i, action) in ShortcutAction::TABS.into_iter().enumerate() {
			let top = 104 + 44 * i;
			assert_eq!(
				target(600, top + 1),
				Some(SettingsTarget::SetShortcut(action))
			);
			assert_eq!(
				target(600, top + ROW_HEIGHT + 2),
				None,
				"gap below {action:?}"
			);
		}
		let (_, last) = SHORTCUT_ROWS[SHORTCUT_ROWS.len() - 1];
		assert!(
			last + ROW_HEIGHT <= MIN_SIZE.1 - 52,
			"clear of the Save button"
		);
		assert_eq!(
			target(600, 510),
			Some(SettingsTarget::SetShortcut(ShortcutAction::ZoomReset))
		);
	}

	#[test]
	fn about_page_has_nothing_to_click() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::About);
		for (x, y) in [(630, 600), (750, 30), (610, 30), (400, 300), (30, 30)] {
			assert_eq!(target(x, y), None, "({x}, {y})");
		}
	}

	#[test]
	fn font_list_rows_are_clickable() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Fonts);
		assert_eq!(
			target(690, 30),
			None,
			"no back button: the sidebar navigates"
		);
		assert_eq!(
			target(40, FONT_LIST_TOP + 4),
			Some(SettingsTarget::FontRow(0))
		);
		assert_eq!(
			target(700, FONT_LIST_TOP + 2 * FONT_LIST_STRIDE + 20),
			Some(SettingsTarget::FontRow(2))
		);
		// The gap between rows and the margins are not rows.
		assert_eq!(target(40, FONT_LIST_TOP + ROW_HEIGHT + 1), None);
		assert_eq!(target(10, FONT_LIST_TOP + 4), None);
		assert_eq!(target(630, 600), Some(SettingsTarget::Save));
		let rows = font_list_rows(SIZE.1, 1);
		assert_eq!(rows, 10);
		let below = FONT_LIST_TOP + rows * FONT_LIST_STRIDE + 4;
		assert_eq!(target(40, below), None, "rows end above the footer");
		assert!(FONT_LIST_TOP + rows * FONT_LIST_STRIDE <= SIZE.1 - 64);
		assert_eq!(font_list_rows(SIZE.1 * 2, 2), rows);
	}

	#[test]
	fn hit_testing_scales_with_the_window() {
		assert_eq!(
			settings_target(
				(530 * 2, (FONT_ROW + 10) * 2),
				(SIZE.0 * 2, SIZE.1 * 2),
				2,
				SettingsPage::Main
			),
			Some(SettingsTarget::Step(FONT_ROW, -1))
		);
	}

	#[test]
	fn a_switch_answers_only_on_and_just_beside_itself() {
		// A 34 pixel switch 24 from the right edge of a 760 wide window.
		let layout = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Layout);
		let main = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Main);
		for (x, hit) in [
			(693, false),
			(694, true),
			(720, true),
			(743, true),
			(744, false),
		] {
			let want = hit.then_some(SettingsTarget::ToggleTabs);
			assert_eq!(layout(x, SHOW_TABS_ROW + 20), want, "layout x {x}");
			let want = hit.then_some(SettingsTarget::ToggleRestore);
			assert_eq!(main(x, RESTORE_ROW + 20), want, "main x {x}");
		}
	}

	#[test]
	fn the_old_wide_button_area_no_longer_flips_anything() {
		let layout = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Layout);
		for x in [530, 600, 640, 690] {
			for row in [SHOW_TABS_ROW, COMPACT_TABS_ROW, BOTTOM_BAR_ROW] {
				assert_eq!(layout(x, row + 20), None, "x {x} row {row}");
			}
		}
	}

	#[test]
	fn a_switch_answers_across_the_whole_height_of_its_row_and_no_further() {
		let layout = |y| settings_target((720, y), SIZE, 1, SettingsPage::Layout);
		assert_eq!(layout(SHOW_TABS_ROW), Some(SettingsTarget::ToggleTabs));
		assert_eq!(
			layout(SHOW_TABS_ROW + ROW_HEIGHT - 1),
			Some(SettingsTarget::ToggleTabs)
		);
		assert_eq!(layout(SHOW_TABS_ROW - 1), None);
		assert_eq!(layout(SHOW_TABS_ROW + ROW_HEIGHT), None);
	}

	#[test]
	fn every_layout_switch_flips_its_own_setting_and_no_other() {
		let layout = |row| settings_target((720, row + 20), SIZE, 1, SettingsPage::Layout);
		let expected = [
			(SHOW_TABS_ROW, SettingsTarget::ToggleTabs),
			(COMPACT_TABS_ROW, SettingsTarget::ToggleCompactTabs),
			(NEW_TAB_BUTTON_ROW, SettingsTarget::ToggleNewTabButton),
			(SETTINGS_BUTTON_ROW, SettingsTarget::ToggleSettingsButton),
			(BOTTOM_BAR_ROW, SettingsTarget::ToggleBottomBar),
		];
		for (row, target) in expected {
			assert_eq!(layout(row), Some(target), "row {row}");
		}
	}

	#[test]
	fn switches_answer_at_twice_the_scale_too() {
		let layout = |x, y| settings_target((x, y), (SIZE.0 * 2, SIZE.1 * 2), 2, SettingsPage::Layout);
		assert_eq!(
			layout(1440, (SHOW_TABS_ROW + 20) * 2),
			Some(SettingsTarget::ToggleTabs)
		);
		assert_eq!(layout(1060, (SHOW_TABS_ROW + 20) * 2), None);
	}

	#[test]
	fn the_shortcut_buttons_keep_their_wide_area() {
		let shortcuts = |x, row| settings_target((x, row + 10), SIZE, 1, SettingsPage::Shortcuts);
		let (_, row) = SHORTCUT_ROWS[0];
		assert!(shortcuts(530, row).is_some());
		assert!(shortcuts(720, row).is_some());
	}

	#[test]
	fn the_tab_position_control_names_top_on_its_left_half_and_side_on_its_right() {
		let layout = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Layout);
		let y = VERTICAL_TABS_ROW + 20;
		// A 156 wide control 24 from the right of a 760 wide window.
		assert_eq!(layout(580, y), Some(SettingsTarget::SetVerticalTabs(false)));
		assert_eq!(layout(657, y), Some(SettingsTarget::SetVerticalTabs(false)));
		assert_eq!(layout(658, y), Some(SettingsTarget::SetVerticalTabs(true)));
		assert_eq!(layout(735, y), Some(SettingsTarget::SetVerticalTabs(true)));
	}

	#[test]
	fn the_tab_position_control_answers_nowhere_off_its_track() {
		let layout = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Layout);
		let (top, bottom) = (VERTICAL_TABS_ROW + 6, VERTICAL_TABS_ROW + 6 + 28);
		assert_eq!(layout(579, VERTICAL_TABS_ROW + 20), None);
		assert_eq!(layout(736, VERTICAL_TABS_ROW + 20), None);
		assert_eq!(layout(650, top - 1), None);
		assert_eq!(layout(650, bottom), None);
		assert_eq!(layout(300, VERTICAL_TABS_ROW + 20), None, "over the label");
	}

	#[test]
	fn the_tab_position_control_is_a_layout_page_control_only() {
		for page in [
			SettingsPage::Main,
			SettingsPage::Shortcuts,
			SettingsPage::Fonts,
			SettingsPage::About,
		] {
			let target = settings_target((650, VERTICAL_TABS_ROW + 20), SIZE, 1, page);
			assert!(
				!matches!(target, Some(SettingsTarget::SetVerticalTabs(_))),
				"{page:?}"
			);
		}
	}

	#[test]
	fn the_tab_position_control_works_at_twice_the_scale() {
		let layout = |x, y| settings_target((x, y), (SIZE.0 * 2, SIZE.1 * 2), 2, SettingsPage::Layout);
		let y = (VERTICAL_TABS_ROW + 20) * 2;
		assert_eq!(
			layout(1170, y),
			Some(SettingsTarget::SetVerticalTabs(false))
		);
		assert_eq!(layout(1400, y), Some(SettingsTarget::SetVerticalTabs(true)));
	}

	#[test]
	fn the_layout_page_keeps_five_switches_and_the_segmented_row_between_them() {
		assert_eq!(LAYOUT_TOGGLES.len(), 5);
		assert!(
			LAYOUT_TOGGLES
				.iter()
				.all(|&(row, _)| row != VERTICAL_TABS_ROW)
		);
	}
}
