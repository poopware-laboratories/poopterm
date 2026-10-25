//! The header bar: flat tabs, the active one raised, and flat icon buttons,
//! drawn from a plain model.
use super::{
	blend::blend,
	glyphs::FontSet,
	label::{Label, draw_label},
	scene::Scene,
};
use crate::ui::ui_font::text_width;

/// `(x, y, width, height)` in pixels.
pub(crate) type Rect = (usize, usize, usize, usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeaderColors {
	pub bar: u32,
	pub separator: u32,
	/// Under a hovered tab or button.
	pub hover: u32,
	/// Under the active tab.
	pub raised: u32,
	pub fg: u32,
	/// Disabled icons.
	pub dim: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Icon {
	Plus,
	Menu,
	Left,
	Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeaderButton {
	pub rect: Rect,
	pub icon: Icon,
	pub hovered: bool,
	pub enabled: bool,
}

/// The sign before a tab's title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Badge {
	Dot(u32),
	Bell(u32),
}

pub(crate) struct HeaderTab<'a> {
	pub x: usize,
	pub width: usize,
	pub label: Label<'a>,
	/// The tab's background, when it has one.
	pub fill: Option<u32>,
	pub badge: Option<Badge>,
	/// The share done, `None` when unknown, and the bar's color.
	pub progress: Option<(Option<u8>, u32)>,
	/// `Some(hovered)` when the close button shows.
	pub close: Option<bool>,
}

/// What every part of the chrome is drawn with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Style {
	pub scale: usize,
	/// The text size.
	pub px: f32,
	pub colors: HeaderColors,
}

pub(crate) struct Header<'a> {
	pub y: usize,
	pub height: usize,
	pub scale: usize,
	/// The text size.
	pub px: f32,
	pub colors: HeaderColors,
	pub tabs: Vec<HeaderTab<'a>>,
	pub buttons: Vec<HeaderButton>,
}

/// The rounded surface of a tab `width` wide at `x`, inside a header at `y`.
pub(crate) fn pill(x: usize, width: usize, (y, height): (usize, usize), scale: usize) -> Rect {
	(x, y + 6 * scale, width, height.saturating_sub(12 * scale))
}

/// The close button of a tab `width` wide at `x`, inside a header at `y`.
pub(crate) fn close_rect(
	x: usize,
	width: usize,
	(y, height): (usize, usize),
	scale: usize,
) -> Rect {
	let size = 22 * scale;
	let left = (x + width).saturating_sub(size + 6 * scale);
	(left, y + height.saturating_sub(size) / 2, size, size)
}

pub(crate) fn draw_header(p: &mut Scene, fonts: &mut FontSet, header: &Header) {
	let (y, height, scale) = (header.y, header.height, header.scale);
	let separator = (y + height).saturating_sub(scale);
	p.rect(0, y, p.width, height, header.colors.bar);
	p.rect(0, separator, p.width, scale, header.colors.separator);
	let style = Style {
		scale,
		px: header.px,
		colors: header.colors,
	};
	for tab in &header.tabs {
		draw_tab(
			p,
			fonts,
			style,
			pill(tab.x, tab.width, (y, height), scale),
			tab,
		);
	}
	for button in &header.buttons {
		draw_button(p, fonts, style, button);
	}
}

