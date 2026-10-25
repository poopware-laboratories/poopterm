//! Tests of glyph bands: every horizontal ray crosses the same curves when
//! it tests only its band as when it tests them all, bands stay small, and
//! glyphs too large to band are refused.
use super::bands::{Banded, MAX_BANDS, MAX_CURVES};
use crate::fonts::{BUNDLED, Font, quads::Quad};

fn quad(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2]) -> Quad {
	Quad { p0, p1, p2 }
}

/// The signed crossings of the ray from `(x, y)` toward +x with `q`: +1
/// where the curve goes up through the ray, -1 where it goes down. A curve
/// counts at its lower end and not its upper, so joined curves count once.
fn crossings(q: &Quad, (x, y): (f32, f32)) -> i32 {
	// In f64, and without subtracting near-equal terms, so a curve that
	// ends a hair below the ray is not found crossing it.
	let [a, b, c] = [q.p0[1], q.p1[1], q.p2[1]].map(f64::from);
	// y(t) = (a - 2b + c)t² + 2(b - a)t + a
	let (qa, qb, qc) = (a - 2.0 * b + c, 2.0 * (b - a), a - f64::from(y));
	let mut roots = Vec::new();
	if qa.abs() < 1e-12 {
		if qb != 0.0 {
			roots.push(-qc / qb);
		}
	} else {
		let d = qb * qb - 4.0 * qa * qc;
		if d >= 0.0 {
			let half = -0.5 * (qb + qb.signum() * d.sqrt());
			roots.push(half / qa);
			if half != 0.0 {
				roots.push(qc / half);
			}
		}
	}
	let mut sum = 0;
	for t in roots.into_iter().filter(|t| (0.0..1.0).contains(t)) {
		let u = 1.0 - t;
		let [x0, x1, x2] = [q.p0[0], q.p1[0], q.p2[0]].map(f64::from);
		let cx = u * u * x0 + 2.0 * u * t * x1 + t * t * x2;
		let slope = 2.0 * u * (b - a) + 2.0 * t * (c - b);
		if cx > f64::from(x) && slope != 0.0 {
			sum += if slope > 0.0 { 1 } else { -1 };
		}
	}
	sum
}

fn winding_all(banded: &Banded, at: (f32, f32)) -> i32 {
	banded.curves.iter().map(|q| crossings(q, at)).sum()
}

fn winding_in_band(banded: &Banded, at: (f32, f32)) -> i32 {
	let band = banded.band(banded.band_at(at.1));
	band
		.iter()
		.map(|&i| crossings(&banded.curves[i as usize], at))
		.sum()
}

/// A deterministic generator for sample points.
fn next(state: &mut u64) -> f32 {
	*state ^= *state << 13;
	*state ^= *state >> 7;
	*state ^= *state << 17;
	(*state >> 40) as f32 / (1u64 << 24) as f32
}

/// Checks band and full winding agree at many points across `banded`,
/// including every band edge.
fn assert_bands_agree(banded: &Banded, what: &str) {
	let (bottom, top) = banded.y_range;
	let bands = banded.starts.len() - 1;
	let edges = (0..=bands).map(|i| bottom + (top - bottom) * i as f32 / bands as f32);
	let mut state = 0x2545_f491_4f6c_dd1d;
	let random = (0..400).map(|_| bottom - 5.0 + (top - bottom + 10.0) * next(&mut state));
	let x_span = banded
		.curves
		.iter()
		.flat_map(|q| [q.p0[0], q.p1[0], q.p2[0]]);
	let (left, right) = x_span.fold((f32::MAX, f32::MIN), |(l, r), x| (l.min(x), r.max(x)));
	for y in edges.chain(random) {
		for i in 0..=8 {
			let x = left - 1.0 + (right - left + 2.0) * i as f32 / 8.0;
			let (all, band) = (winding_all(banded, (x, y)), winding_in_band(banded, (x, y)));
			assert_eq!(all, band, "{what}: at ({x}, {y})");
		}
	}
}

fn square(x: f32, y: f32, side: f32) -> Vec<Quad> {
	let line = |a: [f32; 2], b: [f32; 2]| quad(a, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0], b);
	let [a, b, c, d] = [[x, y], [x + side, y], [x + side, y + side], [x, y + side]];
	vec![line(a, b), line(b, c), line(c, d), line(d, a)]
}

