//! Popovers floating over the terminal: a panel with a text field, and rows
//! below it of which one may be selected.
use super::{
	glyphs::FontSet,
	header::{Rect, Style, line},
	scene::Scene,
};
use crate::ui::ui_font::{ellipsize_start_to_width, ellipsize_to_width, text_width};

pub(crate) struct Field<'a> {
	pub rect: Rect,
	pub text: &'a str,
	/// Shown dim while the text is empty.
	pub placeholder: &'a str,
	/// Dim text at the field's right end, such as a count.
	pub trailing: Option<String>,
}

pub(crate) struct PopoverRow<'a> {
	pub rect: Rect,
	pub text: &'a str,
	pub selected: bool,
}

pub(crate) struct Popover<'a> {
	pub rect: Rect,
	pub style: Style,
	pub field: Field<'a>,
	pub rows: Vec<PopoverRow<'a>>,
	/// A dim line under the field, where rows would go.
	pub note: Option<(Rect, &'a str)>,
}

/// Inside a field or row, the room left of the text.
const PADDING: usize = 12;

pub(crate) fn draw_popover(p: &mut Scene, fonts: &mut FontSet, popover: &Popover) {
	let (x, y, width, height) = popover.rect;
	let (scale, colors) = (popover.style.scale, popover.style.colors);
	let radius = 10 * scale;
	p.round_rect(x, y, width, height, radius, colors.separator);
	// The fill, a pixel in, leaves the outline.
	let (inner_width, inner_height) = (
		width.saturating_sub(2 * scale),
		height.saturating_sub(2 * scale),
	);
	p.round_rect(
		x + scale,
		y + scale,
		inner_width,
		inner_height,
		radius - scale,
		colors.bar,
	);
	draw_field(p, fonts, popover.style, &popover.field);
	for row in &popover.rows {
		let (x, y, width, height) = row.rect;
		if row.selected {
			p.round_rect(x, y, width, height, 6 * scale, colors.raised);
		}
		row_text(p, fonts, popover.style, row.rect, row.text, colors.fg);
	}
	if let Some((rect, note)) = popover.note {
		row_text(p, fonts, popover.style, rect, note, colors.dim);
	}
}

/// Draws `text` across a row, cut to fit inside its padding.
fn row_text(p: &mut Scene, fonts: &mut FontSet, style: Style, rect: Rect, text: &str, color: u32) {
	let room = rect.2.saturating_sub(2 * PADDING * style.scale);
	let text = ellipsize_to_width(text, room, |ch| fonts.advance(ch, style.px));
	let x = rect.0 + PADDING * style.scale;
	text_line(p, fonts, style, (x, rect), &text, color);
}

fn draw_field(p: &mut Scene, fonts: &mut FontSet, style: Style, field: &Field) {
	let (scale, px, colors) = (style.scale, style.px, style.colors);
	let (x, y, width, height) = field.rect;
	p.round_rect(x, y, width, height, 6 * scale, colors.hover);
	let padding = PADDING * scale;
	let mut room = width.saturating_sub(2 * padding);
	if let Some(trailing) = &field.trailing {
		let shown = ellipsize_to_width(trailing, room / 2, |ch| fonts.advance(ch, px));
		let shown_width = text_width(&shown, |ch| fonts.advance(ch, px));
		let right = (x + width).saturating_sub(padding + shown_width);
		text_line(p, fonts, style, (right, field.rect), &shown, colors.dim);
		room = room.saturating_sub(shown_width + padding);
	}
	let caret = 2 * scale;
	if field.text.is_empty() {
		// Past the caret, which stands at the start.
		let (left, room) = (x + padding + 2 * caret, room.saturating_sub(2 * caret));
		let placeholder = ellipsize_to_width(field.placeholder, room, |ch| fonts.advance(ch, px));
		text_line(
			p,
			fonts,
			style,
			(left, field.rect),
			&placeholder,
			colors.dim,
		);
	}
	// The end of the text, where the caret is, stays in view.
	let text = ellipsize_start_to_width(field.text, room.saturating_sub(caret), |ch| {
		fonts.advance(ch, px)
	});
	text_line(p, fonts, style, (x + padding, field.rect), &text, colors.fg);
	let after = x + padding + text_width(&text, |ch| fonts.advance(ch, px));
	let (line_height, _) = line(fonts, px);
	let top = y + height.saturating_sub(line_height) / 2;
	p.rect(after, top, caret, line_height, colors.fg);
}

/// Draws `text` from `x`, centered down `rect`.
fn text_line(
	p: &mut Scene,
	fonts: &mut FontSet,
	style: Style,
	(x, (_, y, _, height)): (usize, Rect),
	text: &str,
	color: u32,
) {
	let (line_height, ascent) = line(fonts, style.px);
	let top = y + height.saturating_sub(line_height) / 2;
	p.text(fonts, x, top, text, style.px, ascent, color);
}
