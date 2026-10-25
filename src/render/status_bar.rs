//! The status bar: flat, in the header's colors, with text at both ends.
use super::{
	glyphs::FontSet,
	header::{Rect, Style, line},
	painter::Painter,
};
use crate::ui::ui_font::{ellipsize_to_width, text_width};

pub(crate) struct StatusBar {
	pub rect: Rect,
	pub style: Style,
	/// The text at the left and its color.
	pub left: (String, u32),
	/// The text at the right and its color; it keeps its room first.
	pub right: (String, u32),
}

pub(crate) fn draw_status_bar(p: &mut Painter, fonts: &mut FontSet, bar: &StatusBar) {
	let (x, y, width, height) = bar.rect;
	let (scale, px, colors) = (bar.style.scale, bar.style.px, bar.style.colors);
	p.rect(x, y, width, height, colors.bar);
	p.rect(x, y, width, scale, colors.separator);
	let (line_height, ascent) = line(fonts, px);
	let top = y + height.saturating_sub(line_height) / 2;
	let (padding, gap) = (12 * scale, 24 * scale);
	let mut advance = |ch| fonts.advance(ch, px);
	let room = width.saturating_sub(2 * padding);
	let right = ellipsize_to_width(&bar.right.0, room, &mut advance);
	let right_width = text_width(&right, &mut advance);
	let left_room = room.saturating_sub(right_width + gap);
	let left = ellipsize_to_width(&bar.left.0, left_room, &mut advance);
	p.text(fonts, x + padding, top, &left, px, ascent, bar.left.1);
	let right_x = (x + width).saturating_sub(padding + right_width);
	p.text(fonts, right_x, top, &right, px, ascent, bar.right.1);
}
