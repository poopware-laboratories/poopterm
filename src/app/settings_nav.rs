//! The Settings window's sidebar: one item per page, and the click
//! mapping that sends everything right of it to the page.
use super::{
	settings_hit::{SettingsPage, SettingsTarget, settings_target},
	settings_pen::Pen,
};
use crate::{
	render::{
		blend::blend,
		scene::{Rect, Scene},
	},
	ui::{chrome_colors::ChromeColors, metrics::Corner},
};

/// The sidebar's width, in logical pixels.
pub(super) const NAV_WIDTH: usize = 188;
/// The pages in reading order, with their names.
pub(super) const ITEMS: [(SettingsPage, &str); 7] = [
	(SettingsPage::Main, "Appearance"),
	(SettingsPage::Layout, "Layout"),
	(SettingsPage::Fonts, "Fonts"),
	(SettingsPage::Shortcuts, "Shortcuts"),
	(SettingsPage::Panes, "Panes"),
	(SettingsPage::Diagnostics, "Diagnostics"),
	(SettingsPage::About, "About"),
];
/// The room above the first item, an item's height and the distance from
/// one item's top to the next's, in logical pixels.
const TOP: usize = 24;
const ITEM_HEIGHT: usize = 32;
const STRIDE: usize = 36;
/// The room between an item and the sidebar's edges.
const SIDE: usize = 8;

/// Item `index`'s rectangle in physical pixels.
pub(super) fn item_rect(index: usize, scale: usize) -> Rect {
	let width = NAV_WIDTH - 2 * SIDE;
	(
		SIDE * scale,
		(TOP + index * STRIDE) * scale,
		width * scale,
		ITEM_HEIGHT * scale,
	)
}

/// The page whose item holds physical `(x, y)`.
pub(super) fn nav_target((x, y): (usize, usize), scale: usize) -> Option<SettingsPage> {
	let held = |index| {
		let (left, top, w, h) = item_rect(index, scale);
		(left..left + w).contains(&x) && (top..top + h).contains(&y)
	};
	let index = (0..ITEMS.len()).find(|&index| held(index))?;
	Some(ITEMS[index].0)
}

/// Maps a click at physical `(x, y)` in a window `(width, height)` large:
/// the sidebar opens pages, and the rest is the page, which sees the click
/// as if it stood alone at the window's left edge.
pub(super) fn window_target(
	(x, y): (usize, usize),
	(width, height): (usize, usize),
	scale: usize,
	page: SettingsPage,
) -> Option<SettingsTarget> {
	let nav = NAV_WIDTH * scale;
	if x < nav {
		return nav_target((x, y), scale).map(SettingsTarget::Open);
	}
	settings_target(
		(x - nav, y),
		(width.saturating_sub(nav), height),
		scale,
		page,
	)
}

/// Draws the sidebar: its panel and edge line, then an item for each page,
/// the open `page` filled, the one under `cursor` lit, and the others quiet.
pub(super) fn compose_nav(
	pen: &mut Pen,
	p: &mut Scene,
	(page, cursor): (SettingsPage, (f64, f64)),
	(colors, fg): (&ChromeColors, u32),
) {
	let (s, height) = (pen.scale, p.height);
	let nav = NAV_WIDTH * s;
	let inner = height.saturating_sub(4 * s);
	p.rect(2 * s, 2 * s, nav - 2 * s, inner, colors.chrome);
	p.rect(nav - s, 2 * s, s, inner, colors.hairline);
	let radius = Corner::Control.radius(s);
	for (index, &(item, label)) in ITEMS.iter().enumerate() {
		let (x, y, w, h) = item_rect(index, s);
		let under = cursor.0 >= x as f64
			&& cursor.0 < (x + w) as f64
			&& cursor.1 >= y as f64
			&& cursor.1 < (y + h) as f64;
		p.focusable((x, y, w, h));
		let open = item == page;
		if open {
			p.round_rect(x, y, w, h, radius, blend(colors.chrome, fg, 36));
		} else if under {
			p.round_rect(x, y, w, h, radius, colors.hover);
		}
		let color = if open { fg } else { colors.muted };
		pen.tinted(p, x + 12 * s, y / s + 8, label, 0.82, color);
	}
}
