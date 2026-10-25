//! Compact forms of drawn glyphs, for drawing them with little work: solid
//! ones as a few rectangles, antialiased ones trimmed to their ink.
use crate::{fonts::Metrics, render::glyphs::Glyph};

pub(crate) enum Drawn {
	/// Rectangles `(x, y, width, height)` from the cell's top left corner,
	/// for glyphs whose pixels are all fully covered or empty.
	Rects(Vec<(usize, usize, usize, usize)>),
	/// Coverage trimmed to the pixels with ink, at `(left, top)` in the cell.
	Bitmap {
		left: usize,
		top: usize,
		glyph: Glyph,
	},
}

/// The compact form of a `w`×`h` coverage bitmap.
pub(crate) fn compact(coverage: &[u8], (w, h): (usize, usize)) -> Drawn {
	if coverage.iter().all(|&c| c == 0 || c == 255) {
		return Drawn::Rects(rects(coverage, w));
	}
	let rows = coverage.chunks_exact(w);
	let inked = |row: &[u8]| row.iter().any(|&c| c > 0);
	let top = rows.clone().position(inked).unwrap_or(0);
	let bottom = h - rows.rev().position(inked).unwrap_or(0);
	let column = |x: usize| (top..bottom).any(|y| coverage[y * w + x] > 0);
	let left = (0..w).position(column).unwrap_or(0);
	let right = w - (0..w).rev().position(column).unwrap_or(0);
	let mut trimmed = Vec::with_capacity((right - left) * (bottom - top));
	for y in top..bottom {
		trimmed.extend_from_slice(&coverage[y * w + left..y * w + right]);
	}
	let metrics = Metrics {
		width: right - left,
		height: bottom - top,
		..Default::default()
	};
	Drawn::Bitmap {
		left,
		top,
		glyph: Glyph {
			metrics,
			coverage: trimmed,
			color: None,
		},
	}
}

/// Covers the inked pixels with rectangles: runs of ink in each band of
/// identical rows.
fn rects(coverage: &[u8], w: usize) -> Vec<(usize, usize, usize, usize)> {
	let mut rects = Vec::new();
	let rows: Vec<&[u8]> = coverage.chunks_exact(w).collect();
	let mut y = 0;
	while y < rows.len() {
		let band = rows[y..].iter().take_while(|&&row| row == rows[y]).count();
		let mut x = 0;
		while x < w {
			if rows[y][x] == 0 {
				x += 1;
				continue;
			}
			let run = rows[y][x..].iter().take_while(|&&c| c != 0).count();
			rects.push((x, y, run, band));
			x += run;
		}
		y += band;
	}
	rects
}
