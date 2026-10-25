//! Tests of outlines as quadratic curves: contours stay closed and joined,
//! cubics are split within the tolerance, and rasterizing the quadratics
//! gives the coverage of the original outline.
use super::{
	BUNDLED, Font,
	face::Sink,
	quads::{Quad, Quads},
	raster::{Path, Raster},
};

fn quad(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2]) -> Quad {
	Quad { p0, p1, p2 }
}

/// Sends a few drawing calls to a fresh [`Quads`].
fn quads(tolerance: f32, draw: impl FnOnce(&mut dyn Sink)) -> Vec<Quad> {
	let mut quads = Quads::new(tolerance);
	draw(&mut quads);
	quads.finish()
}

#[test]
fn lines_become_straight_quadratics_and_contours_close_themselves() {
	let curves = quads(0.1, |s| {
		s.move_to(0.0, 0.0);
		s.line_to(10.0, 0.0);
		s.line_to(10.0, 20.0);
	});
	assert_eq!(
		curves,
		[
			quad([0.0, 0.0], [5.0, 0.0], [10.0, 0.0]),
			quad([10.0, 0.0], [10.0, 10.0], [10.0, 20.0]),
			quad([10.0, 20.0], [5.0, 10.0], [0.0, 0.0]),
		]
	);
}

#[test]
fn a_contour_closed_at_its_start_needs_no_closing_curve() {
	let curves = quads(0.1, |s| {
		s.move_to(0.0, 0.0);
		s.line_to(4.0, 0.0);
		s.line_to(0.0, 4.0);
		s.line_to(0.0, 0.0);
		s.close();
	});
	assert_eq!(curves.len(), 3);
	assert_eq!(curves[2].p2, [0.0, 0.0]);
}

#[test]
fn each_move_closes_the_contour_before_it() {
	let curves = quads(0.1, |s| {
		s.move_to(0.0, 0.0);
		s.line_to(4.0, 0.0);
		s.line_to(4.0, 4.0);
		s.move_to(10.0, 10.0);
		s.line_to(12.0, 10.0);
		s.line_to(12.0, 12.0);
		s.close();
	});
	let ends: Vec<_> = curves.iter().map(|q| (q.p0, q.p2)).collect();
	assert_eq!(
		ends,
		[
			([0.0, 0.0], [4.0, 0.0]),
			([4.0, 0.0], [4.0, 4.0]),
			([4.0, 4.0], [0.0, 0.0]),
			([10.0, 10.0], [12.0, 10.0]),
			([12.0, 10.0], [12.0, 12.0]),
			([12.0, 12.0], [10.0, 10.0]),
		]
	);
}

#[test]
fn points_that_go_nowhere_add_no_curves() {
	let curves = quads(0.1, |s| {
		s.move_to(3.0, 3.0);
		s.line_to(3.0, 3.0);
		s.quad_to(3.0, 3.0, 3.0, 3.0);
		s.curve_to(3.0, 3.0, 3.0, 3.0, 3.0, 3.0);
		s.close();
		s.move_to(7.0, 7.0);
	});
	assert_eq!(curves, []);
}

#[test]
fn quadratics_pass_through_unchanged() {
	let curves = quads(0.1, |s| {
		s.move_to(0.0, 0.0);
		s.quad_to(5.0, 9.0, 10.0, 0.0);
		s.close();
	});
	assert_eq!(curves[0], quad([0.0, 0.0], [5.0, 9.0], [10.0, 0.0]));
	assert_eq!(curves.len(), 2, "and the closing line");
}

#[test]
fn a_cubic_that_is_a_raised_quadratic_stays_one_quadratic() {
	// The quadratic (0,0) (30,60) (90,0) raised to a cubic: controls at
	// 1/3 and 2/3 of the way to the quadratic control.
	let curves = quads(0.01, |s| {
		s.move_to(0.0, 0.0);
		s.curve_to(20.0, 40.0, 50.0, 40.0, 90.0, 0.0);
	});
	assert_eq!(curves[0], quad([0.0, 0.0], [30.0, 60.0], [90.0, 0.0]));
}