/// Draws `tab` on its surface `(x, y, width, height)`, its parts centered
/// across it.
pub(crate) fn draw_tab(
	p: &mut Scene,
	fonts: &mut FontSet,
	style: Style,
	rect: Rect,
	tab: &HeaderTab,
) {
	let (scale, colors, px) = (style.scale, style.colors, style.px);
	let (x, y, width, height) = rect;
	let surface = tab.fill.unwrap_or(colors.bar);
	if let Some(fill) = tab.fill {
		p.round_rect(x, y, width, height, 6 * scale, fill);
	}
	let mut left = x + 12 * scale;
	let (line_height, ascent) = line(fonts, px);
	let top = y + height.saturating_sub(line_height) / 2;
	match tab.badge {
		Some(Badge::Dot(color)) => {
			let size = 6 * scale;
			let dot_y = y + height.saturating_sub(size) / 2;
			p.round_rect(left, dot_y, size, size, size / 2, color);
			left += size + 8 * scale;
		}
		Some(Badge::Bell(color)) => {
			p.text(fonts, left, top, "!", px, ascent, color);
			left += fonts.advance('!', px) + 6 * scale;
		}
		None => {}
	}
	let right = if tab.close.is_some() { 28 } else { 12 } * scale;
	let room = (x + width).saturating_sub(right).saturating_sub(left);
	let font = (px, ascent);
	draw_label(p, fonts, &tab.label, top, font, (left, y, room, height));
	if let Some(hovered) = tab.close {
		let (cx, cy, size, _) = close_rect(x, width, (y, height), scale);
		if hovered {
			p.round_rect(cx, cy, size, size, size / 2, blend(surface, colors.fg, 40));
		}
		let center = (cx + size / 2, cy + size / 2);
		centered_text(p, fonts, "×", center, px, colors.fg);
	}
	if let Some(progress) = tab.progress {
		draw_progress(p, scale, rect, surface, progress);
	}
}

/// Draws a progress bar of `share`, or a dimmer full one when the share is
/// unknown, along the bottom of the tab surface `rect`.
pub(crate) fn draw_progress(
	p: &mut Scene,
	scale: usize,
	(x, y, width, height): Rect,
	surface: u32,
	(share, color): (Option<u8>, u32),
) {
	let (left, track) = (x + 12 * scale, width.saturating_sub(24 * scale));
	let row = (y + height).saturating_sub(3 * scale);
	p.rect(left, row, track, 2 * scale, blend(surface, color, 60));
	let done = share.map_or(track, |share| track * usize::from(share.min(100)) / 100);
	let shade = if share.is_some() { 255 } else { 140 };
	p.rect(left, row, done, 2 * scale, blend(surface, color, shade));
}

pub(crate) fn draw_button(p: &mut Scene, fonts: &mut FontSet, style: Style, button: &HeaderButton) {
	let (scale, colors) = (style.scale, style.colors);
	let (x, y, width, height) = button.rect;
	if button.hovered && button.enabled {
		p.round_rect(x, y, width, height, 6 * scale, colors.hover);
	}
	let color = if button.enabled {
		colors.fg
	} else {
		colors.dim
	};
	let (cx, cy) = (x + width / 2, y + height / 2);
	// Strokes 2 pixels wide at 1x, 3 at 2x.
	let stroke = scale + scale.div_ceil(2);
	let bar = |p: &mut Scene, (x, y): (usize, usize), (w, h): (usize, usize)| {
		p.rect(x, y, w, h, color);
	};
	match button.icon {
		Icon::Plus => {
			let length = 12 * scale;
			bar(p, (cx - length / 2, cy - stroke / 2), (length, stroke));
			bar(p, (cx - stroke / 2, cy - length / 2), (stroke, length));
		}
		Icon::Menu => {
			for row in [cy - 5 * scale, cy, cy + 5 * scale] {
				bar(p, (cx - 7 * scale, row - stroke / 2), (14 * scale, stroke));
			}
		}
		Icon::Left => centered_text(p, fonts, "‹", (cx, cy), style.px * 1.3, color),
		Icon::Right => centered_text(p, fonts, "›", (cx, cy), style.px * 1.3, color),
	}
}

/// The height of a line of text at `px`, and its ascent.
pub(crate) fn line(fonts: &FontSet, px: f32) -> (usize, i32) {
	let metrics = fonts.primary().horizontal_line_metrics(px);
	metrics.map_or((0, 0), |metrics| {
		let line = (metrics.ascent - metrics.descent).round() as usize;
		(line, metrics.ascent.round() as i32)
	})
}

/// Draws `text` with its advance and line centered on `(x, y)`.
pub(crate) fn centered_text(
	p: &mut Scene,
	fonts: &mut FontSet,
	text: &str,
	(x, y): (usize, usize),
	px: f32,
	color: u32,
) {
	let width = text_width(text, |ch| fonts.advance(ch, px));
	let (height, ascent) = line(fonts, px);
	let (left, top) = (x.saturating_sub(width / 2), y.saturating_sub(height / 2));
	p.text(fonts, left, top, text, px, ascent, color);
}