#[test]
fn a_square_needs_only_its_sides_in_each_band() {
	let banded = Banded::new(square(0.0, 0.0, 100.0)).unwrap();
	assert_eq!(banded.y_range, (0.0, 100.0));
	for band in 0..banded.starts.len() - 1 {
		assert_eq!(
			banded.band(band),
			[1, 3],
			"horizontal sides never cross a horizontal ray"
		);
	}
	assert_bands_agree(&banded, "square");
}

#[test]
fn rows_find_their_band_and_rows_outside_clamp_to_the_ends() {
	let mut curves = square(0.0, 0.0, 100.0);
	for i in 0..30 {
		curves.extend(square(i as f32 * 3.0, i as f32 * 3.0, 4.0));
	}
	let banded = Banded::new(curves).unwrap();
	let bands = banded.starts.len() - 1;
	assert!(bands > 1, "{bands} bands for 124 curves");
	assert_eq!(banded.band_at(-50.0), 0);
	assert_eq!(banded.band_at(0.0), 0);
	assert_eq!(banded.band_at(100.0), bands - 1);
	assert_eq!(banded.band_at(1e9), bands - 1);
	assert_eq!(banded.band_at(f32::NAN), 0);
	let height = 100.0 / bands as f32;
	for band in 0..bands {
		let middle = (band as f32 + 0.5) * height;
		assert_eq!(banded.band_at(middle), band);
	}
}

#[test]
fn curves_bulging_past_their_ends_are_listed_where_they_bulge() {
	// An arch rising from y 0 to 100 and one hanging from y 300 to 200,
	// beside a stack of squares, so the glyph spans several bands.
	let mut curves = vec![
		quad([0.0, 0.0], [50.0, 200.0], [100.0, 0.0]),
		quad([100.0, 0.0], [50.0, 0.0], [0.0, 0.0]),
		quad([0.0, 300.0], [50.0, 100.0], [100.0, 300.0]),
		quad([100.0, 300.0], [50.0, 300.0], [0.0, 300.0]),
	];
	for i in 0..20 {
		curves.extend(square(200.0, i as f32 * 15.0, 10.0));
	}
	let banded = Banded::new(curves).unwrap();
	assert!(banded.starts.len() > 8);
	assert_bands_agree(&banded, "arches");
	let rising = banded.band(banded.band_at(95.0));
	assert!(
		rising.contains(&0),
		"the rising arch's top is in its band: {rising:?}"
	);
	let hanging = banded.band(banded.band_at(205.0));
	assert!(
		hanging.contains(&2),
		"the hanging arch's bottom is in its band: {hanging:?}"
	);
}

#[test]
fn bands_hold_fewer_curves_than_the_glyph_in_bundled_glyphs() {
	let font = Font::from_bytes(BUNDLED, 0).unwrap();
	let (mut listed, mut tested) = (0, 0);
	for ch in ('!'..='~').chain("@&%§éßøΩЖ→─│┼█▀🐱".chars()) {
		let curves = font.quads(font.lookup_glyph_index(ch));
		if curves.is_empty() {
			continue;
		}
		let banded = Banded::new(curves).unwrap();
		assert_bands_agree(&banded, &ch.to_string());
		let bands = banded.starts.len() - 1;
		listed += banded.indices.len();
		tested += bands * banded.curves.len();
	}
	// A ray tests, on average, under half of a glyph's curves.
	assert!(listed * 2 < tested, "{listed} listed of {tested}");
}

#[test]
fn band_starts_are_ordered_and_end_at_the_indices() {
	let banded = Banded::new(square(0.0, 0.0, 10.0)).unwrap();
	assert_eq!(banded.starts[0], 0);
	assert!(banded.starts.windows(2).all(|pair| pair[0] <= pair[1]));
	assert_eq!(
		*banded.starts.last().unwrap() as usize,
		banded.indices.len()
	);
	assert!(banded.starts.len() - 1 <= MAX_BANDS);
}

#[test]
fn many_curves_use_many_bands_but_no_more_than_the_limit() {
	let count = |n: usize| {
		let curves = (0..n)
			.flat_map(|i| square(0.0, i as f32 * 2.0, 1.0))
			.collect();
		Banded::new(curves).unwrap().starts.len() - 1
	};
	// A square's two sides cross rays; about four such curves to a band.
	assert_eq!(
		[1, 2, 4, 25, 400, 1000].map(count),
		[1, 1, 2, 13, MAX_BANDS, MAX_BANDS]
	);
}

