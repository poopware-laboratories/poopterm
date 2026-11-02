//! Windows drawn on the GPU: Vulkan's view of a window from its handles,
//! the renderer and its pen, frames of the main window and scenes of the
//! Settings window, and what each frame's outcome means for the next.
use super::{Arc, Window};
use crate::{
	config::RendererPreference,
	gpu::{
		quads::QuadPen,
		vk::{
			call::Error,
			entry::Entry,
			surface,
			window::{Drawn, GpuWindow},
		},
	},
	render::{damage::GridDamage, glyphs::FontSet, scene::Scene},
};
use winit::raw_window_handle::{
	HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

/// The glyph atlas: four pages 1024 pixels square, 16 MiB of the GPU's.
const ATLAS: (usize, usize) = (1024, 4);

/// Whether a Vulkan device drawn by the CPU may be used: only when the GPU
/// is asked for by name, as our software renderer is faster.
pub(super) const fn allows_cpu_device(preference: RendererPreference) -> bool {
	matches!(preference, RendererPreference::Gpu)
}

/// Vulkan's view of the window with these handles: Wayland and Xlib
/// windows only.
pub(super) fn surface_window(
	window: RawWindowHandle,
	display: RawDisplayHandle,
) -> Option<surface::Window> {
	match (window, display) {
		(RawWindowHandle::Wayland(window), RawDisplayHandle::Wayland(display)) => {
			Some(surface::Window::Wayland {
				display: display.display,
				surface: window.surface,
			})
		}
		(RawWindowHandle::Xlib(window), RawDisplayHandle::Xlib(display)) => {
			Some(surface::Window::Xlib {
				display: display.display?,
				window: window.window,
			})
		}
		_ => None,
	}
}

/// What to do after drawing a frame on the GPU.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Next {
	Done,
	/// Draw the frame again now.
	Redraw,
	/// The GPU failed, as said: draw with software from now on.
	Abandon(String),
}

pub(super) fn next(drawn: Result<Drawn, Error>) -> Next {
	match drawn {
		Ok(Drawn::Shown | Drawn::Skipped) => Next::Done,
		Ok(Drawn::Again) => Next::Redraw,
		Err(error) => Next::Abandon(error.to_string()),
	}
}

/// Whether a frame that came to `next` is remembered, for the next to
/// redraw only the rows that changed: only while the pen `keeps` its grid
/// rows, which a pen started over for a frame software painted lacks, and
/// while the GPU still draws.
pub(super) fn remembers(keeps: bool, next: &Next) -> bool {
	keeps && !matches!(next, Next::Abandon(_))
}

/// The renderer to draw a window's next frame with, after its `gpu` drew one
/// that came to `next`: the GPU, which `keep` makes a renderer of; another
/// frame asked for by `again` when one is due; and `fallback`'s software
/// in place of a GPU that failed, if that works.
pub(super) fn after_drawn<G, R>(
	gpu: G,
	next: Next,
	again: impl FnOnce(),
	keep: impl FnOnce(G) -> R,
	fallback: impl FnOnce(G, &str) -> Result<R, String>,
) -> Option<R> {
	match next {
		Next::Done => Some(keep(gpu)),
		Next::Redraw => {
			again();
			Some(keep(gpu))
		}
		Next::Abandon(error) => {
			again();
			let failed =
				|error: &String| crate::warnings::warning!("software renderer unavailable: {error}");
			fallback(gpu, &error).inspect_err(failed).ok()
		}
	}
}

pub(super) struct Gpu<W = Arc<Window>> {
	/// Dropped before the window it draws in.
	gpu: GpuWindow,
	pen: QuadPen,
	window: W,
}

impl Gpu {
	// pr-check: window glue: only a real window has handles, and Vulkan's loader
	pub(super) fn new(window: &Arc<Window>, preference: RendererPreference) -> Result<Self, String> {
		let handle = window.window_handle().map(|handle| handle.as_raw());
		let display = window.display_handle().map(|handle| handle.as_raw());
		let handles = handle.ok().zip(display.ok());
		// SAFETY: the handles are the window's, which the clone keeps alive.
		let gpu = unsafe { Self::open(window.clone(), handles, Entry::load, preference) }?;
		crate::warnings::warning!("drawing on {}", gpu.gpu.device_name());
		Ok(gpu)
	}
	// pr-check: end window glue
}

impl<W> Gpu<W> {
	/// The GPU drawing in `window`, which has these `handles`, through the
	/// Vulkan that `load` loads: on a device drawn by the CPU only when
	/// the `preference` names the GPU.
	///
	/// # Safety
	///
	/// The handles stay valid while `window` lives.
	pub(super) unsafe fn open(
		window: W,
		handles: Option<(RawWindowHandle, RawDisplayHandle)>,
		load: impl FnOnce() -> Result<Entry, Error>,
		preference: RendererPreference,
	) -> Result<Self, String> {
		let (handle, display) = handles.ok_or("the window has no handles")?;
		let surface =
			surface_window(handle, display).ok_or("Vulkan cannot draw in this kind of window")?;
		let entry = load().map_err(|error| error.to_string())?;
		let software = allows_cpu_device(preference);
		// SAFETY: as the caller vouches; the GPU drops before `window`.
		let gpu = unsafe { GpuWindow::new(entry, surface, software, ATLAS) };
		gpu
			.map(|gpu| Self::with(gpu, window, ATLAS))
			.map_err(|error| error.to_string())
	}

	/// Draws in `window` on `gpu`, whose atlas is `atlas` (side, pages).
	fn with(gpu: GpuWindow, window: W, atlas: (usize, usize)) -> Self {
		Self {
			gpu,
			pen: QuadPen::new((0, 0), atlas),
			window,
		}
	}

	/// The window it draws in.
	pub(super) fn window(&self) -> &W {
		&self.window
	}

	/// Whether the pen holds the last frame's grid rows.
	pub(super) fn keeps_rows(&self) -> bool {
		self.pen.keeps_rows()
	}

	/// Has the next frame redraw only `damage`'s grid rows, `cell_height`
	/// pixels tall; every row without.
	pub(super) fn plan_rows(&mut self, damage: Option<GridDamage>, cell_height: usize) {
		self.pen.plan_rows(damage, cell_height);
	}

	/// Draws a frame `size` pixels large, which `quads` draws with the pen,
	/// and shows it.
	pub(super) fn frame(&mut self, size: (usize, usize), quads: impl FnOnce(&mut QuadPen)) -> Next {
		self.pen.begin_frame(size);
		quads(&mut self.pen);
		let size = [size.0, size.1].map(|n| u32::try_from(n).unwrap_or(u32::MAX));
		next(self.gpu.draw(&mut self.pen, size))
	}

	/// Draws `scene`, the window's size, with glyphs from `fonts`, and
	/// shows it.
	pub(super) fn scene(&mut self, scene: &Scene, fonts: &mut FontSet) -> Next {
		self.frame((scene.width, scene.height), |pen| scene.replay(pen, fonts))
	}
}

#[cfg(test)]
#[path = "gpu_frame_tests.rs"]
mod frame_tests;
#[cfg(test)]
#[path = "gpu_headless_tests.rs"]
mod headless_tests;
#[cfg(test)]
#[path = "gpu_open_tests.rs"]
mod open_tests;
#[cfg(test)]
#[path = "gpu_scene_tests.rs"]
mod scene_tests;
#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;
