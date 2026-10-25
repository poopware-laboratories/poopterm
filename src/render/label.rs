//! Tab labels: a title, then a status after a two-space gap, fitted into a
//! box by their measured widths.
use super::{glyphs::FontSet, scene::Scene};
use crate::ui::ui_font::{fit_label, text_width};

pub(crate) struct Label<'a> {
	pub title: &'a str,
	pub status: Option<&'a str>,
	/// The title's color, then the status's.
	pub colors: (u32, u32),
}

/// Draws `label` from the left of `clip = (x, y, width, height)`, its text
/// rows starting at `y`, with `font = (px, ascent)`, fitted into the box.
pub(crate) fn draw_label(
	p: &mut Scene,
	fonts: &mut FontSet,
	label: &Label,
	y: usize,
	(px, ascent): (f32, i32),
	clip: (usize, usize, usize, usize),
) {
	let mut advance = |ch| fonts.advance(ch, px);
	let (title, status) = fit_label(label.title, label.status, clip.2, &mut advance);
	let offset = text_width(&title, &mut advance) + 2 * advance(' ');
	let (title_color, status_color) = label.colors;
	p.text_clipped(fonts, clip.0, y, &title, px, ascent, title_color, clip);
	if !status.is_empty() {
		p.text_clipped(
			fonts,
			clip.0 + offset,
			y,
			&status,
			px,
			ascent,
			status_color,
			clip,
		);
	}
}
