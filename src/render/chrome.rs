//! Window chrome drawn with any [`Pen`]: bordered buttons and lines of UI
//! text, laid out once for the software painter and the GPU's quads alike.
use super::{
	blend::blend,
	glyphs::FontSet,
	painter::{ButtonStyle, glyph_top},
	pen::{Ink, Pen},
};

/// A rounded button at `(x, y, w, h)`: an outline, and a face inside it
/// tinted by the accent when `hovered`.
pub(crate) fn button(
	pen: &mut impl Pen,
	(x, y, w, h): (usize, usize, usize, usize),
	style: ButtonStyle,
	hovered: bool,
) {
	let s = style.scale;
	let (outline, face) = if hovered {
		(style.accent, blend(style.background, style.accent, 52))
	} else {
		(
			blend(style.background, style.border, 90),
			blend(style.background, style.border, 38),
		)
	};
	pen.round_rect((x, y, w, h), 7 * s, outline);
	let inner = (
		x + s,
		y + s,
		w.saturating_sub(2 * s),
		h.saturating_sub(2 * s),
	);
	pen.round_rect(inner, 6 * s, face);
}

/// A line of `text` from the UI `fonts` whose top edge is `y`, its baseline
/// `ascent` below, clipped to `(x, y, width, height)` and the pen's surface.
#[allow(clippy::too_many_arguments)]
pub(crate) fn text_clipped(
	pen: &mut impl Pen,
	fonts: &mut FontSet,
	(x, y): (usize, usize),
	text: &str,
	px: f32,
	ascent: i32,
	color: u32,
	(clip_x, clip_y, clip_width, clip_height): (usize, usize, usize, usize),
) {
	let (width, height) = pen.size();
	let clip_right = clip_x.saturating_add(clip_width).min(width) as i32;
	let clip_bottom = clip_y.saturating_add(clip_height).min(height) as i32;
	let clip = (clip_x as i32, clip_y as i32, clip_right, clip_bottom);
	let mut cursor_x = x as i32;
	for ch in text.chars() {
		let glyph = fonts.glyph(ch, px);
		let origin = (cursor_x + glyph.metrics.xmin, glyph_top(glyph, y, ascent));
		pen.glyph(glyph, Ink::Ui(ch, px), origin, clip, false, color);
		cursor_x += glyph.metrics.advance_width.ceil() as i32;
		if cursor_x >= clip_right {
			break;
		}
	}
}
