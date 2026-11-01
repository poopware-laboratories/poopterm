//! Glyph rasterization: outlines are flattened into line segments whose
//! exact signed area is accumulated per pixel, then a SIMD prefix sum along
//! each row turns the accumulation into 8-bit coverage.
use std::simd::{f32x8, num::SimdFloat, simd_swizzle};

use super::face::{Bounds, Sink};

/// Flattening tolerance in pixels: far below what coverage can show.
const TOLERANCE: f32 = 0.02;
/// Line segments per curve, at most.
const MAX_STEPS: f32 = 64.0;

/// An outline recorded in font units, so it can be measured, then drawn.
#[derive(Default)]
pub(crate) struct Path(Vec<Segment>);

#[derive(Clone, Copy)]
enum Segment {
	Move(f32, f32),
	Line(f32, f32),
	Quad(f32, f32, f32, f32),
	Curve(f32, f32, f32, f32, f32, f32),
	Close,
}

impl Path {
	/// The bounds of the outline itself, which control points may stick
	/// out of: curves add their extremes, where the derivative is zero.
	/// Segments that stay on one point draw nothing and add nothing, so
	/// one-point contours (anchors, bounding-box marks) are left out.
	/// A curve passes its endpoints only where a control point does, so
	/// extremes are solved for only then, which is rare in fonts.
	pub(crate) fn bounds(&self) -> Option<Bounds> {
		let mut bounds = None;
		let mut at = (0.0, 0.0);
		// Whether control coordinate `c` lies outside the span `a..=b`.
		let outside = |c: f32, a: f32, b: f32| c < a.min(b) || c > a.max(b);
		for &segment in &self.0 {
			let (to, moved, extremes) = match segment {
				Segment::Move(x, y) => {
					at = (x, y);
					continue;
				}
				Segment::Line(x, y) => ((x, y), (x, y) != at, [None; 4]),
				Segment::Quad(x1, y1, x, y) => {
					let t = |a, b, c| outside(b, a, c).then(|| quad_extreme(a, b, c)).flatten();
					let moved = (x1, y1) != at || (x, y) != at;
					((x, y), moved, [t(at.0, x1, x), t(at.1, y1, y), None, None])
				}
				Segment::Curve(x1, y1, x2, y2, x, y) => {
					let t = |a, b, c, d| {
						if outside(b, a, d) || outside(c, a, d) {
							cubic_extremes(a, b, c, d)
						} else {
							[None; 2]
						}
					};
					let ([tx0, tx1], [ty0, ty1]) = (t(at.0, x1, x2, x), t(at.1, y1, y2, y));
					let moved = (x1, y1) != at || (x2, y2) != at || (x, y) != at;
					((x, y), moved, [tx0, tx1, ty0, ty1])
				}
				Segment::Close => continue,
			};
			if moved {
				Bounds::include(&mut bounds, at.0, at.1);
				for t in extremes.into_iter().flatten() {
					let (x, y) = point_at(at, segment, t);
					Bounds::include(&mut bounds, x, y);
				}
				Bounds::include(&mut bounds, to.0, to.1);
			}
			at = to;
		}
		bounds
	}
}

/// Where a quadratic's derivative vanishes, if inside the curve.
fn quad_extreme(a: f32, b: f32, c: f32) -> Option<f32> {
	let denominator = a - 2.0 * b + c;
	let t = (a - b) / denominator;
	(denominator != 0.0 && t > 0.0 && t < 1.0).then_some(t)
}

/// Where a cubic's derivative vanishes inside the curve: the roots of
/// `3(at² + bt + c)`.
fn cubic_extremes(p0: f32, p1: f32, p2: f32, p3: f32) -> [Option<f32>; 2] {
	let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
	let b = 2.0 * (p0 - 2.0 * p1 + p2);
	let c = p1 - p0;
	let inside = |t: f32| (t > 0.0 && t < 1.0).then_some(t);
	if a.abs() < 1e-6 {
		return [(b != 0.0).then_some(-c / b).and_then(inside), None];
	}
	let discriminant = b * b - 4.0 * a * c;
	if discriminant < 0.0 {
		return [None, None];
	}
	let root = discriminant.sqrt();
	[
		inside((-b + root) / (2.0 * a)),
		inside((-b - root) / (2.0 * a)),
	]
}

