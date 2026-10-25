//! Hit-testing for the Settings window: which control a click lands on.
use super::settings_view::*;
use crate::input::shortcuts::ShortcutAction;

/// The page the Settings window shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SettingsPage {
	#[default]
	Main,
	Layout,
	Shortcuts,
	About,
	Fonts,
}

/// A clickable control in the Settings window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsTarget {
	/// Opens a page from the main page's header.
	Open(SettingsPage),
	/// The "Settings" button on the other pages.
	Back,
	Save,
	ChooseWallpaper,
	ClearWallpaper,
	ToggleRestore,
	ToggleTabs,
	ToggleVerticalTabs,
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
}

/// Header buttons of the main page: page, distance of the left edge from
/// the window's right edge, and width, in logical pixels.
pub(super) const HEADER_BUTTONS: [(SettingsPage, usize, usize, &str); 4] = [
	(SettingsPage::Fonts, 464, 104, "Fonts"),
	(SettingsPage::Layout, 352, 104, "Layout"),
	(SettingsPage::Shortcuts, 240, 104, "Shortcuts"),
	(SettingsPage::About, 128, 104, "About"),
];

/// How many font list rows fit above the footer of a window `height`
/// physical pixels tall.
pub(super) fn font_list_rows(height: usize, scale: usize) -> usize {
	(height / scale.max(1)).saturating_sub(64 + FONT_LIST_TOP) / FONT_LIST_STRIDE
}

/// Toggles on the Layout page.
pub(super) const LAYOUT_TOGGLES: [(usize, SettingsTarget); 6] = [
	(SHOW_TABS_ROW, SettingsTarget::ToggleTabs),
	(VERTICAL_TABS_ROW, SettingsTarget::ToggleVerticalTabs),
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
	let in_header = (20 * s..56 * s).contains(&y);
	let right = |offset: usize| width.saturating_sub(offset * s);
	let (save_x, save_y) = (right(140), height.saturating_sub(52 * s));
	let on_save = (save_x..save_x + 120 * s).contains(&x) && (save_y..save_y + 36 * s).contains(&y);
	if page != SettingsPage::Main {
		if in_header && (right(164)..right(24)).contains(&x) {
			return Some(SettingsTarget::Back);
		}
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
		if let Some(&(_, target)) = rows
			.iter()
			.find(|(row, _)| in_row(*row) && (right(240)..right(24)).contains(&x))
		{
			return Some(target);
		}
		return on_save.then_some(SettingsTarget::Save);
	}
	if in_header {
		return HEADER_BUTTONS
			.iter()
			.find(|(_, offset, w, _)| (right(*offset)..right(*offset) + w * s).contains(&x))
			.map(|&(page, ..)| SettingsTarget::Open(page));
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
	if in_row(RESTORE_ROW) && (right(240)..right(24)).contains(&x) {
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
const SHORTCUT_TARGETS: [(usize, SettingsTarget); 10] = {
	let mut targets = [(0, SettingsTarget::Save); 10];
	let mut i = 0;
	while i < SHORTCUT_ROWS.len() {
		targets[i] = (
			SHORTCUT_ROWS[i].1,
			SettingsTarget::SetShortcut(SHORTCUT_ROWS[i].0),
		);
		i += 1;
	}
	targets
};

#[cfg(test)]
mod tests {
	use super::*;

	const SIZE: (usize, usize) = (760, 640);

	#[test]
	fn clicks_map_to_main_page_controls() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Main);
		assert_eq!(
			target(680, 30),
			Some(SettingsTarget::Open(SettingsPage::About))
		);
		assert_eq!(
			target(570, 30),
			Some(SettingsTarget::Open(SettingsPage::Shortcuts))
		);
		assert_eq!(
			target(460, 30),
			Some(SettingsTarget::Open(SettingsPage::Layout))
		);
		assert_eq!(
			target(350, 30),
			Some(SettingsTarget::Open(SettingsPage::Fonts))
		);
		assert_eq!(target(250, 30), None);
		assert_eq!(target(630, 600), Some(SettingsTarget::Save));
		assert_eq!(target(30, 240), Some(SettingsTarget::ChooseWallpaper));
		assert_eq!(target(200, 240), Some(SettingsTarget::ClearWallpaper));
		assert_eq!(target(530, 390), Some(SettingsTarget::ToggleRestore));
		assert_eq!(target(530, 280), Some(SettingsTarget::Step(FONT_ROW, -1)));
		assert_eq!(
			target(700, 330),
			Some(SettingsTarget::Step(SCROLLBACK_ROW, 1))
		);
		assert_eq!(target(30, 150), Some(SettingsTarget::Theme(-1)));
		assert_eq!(target(720, 150), Some(SettingsTarget::Theme(1)));
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
		assert_eq!(target(690, 30), Some(SettingsTarget::Back));
		assert_eq!(
			target(530, SHOW_TABS_ROW + 10),
			Some(SettingsTarget::ToggleTabs)
		);
		assert_eq!(
			target(530, VERTICAL_TABS_ROW + 10),
			Some(SettingsTarget::ToggleVerticalTabs)
		);
		assert_eq!(
			target(530, BOTTOM_BAR_ROW + 10),
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
		assert_eq!(target(690, 30), Some(SettingsTarget::Back));
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
		for (i, action) in ShortcutAction::ALL.into_iter().enumerate() {
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
	fn about_page_only_exposes_the_back_button() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::About);
		assert_eq!(target(630, 600), None);
		assert_eq!(target(750, 30), None);
		assert_eq!(target(610, 30), Some(SettingsTarget::Back));
	}

	#[test]
	fn header_buttons_clear_the_title_in_a_small_window() {
		// "Poopterm Settings" ends near x = 210 at the base size.
		let leftmost = HEADER_BUTTONS
			.iter()
			.map(|(_, offset, ..)| offset)
			.max()
			.unwrap();
		assert!(MIN_SIZE.0 - leftmost >= 212);
	}

	#[test]
	fn font_list_rows_are_clickable() {
		let target = |x, y| settings_target((x, y), SIZE, 1, SettingsPage::Fonts);
		assert_eq!(target(690, 30), Some(SettingsTarget::Back));
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
				(530 * 2, 280 * 2),
				(SIZE.0 * 2, SIZE.1 * 2),
				2,
				SettingsPage::Main
			),
			Some(SettingsTarget::Step(FONT_ROW, -1))
		);
	}
}
