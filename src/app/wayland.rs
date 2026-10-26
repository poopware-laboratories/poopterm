//! The windows' software frames on Wayland, shown through our own
//! presenter rather than softbuffer: which windows it shows frames in,
//! opening it, and what each frame shown leads to.
use super::{Window, renderer::Shown};
use crate::present::wayland::{
	client::{Client, Proxy},
	presenter::{Error, Presented, Presenter},
};
use winit::raw_window_handle::{
	HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

/// The `wl_display` and `wl_surface` of a Wayland window.
pub(super) fn handles(
	window: RawWindowHandle,
	display: RawDisplayHandle,
) -> Option<(Proxy, Proxy)> {
	match (window, display) {
		(RawWindowHandle::Wayland(window), RawDisplayHandle::Wayland(display)) => {
			Some((display.display.as_ptr(), window.surface.as_ptr()))
		}
		_ => None,
	}
}

/// Our presenter for `window`, if it is a Wayland window and the presenter
/// opens.
// pr-check: window glue: only a real window has handles
pub(super) fn open_for(window: &Window) -> Option<Presenter> {
	let raw = window.window_handle().ok()?.as_raw();
	let display = window.display_handle().ok()?.as_raw();
	// SAFETY: a window's display and surface live while it does, and its
	// `Software` keeps it until the presenter is gone; both are used from
	// the UI thread alone.
	unsafe { open(handles(raw, display), Client::load) }
}
// pr-check: end window glue

/// A presenter for the window with `handles`: none for other windows,
/// which are never loaded for, or when it fails to open, which is said.
/// It takes the size of each frame.
///
/// # Safety
///
/// The handles are of a live display and a surface on it, which outlive
/// the presenter and are used from this thread alone.
pub(super) unsafe fn open(
	handles: Option<(Proxy, Proxy)>,
	load: impl FnOnce() -> Result<Client, String>,
) -> Option<Presenter> {
	let (display, surface) = handles?;
	let opened = load().and_then(|client| {
		// SAFETY: as the caller vouches.
		unsafe { Presenter::new(client, display, surface, [0, 0]) }.map_err(describe)
	});
	let failed = |error: &String| eprintln!("poopterm: wayland presenter unavailable: {error}");
	opened.inspect_err(failed).ok()
}

/// Shows `pixels`, rows `width` pixels wide: all of them, or only `bands`
/// of pixel rows. A frame without area shows nothing, until a resize
/// draws again.
pub(super) fn present(
	presenter: &mut Presenter,
	pixels: &[u32],
	width: usize,
	bands: Option<&[(usize, usize)]>,
) -> Shown {
	let height = pixels.len().checked_div(width).unwrap_or(0);
	if height == 0 {
		return Shown::Done;
	}
	// Window sizes come as u32s.
	let size = [width as u32, height as u32];
	match presenter.present(pixels, size, bands) {
		Ok(Presented::Shown) => Shown::Done,
		Ok(Presented::Busy) => Shown::Retry,
		Err(error) => Shown::Failed(describe(error)),
	}
}

fn describe(error: Error) -> String {
	match error {
		Error::NoShm => "the compositor offers no wl_shm with XRGB8888 pixels".to_owned(),
		Error::Lost => "the connection to the compositor broke".to_owned(),
		Error::Memory(error) => format!("no shared memory for {error}"),
	}
}

#[cfg(test)]
#[path = "wayland_tests.rs"]
mod tests;
