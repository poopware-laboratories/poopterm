//! The vertical tab sidebar: tabs drawn like the header's, one per row, or
//! numbered on the narrow rail, and flat buttons below them.
use super::{
	blend::blend,
	glyphs::FontSet,
	header::{
		Badge, HeaderButton, HeaderTab, Rect, Style, centered_text, draw_button, draw_progress,
		draw_tab,
	},
	painter::Painter,
};

pub(crate) struct SidebarRow<'a> {
	/// The tab's number, from 1, which the rail shows instead of its title.
	pub number: usize,
	pub rect: Rect,
	pub tab: HeaderTab<'a>,
}

pub(crate) struct Sidebar<'a> {
	pub area: Rect,
	pub style: Style,
	/// Whether the tabs show as numbers on the narrow rail.
	pub rail: bool,
	pub rows: Vec<SidebarRow<'a>>,
	pub buttons: Vec<HeaderButton>,
}

/// The close button of a tab on the rail, in its top right corner, clear of
/// the number.
pub(crate) fn rail_close_rect((x, y, width, _): Rect, scale: usize) -> Rect {
	let size = 16 * scale;
	(
		(x + width).saturating_sub(size + 2 * scale),
		y + 2 * scale,
		size,
		size,
	)
}

/// Draws the whole sidebar. Its background is opaque, so nothing under it
/// needs restoring first.
pub(crate) fn draw_sidebar(p: &mut Painter, fonts: &mut FontSet, sidebar: &Sidebar) {
	let (x, y, width, height) = sidebar.area;
	let (style, scale) = (sidebar.style, sidebar.style.scale);
	p.rect(x, y, width, height, style.colors.bar);
	let edge = (x + width).saturating_sub(scale);
	p.rect(edge, y, scale, height, style.colors.separator);
	for row in &sidebar.rows {
		if sidebar.rail {
			draw_rail_tab(p, fonts, style, row);
		} else {
			draw_tab(p, fonts, style, row.rect, &row.tab);
		}
	}
	for button in &sidebar.buttons {
		draw_button(p, fonts, style, button);
	}
}

fn draw_rail_tab(p: &mut Painter, fonts: &mut FontSet, style: Style, row: &SidebarRow) {
	let (scale, colors, tab) = (style.scale, style.colors, &row.tab);
	let (x, y, width, height) = row.rect;
	let surface = tab.fill.unwrap_or(colors.bar);
	if let Some(fill) = tab.fill {
		p.round_rect(x, y, width, height, 6 * scale, fill);
	}
	let center = (x + width / 2, y + height / 2);
	centered_text(
		p,
		fonts,
		&row.number.to_string(),
		center,
		style.px,
		colors.fg,
	);
	// No room for a bell's mark: every badge is a dot left of the number.
	if let Some(Badge::Dot(color) | Badge::Bell(color)) = tab.badge {
		let size = 6 * scale;
		let top = y + height.saturating_sub(size) / 2;
		p.round_rect(x + 8 * scale, top, size, size, size / 2, color);
	}
	if let Some(hovered) = tab.close {
		let (cx, cy, size, _) = rail_close_rect(row.rect, scale);
		if hovered {
			p.round_rect(cx, cy, size, size, size / 2, blend(surface, colors.fg, 40));
		}
		let center = (cx + size / 2, cy + size / 2);
		centered_text(p, fonts, "×", center, style.px * 0.8, colors.fg);
	}
	if let Some(progress) = tab.progress {
		draw_progress(p, scale, row.rect, surface, progress);
	}
}
