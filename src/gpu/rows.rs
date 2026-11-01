//! Grid rows kept between frames: each row's quads from the last frame,
//! reused for the rows the next one leaves unchanged, so a frame builds the
//! quads of the rows that changed only and the GPU still draws them all.
use super::quads::{Fill, Quad, QuadPen};
use crate::render::damage::GridDamage;

/// The last frame's grid rows, and which rows this frame redraws.
#[derive(Default)]
pub(crate) struct Rows {
	/// Each row's quads, from the top.
	kept: Vec<Vec<Quad>>,
	/// Whether `kept` holds the last frame's whole grid.
	valid: bool,
	/// What the next frame redraws, and the cell height a scroll moves
	/// rows by; `None` redraws every row.
	planned: Option<(GridDamage, usize)>,
	/// The rows this frame redraws; `None` for every row.
	redraw: Option<Vec<bool>>,
	/// Each row drawn this frame, and where its quads start.
	drawn: Vec<(usize, usize)>,
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

	/// Starts the frame's grid as planned, once the atlas started the
	/// frame: kept rows move and keep their pages in the atlas.
	pub(super) fn begin_rows(&mut self) {
		let rows = &mut self.rows;
		rows.drawn.clear();
		let planned = (rows.planned.take())
			.filter(|(damage, _)| rows.valid && damage.rows.len() == rows.kept.len());
		// Until this frame's grid ends.
		rows.valid = false;
		let Some((damage, height)) = planned else {
			rows.redraw = None;
			return;
		};
		// Rows scrolled off the top come back at the bottom, all redrawn.
		rows.kept.rotate_left(damage.scroll);
		let shift = (damage.scroll * height) as i32;
		let kept = rows.kept.iter_mut().zip(&damage.rows);
		for (quads, _) in kept.filter(|(_, redraw)| !**redraw) {
			for quad in quads {
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

	/// The grid of `count` rows is drawn: keeps the rows drawn, and puts
	/// every row's quads in order where the grid started.
	pub(super) fn grid_ends(&mut self, count: usize) {
		let Rows {
			kept, valid, drawn, ..
		} = &mut self.rows;
		let start = drawn.first().map_or(self.quads.len(), |&(_, at)| at);
		kept.resize_with(count, Vec::new);
		let ends = drawn.iter().skip(1).map(|&(_, at)| at);
		for (&(row, from), to) in drawn.iter().zip(ends.chain([self.quads.len()])) {
			kept[row].clear();
			kept[row].extend_from_slice(&self.quads[from..to]);
		}
		self.quads.truncate(start);
		for row in kept.iter() {
			self.quads.extend_from_slice(row);
		}
		*valid = true;
	}
}
