//! The main window drawn on the GPU: Vulkan's view of the window from its
//! handles, the grid renderer and its pen, and what each frame's outcome
//! means for the next.
use super::{Arc, Window};
use crate::{
	config::RendererPreference,
	gpu::{
		quads::QuadPen,
		vk::{
			call::Error,
			entry::Entry,
			renderer::Backdrop,
			surface,
			window::{Drawn, GpuWindow},
		},
	},
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
	/// Paint the frame in software, as the GPU cannot draw it.
	Software,
	/// The GPU failed, as said: draw with software from now on.
	Abandon(String),
}

pub(super) fn next(drawn: Result<Drawn, Error>) -> Next {
	match drawn {
		Ok(Drawn::Shown | Drawn::Skipped) => Next::Done,
		Ok(Drawn::Again) => Next::Redraw,
		Err(Error::Draw) => Next::Software,
		Err(error) => Next::Abandon(error.to_string()),
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
		eprintln!("poopterm: drawing on {}", gpu.gpu.device_name());
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
			.map(|gpu| Self::with(gpu, window))
			.map_err(|error| error.to_string())
	}

	fn with(gpu: GpuWindow, window: W) -> Self {
		Self {
			gpu,
			pen: QuadPen::new((0, 0), ATLAS),
			window,
		}
	}

	/// The window it draws in.
	pub(super) fn window(&self) -> &W {
		&self.window
	}

	/// Draws a frame `size` pixels large and shows it, painted into
	/// `pixels` by software: by `around` all but the grid, which it draws
	/// with the pen for the GPU to draw over the rest, unless an `overlay`
	/// covers the grid or the GPU cannot draw it; then by `whole`.
	pub(super) fn frame<C>(
		&mut self,
		context: &mut C,
		(size, pixels): ((usize, usize), &mut [u32]),
		overlay: bool,
		around: impl FnOnce(&mut C, &mut [u32], &mut QuadPen),
		whole: impl FnOnce(&mut C, &mut [u32]),
	) -> Next {
		let mut next = Next::Software;
		if !overlay {
			self.pen.begin_frame(size);
			around(context, pixels, &mut self.pen);
			next = self.draw(size, pixels);
		}
		// A refused frame started the pen over.
		if next == Next::Software {
			whole(context, pixels);
			next = self.draw(size, pixels);
		}
		next
	}

	/// Draws the pen's frame over `backdrop`, the window's `size` pixels.
	fn draw(&mut self, (width, height): (usize, usize), backdrop: &[u32]) -> Next {
		let backdrop = Backdrop {
			pixels: backdrop,
			width,
		};
		let size = [width, height].map(|n| u32::try_from(n).unwrap_or(u32::MAX));
		next(self.gpu.draw(&mut self.pen, size, Some(backdrop)))
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
#[path = "gpu_tests.rs"]
mod tests;