/// The point at `t` on the curve `segment` starting at `from`.
fn point_at(from: (f32, f32), segment: Segment, t: f32) -> (f32, f32) {
	let u = 1.0 - t;
	match segment {
		Segment::Quad(x1, y1, x, y) => (
			u * u * from.0 + 2.0 * u * t * x1 + t * t * x,
			u * u * from.1 + 2.0 * u * t * y1 + t * t * y,
		),
		Segment::Curve(x1, y1, x2, y2, x, y) => {
			let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
			(
				a * from.0 + b * x1 + c * x2 + d * x,
				a * from.1 + b * y1 + c * y2 + d * y,
			)
		}
		_ => from,
	}
}

impl Sink for Path {
	fn move_to(&mut self, x: f32, y: f32) {
		self.0.push(Segment::Move(x, y));
	}

	fn line_to(&mut self, x: f32, y: f32) {
		self.0.push(Segment::Line(x, y));
	}

	fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
		self.0.push(Segment::Quad(x1, y1, x, y));
	}

	fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
		self.0.push(Segment::Curve(x1, y1, x2, y2, x, y));
	}

	fn close(&mut self) {
		self.0.push(Segment::Close);
	}
}

/// Coverage accumulation for a `width`×`height` bitmap. Rows have one
/// spare cell, so area spilling past the last pixel needs no check.
pub(crate) struct Raster {
	width: usize,
	height: usize,
	stride: usize,
	cells: Vec<f32>,
}

impl Raster {
	pub(crate) fn new(width: usize, height: usize) -> Self {
		let stride = width + 1;
		Self {
			width,
			height,
			stride,
			cells: vec![0.0; stride * height],
		}
	}

	/// Draws `path`, mapping font units `(x, y)` to pixels
	/// `(x · scale - left, top - y · scale)`.
	pub(crate) fn draw(&mut self, path: &Path, scale: f32, left: f32, top: f32) {
		let map = |x: f32, y: f32| (x * scale - left, top - y * scale);
		let (mut start, mut at) = ((0.0, 0.0), (0.0, 0.0));
		for &segment in &path.0 {
			match segment {
				Segment::Move(x, y) => {
					self.line(at, start);
					(start, at) = (map(x, y), map(x, y));
				}
				Segment::Line(x, y) => {
					let to = map(x, y);
					self.line(at, to);
					at = to;
				}
				Segment::Quad(x1, y1, x, y) => {
					let (c, to) = (map(x1, y1), map(x, y));
					self.quad(at, c, to);
					at = to;
				}
				Segment::Curve(x1, y1, x2, y2, x, y) => {
					let (c1, c2, to) = (map(x1, y1), map(x2, y2), map(x, y));
					self.curve(at, c1, c2, to);
					at = to;
				}
				Segment::Close => {
					self.line(at, start);
					at = start;
				}
			}
		}
		self.line(at, start);
	}

	/// A quadratic curve as `n` lines, where the chord error `|p0 - 2p1 + p2| / 4n²`
	/// stays under the tolerance.
	fn quad(&mut self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) {
		let (ddx, ddy) = (p0.0 - 2.0 * p1.0 + p2.0, p0.1 - 2.0 * p1.1 + p2.1);
		let steps = ((ddx.hypot(ddy) / (4.0 * TOLERANCE)).sqrt().ceil()).clamp(1.0, MAX_STEPS);
		let mut from = p0;
		for i in 1..=steps as usize {
			let t = i as f32 / steps;
			let u = 1.0 - t;
			let to = (
				u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
				u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
			);
			self.line(from, to);
			from = to;
		}
	}

