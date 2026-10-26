//! Drawing a frame of the main window on the GPU: software paints the
//! wallpaper and chrome, and the GPU draws the grid over them. A frame the
//! GPU cannot draw, with an overlay over the grid or images in it, is
//! painted whole by software and shown through the GPU.
use super::{
	App,
	canvas::DrawKey,
	gpu::{Gpu, Next},
	renderer::Renderer,
};
use crate::{gpu::quads::QuadPen, render::painter::Painter};

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
		let next = gpu.frame(self, pixels_of, key.overlay, Self::around_grid, Self::whole);
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
	fn around_grid(&mut self, pixels: &mut [u32], pen: &mut QuadPen) {
		let (width, height) = (self.width as usize, self.height as usize);
		self.compose_around(&mut Painter::new(pixels, width, height), false);
		self.draw_terminal(pen, self.scale(), None);
	}

	fn whole(&mut self, pixels: &mut [u32]) {
		let (width, height) = (self.width as usize, self.height as usize);
		self.compose(&mut Painter::new(pixels, width, height));
	}
	// pr-check: end window glue
}
