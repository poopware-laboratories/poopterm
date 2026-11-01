//! The window's plain background, without a wallpaper: the theme's border
//! color within the border, its background inside, drawn with any [`Pen`]
//! so software and the GPU draw it alike.
use super::pen::Pen;

/// Fills `rows` (the first, and how many) from column `left` to the right
/// edge: `edge` within `border` pixels of the window's edges, `inside`
/// elsewhere.
pub(crate) fn fill(
	pen: &mut impl Pen,
	(top, rows): (usize, usize),
	left: usize,
	border: usize,
	(edge, inside): (u32, u32),
) {
	let (width, height) = pen.size();
	let bottom = top.saturating_add(rows).min(height);
	// The inside, as the columns and rows it spans; none when the border
	// takes half the window or more.
	let columns = (border.min(width), width.saturating_sub(border));
	let inner = (border.min(height), height.saturating_sub(border));
	let mut band = |from: usize, to: usize, x: usize, right: usize, color: u32| {
		if from < to && x < right {
			pen.rect(x, from, right - x, to - from, color);
		}
	};
	let (from, to) = (top.max(inner.0), bottom.min(inner.1));
	band(from, to, left, columns.0, edge);
	band(from, to, left.max(columns.0), columns.1, inside);
	band(from, to, left.max(columns.1), width, edge);
	band(top, bottom.min(inner.0), left, width, edge);
	band(top.max(inner.1), bottom, left, width, edge);
}

#[cfg(test)]
#[path = "background_tests.rs"]
mod tests;
