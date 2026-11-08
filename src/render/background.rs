//! The window's background: the theme's border color within the border,
//! and inside it the theme's background or the wallpaper, drawn with any
//! [`Pen`] so software and the GPU draw it alike.
use super::pen::Pen;
use crate::graphics::{Bitmap, Placement, Rect};
use std::sync::Arc;

/// Fills `rows` (the first, and how many) from column `left` to the right
/// edge: `edge` within `border` pixels of the window's edges, `inside`
/// elsewhere.
pub(crate) fn fill(
	pen: &mut impl Pen,
	rows: (usize, usize),
	left: usize,
	border: usize,
	(edge, inside): (u32, u32),
) {
	paint(pen, rows, left, border, edge, |pen, x, y, w, h| {
		pen.rect(x, y, w, h, inside);
	});
}

/// [`fill`] with the inside showing `image`, the wallpaper made the
/// inside's size, so its top-left pixel sits at the inside's corner.
pub(crate) fn wallpaper(
	pen: &mut impl Pen,
	rows: (usize, usize),
	left: usize,
	border: usize,
	edge: u32,
	image: &Arc<Bitmap>,
) {
	let placement = Placement::natural(image.clone(), 0, 0);
	let corner = (border as isize, border as isize);
	paint(pen, rows, left, border, edge, |pen, x, y, width, height| {
		let clip = Rect {
			x,
			y,
			width,
			height,
		};
		pen.placement(clip, &placement, corner, (1, 1));
	});
}

/// Fills `rect`, which lies inside the border, with `inside`, or with the
/// wallpaper `image` as [`wallpaper`] places it.
pub(crate) fn fill_inside(
	pen: &mut impl Pen,
	rect: Rect,
	border: usize,
	(inside, image): (u32, Option<&Arc<Bitmap>>),
) {
	let Some(image) = image else {
		return pen.rect(rect.x, rect.y, rect.width, rect.height, inside);
	};
	let placement = Placement::natural(image.clone(), 0, 0);
	pen.placement(rect, &placement, (border as isize, border as isize), (1, 1));
}

/// Paints `rows` from column `left`: `edge` within `border` pixels of the
/// window's edges, and the inside's part in them with `inside`, given its
/// left, top, width and height.
fn paint<P: Pen>(
	pen: &mut P,
	(top, rows): (usize, usize),
	left: usize,
	border: usize,
	edge: u32,
	mut inside: impl FnMut(&mut P, usize, usize, usize, usize),
) {
	let (width, height) = pen.size();
	let bottom = top.saturating_add(rows).min(height);
	// The inside, as the columns and rows it spans; none when the border
	// takes half the window or more.
	let columns = (border.min(width), width.saturating_sub(border));
	let inner = (border.min(height), height.saturating_sub(border));
	let mut band = |pen: &mut P, from: usize, to: usize, x: usize, right: usize, color| {
		if from < to && x < right {
			match color {
				Some(color) => pen.rect(x, from, right - x, to - from, color),
				None => inside(pen, x, from, right - x, to - from),
			}
		}
	};
	let (from, to) = (top.max(inner.0), bottom.min(inner.1));
	band(pen, from, to, left, columns.0, Some(edge));
	band(pen, from, to, left.max(columns.0), columns.1, None);
	band(pen, from, to, left.max(columns.1), width, Some(edge));
	band(pen, top, bottom.min(inner.0), left, width, Some(edge));
	band(pen, top.max(inner.1), bottom, left, width, Some(edge));
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
