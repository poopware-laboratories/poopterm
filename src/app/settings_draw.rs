//! Showing the Settings window's frames: its scene drawn on the GPU when the
//! window has one, painted by software otherwise.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use super::gpu::{Gpu, after_drawn};
use super::{
	App,
	renderer::{Renderer, Software, after_shown},
};
use crate::render::{painter::Painter, scene::Scene};

impl App {
	/// Shows `scene` with `renderer`: the renderer for the next frame.
	pub(super) fn show_settings(&mut self, renderer: Renderer, scene: &Scene) -> Option<Renderer> {
		match renderer {
			#[cfg(any(target_os = "linux", target_os = "freebsd"))]
			Renderer::Gpu(gpu) => self.settings_on_gpu(gpu, scene),
			Renderer::Software(surface) => self.settings_in_software(surface, scene),
		}
	}

	// pr-check: window glue: draws the real settings window's scene for its GPU to show
	#[cfg(any(target_os = "linux", target_os = "freebsd"))]
	fn settings_on_gpu(&mut self, mut gpu: Box<Gpu>, scene: &Scene) -> Option<Renderer> {
		let next = gpu.scene(scene, &mut self.ui_fonts, &mut Vec::new());
		let again = || self.redraw_settings();
		after_drawn(gpu, next, again, Renderer::Gpu, Renderer::after_gpu)
	}
	// pr-check: end window glue

	// pr-check: window glue: paints the real settings window's scene and shows it
	fn settings_in_software(&mut self, mut surface: Software, scene: &Scene) -> Option<Renderer> {
		let (width, height) = (scene.width, scene.height);
		let mut pixels = vec![0; width * height];
		scene.replay(
			&mut Painter::new(&mut pixels, width, height),
			&mut self.ui_fonts,
		);
		let shown = surface.present(&pixels, width, None, false);
		let again = || self.redraw_settings();
		after_shown(surface, shown, again, Software::fallback).map(Renderer::Software)
	}
	// pr-check: end window glue
}
