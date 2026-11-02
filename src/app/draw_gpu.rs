//! Drawing a frame of the main window on the GPU, all of it as quads, the
//! wallpaper and images as textures. The pen keeps the grid rows that did
//! not change since the last frame, so only the others are drawn again. A
//! frame the GPU cannot draw (glyphs the atlas cannot hold, images past its
//! budget) hands the window to software, as any other GPU failure does.
use super::{
	App,
	canvas::{DrawKey, Plan},
	gpu::{Gpu, after_drawn, drawn_as, remembers},
	renderer::Renderer,
};
use crate::gpu::quads::QuadPen;
use std::time::Instant;

impl App {
	/// Draws the frame on `gpu`: the renderer to draw the next with,
	/// software once the GPU failed.
	pub(super) fn draw_on_gpu(
		&mut self,
		mut gpu: Box<Gpu>,
		key: DrawKey,
		size: (usize, usize),
	) -> Option<Renderer> {
		// The canvas remembers the frame whose rows the pen keeps.
		if !gpu.keeps_rows() || self.full_redraw_only {
			self.canvas.invalidate();
		}
		// Kept rows move on the GPU, over a background drawn again.
		let plan = (self.canvas).plan(&self.frame, key, size, true);
		let damage = match plan {
			Plan::Full => None,
			Plan::Partial(damage) => Some(damage),
		};
		gpu.plan_rows(damage, self.ch);
		let next = gpu.frame(size, |pen| self.quads(pen));
		if remembers(gpu.keeps_rows(), &next) {
			self.canvas.remember(&mut self.frame, key);
		} else {
			// Software would redraw in part pixels that never held the grid.
			self.canvas.invalidate();
		}
		drawn_as(&mut self.drawing, &next);
		let again = || self.request_redraw();
		after_drawn(gpu, next, again, Renderer::Gpu, Renderer::after_gpu)
	}

	// pr-check: window glue: draws the real window's frame for the GPU to show
	fn quads(&mut self, pen: &mut QuadPen) {
		let (painting, rows) = (Instant::now(), pen.rows_to_draw().map(<[bool]>::to_vec));
		self.compose(pen, rows.as_deref());
		self.painted(painting.elapsed());
	}
	// pr-check: end window glue
}
