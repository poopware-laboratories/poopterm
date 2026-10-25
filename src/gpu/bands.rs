//! A glyph's quadratic curves sorted into horizontal bands, so a coverage
//! shader casting a horizontal ray tests only the curves near its row.
use crate::fonts::quads::Quad;

/// Bands per glyph, at most.
pub(crate) const MAX_BANDS: usize = 16;
/// Curves per glyph, at most; indices fit in 16 bits.
pub(crate) const MAX_CURVES: usize = 4096;

/// A glyph's curves and, per band, the indices of those that reach it.
#[derive(Debug, PartialEq)]
pub(crate) struct Banded {
	pub curves: Vec<Quad>,
	/// The bottom of the lowest band and the top of the highest, font units.
	pub y_range: (f32, f32),
	/// Where each band's indices start in `indices`; one more entry than
	/// there are bands, the last being `indices.len()`.
	pub starts: Vec<u32>,
	pub indices: Vec<u16>,
}

impl Banded {
	/// Bands `curves`, about four curves to a band. Refuses glyphs with no
	/// curve a horizontal ray could cross, and those with too many curves.
	pub(crate) fn new(curves: Vec<Quad>) -> Option<Self> {
		if curves.len() > MAX_CURVES {
			return None;
		}
		// Flat curves run along any ray that meets them and cross none.
		let spans: Vec<_> = curves
			.iter()
			.map(|q| (q.p0[1] != q.p1[1] || q.p1[1] != q.p2[1]).then(|| y_span(q)))
			.collect();
		let crossed = spans.iter().flatten();
		let bottom = crossed.clone().map(|span| span.0).reduce(f32::min)?;
		let top = crossed.clone().map(|span| span.1).reduce(f32::max)?;
		let count = crossed.count().div_ceil(4).clamp(1, MAX_BANDS);
		let mut banded = Self {
			curves,
			y_range: (bottom, top),
			starts: vec![0; count + 1],
			indices: Vec::new(),
		};
		// Each band in turn, so its indices are contiguous.
		for band in 0..count {
			for (index, span) in spans.iter().enumerate() {
				let Some((low, high)) = *span else {
					continue;
				};
				if (banded.band_at(low)..=banded.band_at(high)).contains(&band) {
					banded.indices.push(index as u16);
				}
			}
			banded.starts[band + 1] = banded.indices.len() as u32;
		}
		Some(banded)
	}

	/// The band holding height `y`, clamped to the bands.
	pub(crate) fn band_at(&self, y: f32) -> usize {
		let count = self.starts.len() - 1;
		let (bottom, top) = self.y_range;
		// `as` turns NaN into 0 and clamps the rest.
		let band = ((y - bottom) / (top - bottom) * count as f32) as usize;
		band.min(count - 1)
	}

	/// The curves listed in band `band`.
	pub(crate) fn band(&self, band: usize) -> &[u16] {
		&self.indices[self.starts[band] as usize..self.starts[band + 1] as usize]
	}
}

/// The lowest and highest points of `q`: its ends, or where it turns.
fn y_span(q: &Quad) -> (f32, f32) {
	let [a, b, c] = [q.p0[1], q.p1[1], q.p2[1]];
	let (mut low, mut high) = (a.min(c), a.max(c));
	let t = (a - b) / (a - 2.0 * b + c);
	if t > 0.0 && t < 1.0 {
		let u = 1.0 - t;
		let turn = u * u * a + 2.0 * u * t * b + t * t * c;
		(low, high) = (low.min(turn), high.max(turn));
	}
	(low, high)
}