	/// A cubic curve as `n` lines; its second derivative is at most six
	/// times the larger control polygon bend, so the error is `3M / 4n²`.
	fn curve(&mut self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)) {
		let bend = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
			(a.0 - 2.0 * b.0 + c.0).hypot(a.1 - 2.0 * b.1 + c.1)
		};
		let m = bend(p0, p1, p2).max(bend(p1, p2, p3));
		let steps = ((3.0 * m / (4.0 * TOLERANCE)).sqrt().ceil()).clamp(1.0, MAX_STEPS);
		let mut from = p0;
		for i in 1..=steps as usize {
			let t = i as f32 / steps;
			let u = 1.0 - t;
			let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
			let to = (
				a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
				a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
			);
			self.line(from, to);
			from = to;
		}
	}

	/// Accumulates the line from `p0` to `p1`, clipped to the bitmap rows.
	fn line(&mut self, p0: (f32, f32), p1: (f32, f32)) {
		if p0.1 == p1.1
			|| !(p0.0.is_finite() && p0.1.is_finite() && p1.0.is_finite() && p1.1.is_finite())
		{
			return;
		}
		// Downward lines add area and upward ones remove it; only the
		// magnitude of the winding sum matters.
		let (direction, (tx, ty), (bx, by)) = if p0.1 < p1.1 {
			(1.0, p0, p1)
		} else {
			(-1.0, p1, p0)
		};
		let dxdy = (bx - tx) / (by - ty);
		// Walk the rows the line crosses, carrying x from one boundary to
		// the next.
		let (mut y, end) = (ty.max(0.0), by.min(self.height as f32));
		let mut x = tx + (y - ty) * dxdy;
		let mut row = y as usize;
		while y < end {
			let next = end.min((row + 1) as f32);
			let next_x = x + (next - y) * dxdy;
			self.span(row, x, next_x, (next - y) * direction);
			(x, y, row) = (next_x, next, row + 1);
		}
	}

	/// Adds a line piece within one row, spanning `xa..xb` with height
	/// `dy`. In each column it covers, the pixel gets the area right of the
	/// piece, and every pixel further right the full `dy`; the prefix sum
	/// supplies the latter from the cell after it.
	/// Lines arrive finite, so plain comparisons replace the NaN-aware
	/// `min`, `max` and `clamp`, which cost several instructions each.
	fn span(&mut self, row: usize, xa: f32, xb: f32, dy: f32) {
		let right = self.width as f32;
		let (lo, hi) = if xa < xb { (xa, xb) } else { (xb, xa) };
		// Area left of the bitmap lands in column 0; right of it, nowhere.
		let lo = if lo > 0.0 { lo } else { 0.0 };
		let hi = if hi < right { hi } else { right };
		if lo >= right {
			return;
		}
		let cells = &mut self.cells[row * self.stride..][..self.stride];
		let first = lo as usize;
		let first_x = first as f32;
		if hi <= first_x + 1.0 {
			let mid = (lo + hi) * 0.5 - first_x;
			cells[first] += dy * (1.0 - mid);
			cells[first + 1] += dy * mid;
			return;
		}
		let per_x = dy / (hi - lo);
		let last = ceil(hi) - 1;
		for column in first..=last {
			let x = column as f32;
			let l = if lo > x { lo } else { x };
			let r = if hi < x + 1.0 { hi } else { x + 1.0 };
			let (part, mid) = (per_x * (r - l), (l + r) * 0.5 - x);
			cells[column] += part * (1.0 - mid);
			cells[column + 1] += part * mid;
		}
	}

	/// Coverage, one byte per pixel, rows top to bottom.
	pub(crate) fn coverage(&self) -> Vec<u8> {
		let mut out = vec![0u8; self.width * self.height];
		if self.width == 0 {
			return out;
		}
		for (cells, out) in self
			.cells
			.chunks_exact(self.stride)
			.zip(out.chunks_exact_mut(self.width))
		{
			let cells = &cells[..self.width];
			let (chunks, rest) = cells.as_chunks::<8>();
			let mut carry = f32x8::splat(0.0);
			for (chunk, out) in chunks.iter().zip(out.as_chunks_mut::<8>().0) {
				let sums = prefix_sum(f32x8::from_array(*chunk)) + carry;
				carry = f32x8::splat(sums[7]);
				*out = to_bytes(sums).to_array();
			}
			let mut sum = carry[0];
			for (cell, out) in rest.iter().zip(&mut out[chunks.len() * 8..]) {
				sum += cell;
				*out = (sum.abs().min(1.0) * 255.0 + 0.5) as u8;
			}
		}
		out
	}
}

/// `x.ceil()` for `x >= 0` (0 below): without SSE4.1, the baseline x86-64
/// target makes `ceil` and `floor` library calls, and truncating is
/// already `floor` for non-negative values.
#[inline]
fn ceil(x: f32) -> usize {
	let whole = x as usize;
	whole + usize::from((whole as f32) < x)
}

/// Running totals within one vector: three shifted adds.
#[inline]
fn prefix_sum(v: f32x8) -> f32x8 {
	let zero = f32x8::splat(0.0);
	let v = v + simd_swizzle!(v, zero, [8, 0, 1, 2, 3, 4, 5, 6]);
	let v = v + simd_swizzle!(v, zero, [8, 9, 0, 1, 2, 3, 4, 5]);
	v + simd_swizzle!(v, zero, [8, 9, 10, 11, 0, 1, 2, 3])
}

#[inline]
fn to_bytes(sums: f32x8) -> std::simd::u8x8 {
	(sums.abs().simd_min(f32x8::splat(1.0)) * f32x8::splat(255.0) + f32x8::splat(0.5)).cast()
}