fn cubic_at(p: [[f32; 2]; 4], t: f32) -> [f32; 2] {
	let u = 1.0 - t;
	let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
	[0, 1].map(|i| a * p[0][i] + b * p[1][i] + c * p[2][i] + d * p[3][i])
}

fn quad_at(q: &Quad, t: f32) -> [f32; 2] {
	let u = 1.0 - t;
	[0, 1].map(|i| u * u * q.p0[i] + 2.0 * u * t * q.p1[i] + t * t * q.p2[i])
}

/// The farthest any point of the quadratics lies from the cubic, measured
/// to a 4000-segment polyline of it, whose own error is negligible.
fn farthest(cubic: [[f32; 2]; 4], curves: &[Quad]) -> f32 {
	let on_cubic: Vec<_> = (0..=4000)
		.map(|i| cubic_at(cubic, i as f32 / 4000.0))
		.collect();
	let distance = |p: [f32; 2]| {
		let to_segment = |s: &[[f32; 2]]| {
			let (a, b) = (s[0], s[1]);
			let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
			let length = dx * dx + dy * dy;
			let t = if length == 0.0 {
				0.0
			} else {
				(((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
			};
			(a[0] + t * dx - p[0]).hypot(a[1] + t * dy - p[1])
		};
		on_cubic.windows(2).map(to_segment).fold(f32::MAX, f32::min)
	};
	let points = curves
		.iter()
		.flat_map(|q| (0..=50).map(move |i| quad_at(q, i as f32 / 50.0)));
	points.map(distance).fold(0.0, f32::max)
}

const CUBICS: [[[f32; 2]; 4]; 4] = [
	// A quarter circle, an S, a loop and a cusp.
	[[100.0, 0.0], [100.0, 55.2], [55.2, 100.0], [0.0, 100.0]],
	[[0.0, 0.0], [100.0, 0.0], [0.0, 100.0], [100.0, 100.0]],
	[[0.0, 0.0], [150.0, 100.0], [-50.0, 100.0], [100.0, 0.0]],
	[[0.0, 0.0], [100.0, 100.0], [0.0, 100.0], [100.0, 0.0]],
];

#[test]
fn cubics_are_split_into_joined_quadratics_within_the_tolerance() {
	for cubic in CUBICS {
		for tolerance in [2.0, 0.5, 0.05] {
			let curves = quads(tolerance, |s| {
				s.move_to(cubic[0][0], cubic[0][1]);
				let [_, c1, c2, end] = cubic;
				s.curve_to(c1[0], c1[1], c2[0], c2[1], end[0], end[1]);
				s.line_to(cubic[0][0], cubic[0][1]);
			});
			let pieces = &curves[..curves.len() - 1];
			assert_eq!(pieces.first().unwrap().p0, cubic[0]);
			assert_eq!(pieces.last().unwrap().p2, cubic[3]);
			for pair in pieces.windows(2) {
				assert_eq!(pair[0].p2, pair[1].p0, "{cubic:?}: pieces join");
			}
			let error = farthest(cubic, pieces);
			assert!(
				error <= tolerance * 1.05 + 0.01,
				"{cubic:?} at {tolerance}: off by {error}"
			);
		}
	}
}

#[test]
fn a_finer_tolerance_takes_more_pieces_up_to_a_limit() {
	let count = |tolerance: f32| {
		let [_, c1, c2, end] = CUBICS[1];
		let curves = quads(tolerance, |s| {
			s.move_to(0.0, 0.0);
			s.curve_to(c1[0], c1[1], c2[0], c2[1], end[0], end[1]);
		});
		curves.len() - 1
	};
	let counts = [10.0, 1.0, 0.1, 0.01, 1e-9].map(count);
	assert!(
		counts.windows(2).all(|pair| pair[0] <= pair[1]),
		"{counts:?}"
	);
	assert!(counts[0] < counts[3], "{counts:?}");
	assert_eq!(
		counts[4], 16,
		"untrusted fonts cannot ask for endless pieces"
	);
}

#[test]
fn curves_through_points_that_are_not_finite_are_dropped_and_drawing_goes_on() {
	let curves = quads(0.1, |s| {
		s.move_to(0.0, 0.0);
		s.line_to(f32::NAN, 1.0);
		s.curve_to(1.0, f32::INFINITY, 2.0, 2.0, 3.0, 3.0);
		s.quad_to(5.0, 5.0, 6.0, 0.0);
	});
	assert_eq!(
		curves,
		[
			quad([0.0, 0.0], [5.0, 5.0], [6.0, 0.0]),
			quad([6.0, 0.0], [3.0, 0.0], [0.0, 0.0]),
		]
	);
}

#[test]
fn a_cubic_takes_the_fewest_pieces_its_error_bound_allows() {
	// The S's third difference is (400, -200), so one quadratic strays by up
	// to √3/36 · 447.2 = 21.5 units, and n pieces by 21.5 / n³.
	let count = |tolerance: f32| {
		let [_, c1, c2, end] = CUBICS[1];
		quads(tolerance, |s| {
			s.move_to(0.0, 0.0);
			s.curve_to(c1[0], c1[1], c2[0], c2[1], end[0], end[1]);
		})
		.len()
			- 1
	};
	assert_eq!([30.0, 10.0, 1.0, 0.1, 0.006].map(count), [1, 2, 3, 6, 16]);
}

#[test]
fn every_piece_count_ends_exactly_at_the_cubics_end() {
	let [start, c1, c2, end] = [[0.1, 0.7], [91.3, 3.3], [7.9, 97.1], [100.3, 99.9]];
	for pieces in 1..=16 {
		// The S above, scaled so `pieces` pieces just meet the tolerance.
		let tolerance = 21.6 / (pieces as f32).powi(3) * 0.99;
		let curves = quads(tolerance, |s| {
			s.move_to(start[0], start[1]);
			s.curve_to(c1[0], c1[1], c2[0], c2[1], end[0], end[1]);
		});
		assert_eq!(curves[curves.len() - 2].p2, end, "{pieces} pieces");
		assert_eq!(
			curves.last().unwrap().p0,
			end,
			"and the closing line starts there"
		);
	}
}

/// Rasterizes `draw` and `curves` at `px` pixels per `em` units, and
/// returns the largest coverage difference between the two.
fn coverage_difference(draw: &dyn Fn(&mut dyn Sink), curves: &[Quad], em: f32, px: f32) -> u8 {
	let mut path = Path::default();
	draw(&mut path);
	let mut from_quads = Path::default();
	let mut at = None;
	for q in curves {
		if at != Some(q.p0) {
			from_quads.move_to(q.p0[0], q.p0[1]);
		}
		from_quads.quad_to(q.p1[0], q.p1[1], q.p2[0], q.p2[1]);
		at = Some(q.p2);
	}
	let Some(bounds) = path.bounds() else {
		return 0;
	};
	let scale = px / em;
	let (left, top) = (
		(bounds.x_min * scale).floor() - 1.0,
		(bounds.y_max * scale).ceil() + 1.0,
	);
	let width = ((bounds.x_max * scale).ceil() - left) as usize + 2;
	let height = (top - (bounds.y_min * scale).floor()) as usize + 2;
	let raster = |path: &Path| {
		let mut raster = Raster::new(width, height);
		raster.draw(path, scale, left, top);
		raster.coverage()
	};
	let (want, got) = (raster(&path), raster(&from_quads));
	want
		.iter()
		.zip(&got)
		.map(|(a, b)| a.abs_diff(*b))
		.max()
		.unwrap_or(0)
}

/// `cubic` cut into `n` equal spans, each exact, by de Casteljau's
/// construction: cutting at `t` splits every control segment there.
fn pieces(cubic: [[f32; 2]; 4], n: usize) -> Vec<[[f32; 2]; 4]> {
	let lerp = |a: [f32; 2], b: [f32; 2], t: f32| [0, 1].map(|i| a[i] + (b[i] - a[i]) * t);
	let mut rest = cubic;
	let mut out = Vec::new();
	for k in 0..n {
		// Cut the remaining span so the first part is one of the n.
		let t = 1.0 / (n - k) as f32;
		let [p0, p1, p2, p3] = rest;
		let (a, b, c) = (lerp(p0, p1, t), lerp(p1, p2, t), lerp(p2, p3, t));
		let (d, e) = (lerp(a, b, t), lerp(b, c, t));
		let f = lerp(d, e, t);
		out.push([p0, a, d, f]);
		rest = [f, e, c, p3];
	}
	out
}

#[test]
fn quadratics_rasterize_like_the_cubics_they_replace() {
	for cubic in CUBICS {
		for px in [16.0, 64.0, 400.0] {
			// The rasterizer flattens a cubic into at most 64 lines, too few
			// for its tolerance at large sizes, so the reference draws the
			// cubic as 16 exact pieces.
			let draw = |s: &mut dyn Sink| {
				s.move_to(cubic[0][0], cubic[0][1]);
				for [_, c1, c2, end] in pieces(cubic, 16) {
					s.curve_to(c1[0], c1[1], c2[0], c2[1], end[0], end[1]);
				}
				s.close();
			};
			// A tolerance of 1/4096 em, as glyphs use. The rasterizer flattens
			// both outlines within 0.02 px each, so an edge may move by both
			// of those and the tolerance: that much of a pixel's coverage.
			let tolerance: f32 = 100.0 / 4096.0;
			let moved = 2.0 * 0.02 + tolerance * px / 100.0;
			let bound = (255.0 * moved).ceil() as u8 + 1;
			let difference = coverage_difference(&draw, &quads(tolerance, draw), 100.0, px);
			assert!(
				difference <= bound,
				"{cubic:?} at {px}px: {difference} > {bound}"
			);
		}
	}
}

#[test]
fn bundled_glyphs_rasterize_the_same_from_their_quadratics() {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	let sample = ('!'..='~').chain("éßøΩЖ→─│┼█▀".chars());
	let mut checked = 0;
	for ch in sample {
		let glyph = font.lookup_glyph_index(ch);
		let curves = font.quads(glyph);
		assert!(!curves.is_empty() || ch == ' ', "{ch} has an outline");
		let draw = |s: &mut dyn Sink| font.outline_into(glyph, s);
		// TrueType outlines hold no cubics, so no tolerance applies.
		assert_eq!(curves, quads(f32::MAX, |s| draw(s)), "{ch}");
		for px in [12.0, 31.0] {
			let difference = coverage_difference(&draw, &curves, font.units_per_em(), px);
			assert!(difference <= 1, "{ch} at {px}px: {difference}");
		}
		checked += 1;
	}
	assert!(checked > 90);
}

/// Every installed font: `cargo test --release -- --ignored installed_quads`.
#[test]
#[ignore = "reads the fonts installed on this machine"]
fn installed_quads_rasterize_like_their_outlines() {
	let (mut checked, mut worst) = (0, (0, String::new()));
	for ((path, index), _) in super::system_fonts().faces() {
		let Ok(font) = std::fs::read(path).map(|bytes| Font::from_bytes(bytes, index)) else {
			continue;
		};
		let Ok(font) = font else {
			continue;
		};
		for ch in ('!'..='~').chain("éßøΩЖ→".chars()) {
			let glyph = font.lookup_glyph_index(ch);
			if glyph == 0 {
				continue;
			}
			let draw = |s: &mut dyn Sink| font.outline_into(glyph, s);
			let em = font.units_per_em();
			let difference = coverage_difference(&draw, &font.quads(glyph), em, 24.0);
			if difference > worst.0 {
				worst = (difference, format!("{ch} in {}", path.display()));
			}
			checked += 1;
		}
	}
	eprintln!("{checked} glyphs, worst {} ({})", worst.0, worst.1);
	// At 24 px: both flattenings (0.02 px each) and 1/4096 em.
	assert!(checked > 0 && worst.0 <= (255.0f32 * (0.04 + 24.0 / 4096.0)).ceil() as u8 + 1);
}