#[test]
fn flat_or_empty_glyphs_and_glyphs_with_too_many_curves_are_refused() {
	assert_eq!(Banded::new(Vec::new()), None);
	let flat = vec![quad([0.0, 5.0], [3.0, 5.0], [9.0, 5.0])];
	assert_eq!(Banded::new(flat), None, "nothing a ray could cross");
	let line = quad([0.0, 0.0], [0.0, 1.0], [0.0, 2.0]);
	assert!(Banded::new(vec![line; MAX_CURVES]).is_some());
	assert_eq!(Banded::new(vec![line; MAX_CURVES + 1]), None);
}

/// A closed path through `points`, each side a quad bent by `bend`.
fn closed(points: &[[f32; 2]], bend: f32) -> Vec<Quad> {
	let mut curves = Vec::new();
	for (i, &a) in points.iter().enumerate() {
		let b = points[(i + 1) % points.len()];
		let middle = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
		// Bend sideways to the side, so some sides bulge past their ends.
		let control = [
			middle[0] - (b[1] - a[1]) * bend,
			middle[1] + (b[0] - a[0]) * bend,
		];
		curves.push(quad(a, control, b));
	}
	curves
}

#[test]
fn random_closed_paths_wind_alike_in_their_bands() {
	let mut state = 0x9e37_79b9_7f4a_7c15;
	for path in 0..60 {
		let sides = 3 + path % 40;
		let points: Vec<_> = (0..sides)
			.map(|_| [next(&mut state) * 1000.0, next(&mut state) * 1000.0])
			.collect();
		let bend = next(&mut state) - 0.5;
		let banded = Banded::new(closed(&points, bend)).unwrap();
		assert_bands_agree(&banded, &format!("path {path}, {sides} sides"));
	}
}

#[test]
fn a_circle_lists_each_arc_only_in_the_bands_it_reaches() {
	// Sixty-four arcs on a circle of radius 500: every arc turns or tilts.
	let point = |i: usize| {
		let angle = i as f32 * std::f32::consts::TAU / 64.0;
		[500.0 * angle.cos(), 500.0 * angle.sin()]
	};
	let points: Vec<_> = (0..64).map(point).collect();
	let banded = Banded::new(closed(&points, 0.05)).unwrap();
	assert_eq!(banded.starts.len() - 1, MAX_BANDS);
	assert_bands_agree(&banded, "circle");
	for band in 0..MAX_BANDS {
		let listed = banded.band(band).len();
		assert!(listed < 64 / 2, "band {band} lists {listed} arcs");
	}
}

#[test]
fn hairline_and_huge_glyphs_band_like_any_other() {
	// A hairline a thousandth of a unit tall, and a box at the limits of
	// 16-bit font units.
	let hairline = closed(&[[0.0, 0.0], [900.0, 0.0005], [800.0, 0.001]], 0.0);
	let huge = closed(
		&[[-32768.0, -32768.0], [32767.0, -30000.0], [0.0, 32767.0]],
		0.2,
	);
	for (curves, what) in [(hairline, "hairline"), (huge, "huge")] {
		let banded = Banded::new(curves).unwrap();
		assert!(banded.y_range.0 < banded.y_range.1, "{what}");
		assert_bands_agree(&banded, what);
	}
}

#[test]
fn curves_turning_at_an_end_or_straight_in_y_span_their_ends() {
	// Each shape is closed by a straight side, so only the first curve
	// decides the range: it turns at its start, at its end, or is straight
	// in y (its control halfway, where the turning point is 0/0).
	for (first, range) in [
		(quad([0.0, 10.0], [50.0, 10.0], [100.0, 90.0]), (10.0, 90.0)),
		(quad([0.0, 10.0], [50.0, 90.0], [100.0, 90.0]), (10.0, 90.0)),
		(quad([0.0, 10.0], [50.0, 50.0], [100.0, 90.0]), (10.0, 90.0)),
	] {
		let closing = quad(first.p2, [50.0, 50.0], first.p0);
		let banded = Banded::new(vec![first, closing]).unwrap();
		assert_eq!(banded.y_range, range, "{first:?}");
	}
}
