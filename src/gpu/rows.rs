//! Grid rows kept between frames: each row's quads from the last frame,
//! reused for the rows the next one leaves unchanged, so a frame builds the
//! quads of the rows that changed only and the GPU still draws them all.
//! The rows stay where the last frame put them, among its quads, so a frame
//! drawing every row copies none.
use super::quads::{Fill, Quad, QuadPen};
use crate::render::damage::GridDamage;
use std::ops::Range;

/// The last frame's grid rows, and which rows this frame redraws.
#[derive(Default)]
pub(crate) struct Rows {
	/// The last frame's quads, once this one began.
	last: Vec<Quad>,
	/// Where each grid row's quads are, from the top: in the frame's quads
	/// once its grid ended, in `last` once the next frame began.
	spans: Vec<Range<usize>>,
	/// Whether `spans` holds the last frame's whole grid.
	valid: bool,
	/// What the next frame redraws, and the cell height a scroll moves
	/// rows by; `None` redraws every row.
	planned: Option<(GridDamage, usize)>,
	/// The rows this frame redraws; `None` for every row.
	redraw: Option<Vec<bool>>,
	/// Each row drawn this frame, and where its quads start.
	drawn: Vec<(usize, usize)>,
	/// The quads of the rows drawn, while the grid is put in order.
	spare: Vec<Quad>,
}

/// `damage`, with the rows a scroll leaves wrong in kept quads redrawn as
/// well: the first, as the padding above the grid was the old first
/// row's, and the old last, which carries the padding below the grid.
fn after_scroll(mut damage: GridDamage) -> GridDamage {
	let rows = damage.rows.len();
	if damage.scroll > 0 && rows > 0 {
		damage.rows[0] = true;
		if let Some(last) = (rows - 1).checked_sub(damage.scroll) {
			damage.rows[last] = true;
		}
	}
	damage
}

impl QuadPen {
	/// Whether the pen holds the last frame's grid rows, so the next frame
	/// may redraw only some.
	pub(crate) fn keeps_rows(&self) -> bool {
		self.rows.valid
	}

	/// Has the next frame redraw only `damage`'s rows, and those a scroll
	/// leaves wrong, keeping the rest moved up `damage.scroll` rows of
	/// `cell_height` pixels; every row without `damage`, or when the pen
	/// holds no rows of the same number.
	pub(crate) fn plan_rows(&mut self, damage: Option<GridDamage>, cell_height: usize) {
		self.rows.planned = damage.map(|damage| (after_scroll(damage), cell_height));
	}

	/// The rows this frame redraws, as planned; `None` for every row.
	pub(crate) fn rows_to_draw(&self) -> Option<&[bool]> {
		self.rows.redraw.as_deref()
	}

	/// Starts the frame's quads, empty, and its grid as planned, once the
	/// atlas started the frame: kept rows move and keep their pages in the
	/// atlas.
	pub(super) fn begin_rows(&mut self) {
		let rows = &mut self.rows;
		std::mem::swap(&mut self.quads, &mut rows.last);
		self.quads.clear();
		rows.drawn.clear();
		let planned = (rows.planned.take())
			.filter(|(damage, _)| rows.valid && damage.rows.len() == rows.spans.len());
		// Until this frame's grid ends.
		rows.valid = false;
		let Some((mut damage, height)) = planned else {
			rows.redraw = None;
			return;
		};
		// Rows scrolled off the top come back at the bottom, all redrawn.
		rows.spans.rotate_left(damage.scroll);
		// The frame lists its images anew: rows showing one are drawn again.
		for (span, redraw) in rows.spans.iter().zip(&mut damage.rows) {
			*redraw |= rows.last[span.clone()]
				.iter()
				.any(|quad| matches!(quad.fill, Fill::Image(..)));
		}
		let shift = (damage.scroll * height) as i32;
		let kept = rows.spans.iter().zip(&damage.rows);
		for (span, _) in kept.filter(|(_, redraw)| !**redraw) {
			for quad in &mut rows.last[span.clone()] {
				quad.at.1 -= shift;
				quad.clip.1 -= shift;
				quad.clip.3 -= shift;
				if let Fill::Coverage(slot) | Fill::Color(slot) = quad.fill {
					self.atlas.touch(slot.page);
				}
			}
		}
		rows.redraw = Some(damage.rows);
	}

	/// Grid row `row` starts drawing here.
	pub(super) fn row_starts(&mut self, row: usize) {
		self.rows.drawn.push((row, self.quads.len()));
	}

	/// The grid of `count` rows is drawn: puts every row's quads in order
	/// where the grid started, the kept ones from the last frame, and
	/// remembers where each row is.
	pub(super) fn grid_ends(&mut self, count: usize) {
		let Rows {
			last,
			spans,
			valid,
			drawn,
			spare,
			..
		} = &mut self.rows;
		let (start, end) = (
			drawn.first().map_or(self.quads.len(), |&(_, at)| at),
			self.quads.len(),
		);
		let ends = drawn.iter().skip(1).map(|&(_, at)| at).chain([end]);
		let segments = drawn
			.iter()
			.zip(ends)
			.map(|(&(row, from), to)| (row, from..to));
		*valid = true;
		// The grid draws its rows in order.
		if drawn.len() == count {
			// Every row drawn: the quads are where they belong.
			spans.clear();
			spans.extend(segments.map(|(_, span)| span));
			return;
		}
		spare.clear();
		spare.extend_from_slice(&self.quads[start..]);
		self.quads.truncate(start);
		let mut segments = segments.map(|(row, span)| (row, span.start - start..span.end - start));
		let mut next = segments.next();
		spans.resize(count, 0..0);
		for (row, span) in spans.iter_mut().enumerate() {
			let at = self.quads.len();
			match next.take_if(|(drawn, _)| *drawn == row) {
				Some((_, drawn)) => {
					self.quads.extend_from_slice(&spare[drawn]);
					next = segments.next();
				}
				None => self.quads.extend_from_slice(&last[span.clone()]),
			}
			*span = at..self.quads.len();
		}
	}
}
