//! Window renderer boundary: the GPU where it can draw in the window, else
//! the software backend, whose frames our own presenter shows on Wayland
//! and softbuffer elsewhere.
use super::{Arc, Window, repaint};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use super::{gpu::Gpu, wayland};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use crate::present::wayland::presenter::Presenter;
use crate::{
	config::RendererPreference,
	render::{
		backend::{self, Backend, select},
		damage::GridDamage,
	},
	term::Frame,
};
use std::num::NonZeroU32;

pub(super) enum Renderer {
	Software(Software),
	#[cfg(any(target_os = "linux", target_os = "freebsd"))]
	Gpu(Box<Gpu>),
}

/// What showing a software frame came to.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(
	not(any(target_os = "linux", target_os = "freebsd")),
	allow(dead_code, reason = "only Wayland windows wait or fail")
)]
pub(super) enum Shown {
	Done,
	/// The window system holds every buffer: show the frame again soon.
	/// What changed is kept for then.
	Retry,
	/// Showing failed, as said: show frames through softbuffer from now on.
	Failed(String),
}

/// The sink to show the next frame with, after this one was `shown`:
/// `again` asks for another frame, and `fallback` replaces a sink that
/// failed.
pub(super) fn after_shown<S>(
	sink: S,
	shown: Shown,
	again: impl FnOnce(),
	fallback: impl FnOnce(S, &str) -> Result<S, String>,
) -> Option<S> {
	match shown {
		Shown::Done => Some(sink),
		Shown::Retry => {
			again();
			Some(sink)
		}
		Shown::Failed(error) => {
			again();
			let failed = |error: &String| eprintln!("poopterm: software renderer unavailable: {error}");
			fallback(sink, &error).inspect_err(failed).ok()
		}
	}
}

/// A window's frames drawn by the CPU, shown through our own presenter on
/// Wayland and through softbuffer elsewhere.
pub(super) struct Software {
	/// Dropped before the window it shows frames in.
	sink: Sink,
	window: Arc<Window>,
}

enum Sink {
	Softbuffer {
		_context: softbuffer::Context<Arc<Window>>,
		surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
	},
	#[cfg(any(target_os = "linux", target_os = "freebsd"))]
	Wayland(Box<Presenter>),
}

impl Software {
	// pr-check: window glue: shows frames only in a real window
	pub(super) fn new(window: Arc<Window>) -> Result<Self, String> {
		#[cfg(any(target_os = "linux", target_os = "freebsd"))]
		if let Some(presenter) = wayland::open_for(&window) {
			let sink = Sink::Wayland(Box::new(presenter));
			return Ok(Self { sink, window });
		}
		Self::softbuffer(window)
	}
	// pr-check: end window glue

	// pr-check: window glue: softbuffer shows frames only in a real window
	fn softbuffer(window: Arc<Window>) -> Result<Self, String> {
		let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
		let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
		let sink = Sink::Softbuffer {
			_context: context,
			surface,
		};
		Ok(Self { sink, window })
	}
	// pr-check: end window glue

	/// Softbuffer for the window whose presenter failed with `error`.
	// pr-check: window glue: hands a real window from its failed presenter to softbuffer
	pub(super) fn fallback(self, error: &str) -> Result<Self, String> {
		let window = self.window.clone();
		// Its buffers go before softbuffer attaches its own.
		drop(self);
		eprintln!("poopterm: wayland presenter failed: {error}; using softbuffer");
		let size = window.inner_size();
		let mut software = Self::softbuffer(window)?;
		software.resize(size.width, size.height);
		Ok(software)
	}
	// pr-check: end window glue

	pub(super) fn resize(&mut self, width: u32, height: u32) {
		match &mut self.sink {
			// pr-check: window glue: softbuffer's surface is only a real window's
			Sink::Softbuffer { surface, .. } => {
				let size = (NonZeroU32::new(width), NonZeroU32::new(height));
				if let (Some(width), Some(height)) = size {
					let _ = surface.resize(width, height);
				}
			}
			// pr-check: end window glue
			// The presenter makes buffers at the size of each frame.
			#[cfg(any(target_os = "linux", target_os = "freebsd"))]
			Sink::Wayland(_) => {}
		}
	}

	/// Shows `pixels`, rows `width` wide: all of them, or only `bands` of
	/// pixel rows, reported as the only damage with `damage_only`.
	pub(super) fn present(
		&mut self,
		pixels: &[u32],
		width: usize,
		bands: Option<&[(usize, usize)]>,
		damage_only: bool,
	) -> Shown {
		match &mut self.sink {
			// pr-check: window glue: softbuffer's surface is only a real window's
			Sink::Softbuffer { surface, .. } => {
				if let Ok(buffer) = surface.buffer_mut() {
					repaint::present(buffer, pixels, width, bands, damage_only);
				}
				Shown::Done
			}
			// pr-check: end window glue
			#[cfg(any(target_os = "linux", target_os = "freebsd"))]
			Sink::Wayland(presenter) => wayland::present(presenter, pixels, width, bands),
		}
	}
}

impl Renderer {
	pub(super) fn new(window: Arc<Window>, preference: RendererPreference) -> Result<Self, String> {
		let variable = std::env::var(backend::VARIABLE).ok();
		let preference = backend::preference(preference, variable.as_deref());
		let selected = select(preference, |backend| match backend {
			Backend::Gpu => Self::gpu(&window, preference),
			Backend::Software => Software::new(window.clone()).map(Self::Software),
		})
		.map_err(|failures| format_failures(&failures))?;
		if !selected.failures.is_empty() {
			eprintln!(
				"poopterm: {}; using {} renderer",
				format_failures(&selected.failures),
				selected.backend.name()
			);
		}
		Ok(selected.value)
	}

