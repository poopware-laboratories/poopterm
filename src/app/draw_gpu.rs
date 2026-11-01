//! Drawing a frame of the main window on the GPU: all of it as quads, or
//! with a wallpaper, which quads cannot show, the grid over the wallpaper
//! and chrome software painted. A frame the GPU cannot draw, with images
//! in it or an overlay over a wallpaper's grid, is painted whole by
//! software and shown through the GPU.
use super::{
	App,
	canvas::DrawKey,
	gpu::{Gpu, Next, layers},
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
		// The pixels never hold the grid the GPU drew: never redrawn in part.
		self.canvas.invalidate();
		self.canvas.plan(&self.frame, key, size, false);
		let mut pixels = std::mem::take(&mut self.canvas.pixels);
		let pixels_of = (size, &mut pixels[..]);
		let layers = layers(self.wallpaper.is_some(), key.overlay);
		let next = gpu.frame(self, pixels_of, layers, Self::around_grid, Self::whole);
		self.canvas.pixels = pixels;
		match next {
			Next::Done | Next::Software => Some(Renderer::Gpu(gpu)),
			Next::Redraw => {
				self.request_redraw();
				Some(Renderer::Gpu(gpu))
			}
			Next::Abandon(error) => {
				self.request_redraw();
				Renderer::after_gpu(gpu, &error)
					.inspect_err(|error| eprintln!("poopterm: software renderer unavailable: {error}"))
					.ok()
			}
		}
	}

	// pr-check: window glue: paints the real window's frame for the GPU to show
	fn around_grid(&mut self, pixels: Option<&mut [u32]>, pen: &mut QuadPen) {
		let (width, height) = (self.width as usize, self.height as usize);
		let painting = Instant::now();
		let Some(pixels) = pixels else {
			self.compose_plain(pen);
			return self.painted(painting.elapsed(), false);
		};
		self.compose_around(&mut Painter::new(pixels, width, height), false);
		self.painted(painting.elapsed(), false);
		self.draw_terminal(pen, self.scale(), None);
	}
	// pr-check: end window glue

	// pr-check: window glue: paints the real window's frame whole for the GPU to show
	fn whole(&mut self, pixels: &mut [u32]) {
		let (width, height) = (self.width as usize, self.height as usize);
		let painting = Instant::now();
		self.compose(&mut Painter::new(pixels, width, height));
		self.painted(painting.elapsed(), true);
	}
	// pr-check: end window glue
}
