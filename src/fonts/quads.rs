//! Glyph outlines as quadratic curves only, the form a GPU shader solves
//! for coverage: lines become straight quadratics and cubics (CFF) are split
//! into quadratics within a tolerance.
use super::face::Sink;

/// A quadratic Bézier curve in font units: start, control and end points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Quad {
	pub p0: [f32; 2],
	pub p1: [f32; 2],
	pub p2: [f32; 2],
}

/// Quadratics one cubic may become, so fonts cannot ask for endless pieces.
const MAX_PIECES: usize = 16;

/// Collects an outline as closed contours of quadratic curves.
pub(crate) struct Quads {
	curves: Vec<Quad>,
	tolerance: f32,
	start: [f32; 2],
	at: [f32; 2],
}

impl Quads {
	/// Cubics are split until each piece stays within `tolerance` font units.
	pub(crate) fn new(tolerance: f32) -> Self {
		Self {
			curves: Vec::new(),
			tolerance,
			start: [0.0; 2],
			at: [0.0; 2],
		}
	}

	/// The curves, with the last contour closed.
	pub(crate) fn finish(mut self) -> Vec<Quad> {
		self.close();
		self.curves
	}

	/// Adds the curve from the current point, unless it goes nowhere or
	/// holds a point that is not finite.
	fn push(&mut self, p1: [f32; 2], p2: [f32; 2]) {
		let finite = [p1, p2].iter().flatten().all(|v| v.is_finite());
		if finite && (p1 != self.at || p2 != self.at) {
			self.curves.push(Quad {
				p0: self.at,
				p1,
				p2,
			});
		}
		if finite {
			self.at = p2;
		}
	}
}

impl Sink for Quads {
	fn move_to(&mut self, x: f32, y: f32) {
		self.close();
		(self.start, self.at) = ([x, y], [x, y]);
	}

	fn line_to(&mut self, x: f32, y: f32) {
		let middle = [(self.at[0] + x) / 2.0, (self.at[1] + y) / 2.0];
		self.push(middle, [x, y]);
	}

	fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
		self.push([x1, y1], [x, y]);
	}

	/// Splits the cubic into `n` equal spans and replaces each with the
	/// quadratic through its ends whose control is the mean of the two
	/// lines' meeting points. That quadratic strays from its span by at most
	/// `√3/36 · |p3 - 3p2 + 3p1 - p0|`, a third difference that shrinks with
	/// the cube of `n`.
	fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
		let p = [self.at, [x1, y1], [x2, y2], [x, y]];
		let third = [0, 1].map(|i| p[3][i] - 3.0 * p[2][i] + 3.0 * p[1][i] - p[0][i]);
		let error = 3f32.sqrt() / 36.0 * third[0].hypot(third[1]);
		let n = (error / self.tolerance).cbrt().ceil();
		let n = if n.is_finite() {
			n.clamp(1.0, MAX_PIECES as f32) as usize
		} else {
			MAX_PIECES
		};
		let point = |t: f32| {
			let u = 1.0 - t;
			let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
			[0, 1].map(|i| a * p[0][i] + b * p[1][i] + c * p[2][i] + d * p[3][i])
		};
		let slope = |t: f32| {
			let u = 1.0 - t;
			let (a, b, c) = (3.0 * u * u, 6.0 * u * t, 3.0 * t * t);
			[0, 1].map(|i| a * (p[1][i] - p[0][i]) + b * (p[2][i] - p[1][i]) + c * (p[3][i] - p[2][i]))
		};
		let h = 1.0 / n as f32;
		for k in 0..n {
			let (t0, t1) = (k as f32 * h, (k + 1) as f32 * h);
			let (a, d) = (point(t0), point(t1));
			let (da, dd) = (slope(t0), slope(t1));
			// The span's own cubic controls are `a + h/3 · da` and
			// `d - h/3 · dd`; the quadratic control is `(3(c1 + c2) - a - d) / 4`.
			let control = [0, 1].map(|i| {
				let (c1, c2) = (a[i] + h / 3.0 * da[i], d[i] - h / 3.0 * dd[i]);
				(3.0 * (c1 + c2) - a[i] - d[i]) / 4.0
			});
			self.push(control, d);
		}
	}

	/// A closing line, which adds nothing when the contour already ends at
	/// its start.
	fn close(&mut self) {
		self.line_to(self.start[0], self.start[1]);
	}
}
