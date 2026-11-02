//! The window buttons' icons, drawn in code as the design draws them: lines
//! on a 12-unit square, 1.5 units wide with round ends, made into coverage
//! at any scale so both renderers draw them as glyphs.
use super::{boxdraw::segment_distance, glyphs::Glyph};
use crate::fonts::Metrics;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum WindowIcon {
	Minimize,
	Maximize,
	/// Maximize's icon while the window is maximized.
	Restore,
	Close,
}

/// An icon's side, before scaling.
pub(crate) const SIZE: usize = 12;
/// Samples across each pixel, in each direction.
const SAMPLES: usize = 4;

type Line = Vec<(f32, f32)>;

/// The points of a quarter circle around `center`, from `start` degrees on
/// clockwise (y grows down).
fn arc(center: (f32, f32), radius: f32, start: f32) -> impl Iterator<Item = (f32, f32)> {
	(0..=6).map(move |step| {
		let angle = (start + 15.0 * step as f32).to_radians();
		(
			center.0 + radius * angle.cos(),
			center.1 + radius * angle.sin(),
		)
	})
}

/// The outline of the square at `(x, y)` with corners rounded to `radius`.
fn rounded_square((x, y): (f32, f32), side: f32, radius: f32) -> Line {
	let (near, far) = (radius, side - radius);
	let corners = [(far, near), (far, far), (near, far), (near, near)];
	let mut line: Line = (corners.into_iter().zip([-90.0, 0.0, 90.0, 180.0]))
		.flat_map(|((cx, cy), start)| arc((x + cx, y + cy), radius, start))
		.collect();
	line.push(line[0]);
	line
}

fn lines(icon: WindowIcon) -> Vec<Line> {
	match icon {
		WindowIcon::Minimize => vec![vec![(2.0, 6.5), (10.0, 6.5)]],
		WindowIcon::Maximize => vec![rounded_square((2.0, 2.0), 8.0, 1.5)],
		WindowIcon::Restore => {
			// The square behind shows only its top and right sides.
			let mut back = vec![(4.0, 2.25)];
			back.extend(arc((8.5, 3.5), 1.25, -90.0));
			back.push((9.75, 8.0));
			vec![rounded_square((1.75, 4.0), 6.25, 1.25), back]
		}
		WindowIcon::Close => vec![vec![(2.5, 2.5), (9.5, 9.5)], vec![(9.5, 2.5), (2.5, 9.5)]],
	}
}

/// `icon` as coverage, `SIZE * scale` pixels square.
pub(crate) fn rasterize(icon: WindowIcon, scale: usize) -> Glyph {
	let (lines, side) = (lines(icon), SIZE * scale);
	let inked = |point| {
		let near = |pair: &[(f32, f32)]| segment_distance(point, pair[0], pair[1]) <= 0.75;
		lines.iter().any(|line| line.windows(2).any(near))
	};
	let unit = |pixel: usize, sample: usize| {
		(pixel as f32 + (sample as f32 + 0.5) / SAMPLES as f32) / scale as f32
	};
	let pixel = |index: usize| {
		let (x, y) = (index % side, index / side);
		let at = |sample: usize| (unit(x, sample % SAMPLES), unit(y, sample / SAMPLES));
		let hits = (0..SAMPLES * SAMPLES).filter(|&s| inked(at(s))).count();
		((hits * 255 + SAMPLES * SAMPLES / 2) / (SAMPLES * SAMPLES)) as u8
	};
	Glyph {
		metrics: Metrics {
			xmin: 0,
			ymin: 0,
			width: side,
			height: side,
			advance_width: side as f32,
		},
		coverage: (0..side * side).map(pixel).collect(),
		color: None,
	}
}
