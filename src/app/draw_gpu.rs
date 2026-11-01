//! Drawing a frame of the main window on the GPU, all of it as quads, the
//! wallpaper and images as textures. The pen keeps the grid rows that did
//! not change since the last frame, so only the others are drawn again. A
//! frame the GPU cannot draw (glyphs the atlas cannot hold, images past its
//! budget) is painted whole by software and shown through the GPU.
use super::{
	App,
	canvas::{DrawKey, Plan},
	gpu::{Gpu, after_drawn, remembers},
	renderer::Renderer,
};
use crate::{gpu::quads::QuadPen, render::painter::Painter};
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
		let mut pixels = std::mem::take(&mut self.canvas.pixels);
		let pixels_of = (size, &mut pixels[..]);
		let next = gpu.frame(self, pixels_of, Self::quads, Self::whole);
		self.canvas.pixels = pixels;
		if remembers(gpu.keeps_rows(), &next) {
			self.canvas.remember(&mut self.frame, key);
		} else {
			// Software would redraw in part pixels that never held the grid.
			self.canvas.invalidate();
		}
		let again = || self.request_redraw();
		after_drawn(gpu, next, again, Renderer::Gpu, Renderer::after_gpu)
	}

	// pr-check: window glue: draws the real window's frame for the GPU to show
	fn quads(&mut self, pen: &mut QuadPen) {
		let (painting, rows) = (Instant::now(), pen.rows_to_draw().map(<[bool]>::to_vec));
		self.compose(pen, rows.as_deref());
		self.painted(painting.elapsed(), false);
	}
	// pr-check: end window glue

	// pr-check: window glue: paints the real window's frame whole for the GPU to show
	fn whole(&mut self, pixels: &mut [u32]) {
		let (width, height) = (self.width as usize, self.height as usize);
		let painting = Instant::now();
		self.compose(&mut Painter::new(pixels, width, height), None);
		self.painted(painting.elapsed(), true);
	}
	// pr-check: end window glue
}