	// pr-check: window glue: the GPU draws in a real window's surface
	#[cfg(any(target_os = "linux", target_os = "freebsd"))]
	fn gpu(window: &Arc<Window>, preference: RendererPreference) -> Result<Self, String> {
		Gpu::new(window, preference).map(|gpu| Self::Gpu(Box::new(gpu)))
	}

	#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
	fn gpu(_: &Arc<Window>, _: RendererPreference) -> Result<Self, String> {
		Err("not available on this system yet".to_owned())
	}
	// pr-check: end window glue

	/// The software renderer, for a window whose GPU failed with `error`.
	// pr-check: window glue: hands a real window from its failed GPU to softbuffer
	#[cfg(any(target_os = "linux", target_os = "freebsd"))]
	pub(super) fn after_gpu(gpu: Box<Gpu>, error: &str) -> Result<Self, String> {
		let window = gpu.window().clone();
		// Its swapchain goes before software presents in the window.
		drop(gpu);
		eprintln!("poopterm: gpu renderer failed: {error}; using software renderer");
		let size = window.inner_size();
		let mut renderer = Software::new(window).map(Self::Software)?;
		renderer.resize(size.width, size.height);
		Ok(renderer)
	}
	// pr-check: end window glue

	/// Softbuffer in place of a presenter that failed with `error`.
	// pr-check: window glue: hands a real window's software renderer to softbuffer
	pub(super) fn after_presenter(self, error: &str) -> Result<Self, String> {
		match self {
			Self::Software(software) => software.fallback(error).map(Self::Software),
			#[cfg(any(target_os = "linux", target_os = "freebsd"))]
			gpu @ Self::Gpu(_) => Ok(gpu),
		}
	}
	// pr-check: end window glue

	pub(super) fn resize(&mut self, width: u32, height: u32) {
		match self {
			Self::Software(software) => software.resize(width, height),
			// The GPU makes its swapchain at the size of each frame.
			#[cfg(any(target_os = "linux", target_os = "freebsd"))]
			Self::Gpu(_) => {}
		}
	}

	pub(super) fn present(
		&mut self,
		_frame: &Frame,
		_damage: Option<&GridDamage>,
		pixels: &[u32],
		width: usize,
		bands: Option<&[(usize, usize)]>,
		damage_only: bool,
	) -> Shown {
		match self {
			Self::Software(software) => software.present(pixels, width, bands, damage_only),
			// The GPU shows its frames as it draws them.
			#[cfg(any(target_os = "linux", target_os = "freebsd"))]
			Self::Gpu(_) => Shown::Done,
		}
	}
}

fn format_failures(failures: &[(Backend, String)]) -> String {
	failures
		.iter()
		.map(|(backend, error)| format!("{} renderer unavailable: {error}", backend.name()))
		.collect::<Vec<_>>()
		.join("; ")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fallback_diagnostic_names_the_backend_and_reason() {
		let failures = [(Backend::Gpu, "Metal device creation failed".to_owned())];
		assert_eq!(
			format_failures(&failures),
			"gpu renderer unavailable: Metal device creation failed"
		);
	}

	#[test]
	fn startup_diagnostic_keeps_multiple_failures_in_attempt_order() {
		let failures = [
			(Backend::Gpu, "no device".to_owned()),
			(Backend::Software, "no window surface".to_owned()),
		];
		assert_eq!(
			format_failures(&failures),
			"gpu renderer unavailable: no device; software renderer unavailable: no window surface"
		);
	}

	/// What `after_shown` did: the sink it returned, and whether it asked
	/// for another frame and fell back, with which error.
	fn follow(shown: Shown, replacement: Result<u8, String>) -> (Option<u8>, bool, Option<String>) {
		let mut asked = false;
		let mut failed = None;
		let fallback = |sink: u8, error: &str| {
			assert_eq!(sink, 1, "the failed sink is handed over");
			failed = Some(error.to_owned());
			replacement
		};
		let next = after_shown(1, shown, || asked = true, fallback);
		(next, asked, failed)
	}

	#[test]
	fn a_frame_shown_keeps_its_sink_and_asks_for_nothing() {
		assert_eq!(follow(Shown::Done, Ok(2)), (Some(1), false, None));
	}

	#[test]
	fn a_frame_waiting_for_a_buffer_keeps_its_sink_and_asks_for_another() {
		assert_eq!(follow(Shown::Retry, Ok(2)), (Some(1), true, None));
	}

	#[test]
	fn a_sink_that_failed_is_replaced_and_the_frame_shown_again() {
		let shown = Shown::Failed("the connection to the compositor broke".to_owned());
		let (next, asked, failed) = follow(shown, Ok(2));
		assert_eq!((next, asked), (Some(2), true));
		assert_eq!(
			failed.as_deref(),
			Some("the connection to the compositor broke")
		);
	}

	#[test]
	fn a_replacement_that_fails_leaves_the_window_without_a_sink() {
		let shown = Shown::Failed("lost".to_owned());
		let (next, asked, failed) = follow(shown, Err("no softbuffer".to_owned()));
		assert_eq!((next, asked, failed.as_deref()), (None, true, Some("lost")));
	}

	#[test]
	fn no_failures_produce_no_diagnostic_text() {
		assert_eq!(format_failures(&[]), "");
	}
}
