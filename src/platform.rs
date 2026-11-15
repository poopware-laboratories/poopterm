//! Platform-specific window tweaks.
// Calls into C or the kernel; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
#[cfg(target_os = "macos")]
use winit::window::Window;
use winit::window::WindowAttributes;

/// The app id `--app-id` chose, set once at startup.
static APP_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Sets the app id [`identify`] gives windows; only the first call counts.
pub(crate) fn set_app_id(id: String) {
	let _ = APP_ID.set(id);
}

/// The app id windows get: the one set, or `poopterm`.
#[cfg(any(
	target_os = "linux",
	target_os = "dragonfly",
	target_os = "freebsd",
	target_os = "netbsd",
	target_os = "openbsd"
))]
fn app_id() -> &'static str {
	APP_ID.get().map_or("poopterm", String::as_str)
}

/// Tags windows with Wayland app_id and X11 WM_CLASS `poopterm`, or the
/// `--app-id` given. Desktops match `poopterm` to `poopterm.desktop` for
/// the window's icon, name, and dock pinning.
#[cfg(any(
	target_os = "linux",
	target_os = "dragonfly",
	target_os = "freebsd",
	target_os = "netbsd",
	target_os = "openbsd"
))]
pub(crate) fn identify(attributes: WindowAttributes) -> WindowAttributes {
	use winit::platform::{wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11};
	let id = app_id();
	let attributes = WindowAttributesExtWayland::with_name(attributes, id, id);
	WindowAttributesExtX11::with_name(attributes, id, id)
}

/// Other platforms identify windows by their app bundle.
#[cfg(not(any(
	target_os = "linux",
	target_os = "dragonfly",
	target_os = "freebsd",
	target_os = "netbsd",
	target_os = "openbsd"
)))]
pub(crate) fn identify(attributes: WindowAttributes) -> WindowAttributes {
	attributes
}

#[cfg(target_os = "macos")]
pub(crate) fn set_window_opacity(window: &Window, opacity: usize) {
	use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
	let Ok(handle) = window.window_handle() else {
		return;
	};
	let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
		return;
	};
	let view = handle.ns_view.as_ptr().cast::<objc2::runtime::AnyObject>();
	// SAFETY: `view` is the live NSView winit gave for this window, and
	// `window` takes nothing and returns its NSWindow or nil.
	let native_window: *mut objc2::runtime::AnyObject = unsafe { objc2::msg_send![view, window] };
	if !native_window.is_null() {
		// SAFETY: a live NSWindow, checked above; `setAlphaValue:` takes a
		// CGFloat and returns nothing.
		let _: () = unsafe { objc2::msg_send![native_window, setAlphaValue: opacity as f64 / 100.0] };
	}
}

/// The mouse buttons the system says are held right now (bit 0 is the left
/// button), or `None` where the window system always delivers releases.
#[cfg(target_os = "macos")]
pub(crate) fn held_mouse_buttons() -> Option<usize> {
	// SAFETY: `pressedMouseButtons` is a class method of `NSEvent` that takes
	// no arguments and returns an `NSUInteger`; it reads state and is safe to
	// call from the main thread, where the pointer events are handled.
	Some(unsafe { objc2::msg_send![objc2::class!(NSEvent), pressedMouseButtons] })
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn held_mouse_buttons() -> Option<usize> {
	None
}

/// This process's real user ID, which names its runtime files and finds
/// its account.
pub(crate) use crate::sys::user::uid;

#[cfg(all(test, target_os = "linux"))]
mod tests {
	use super::{app_id, set_app_id};

	// The only test that sets the app id, so the order below holds.
	#[test]
	fn windows_take_the_first_app_id_set_or_poopterm() {
		assert_eq!(app_id(), "poopterm", "nothing set yet");
		set_app_id("dropdown".into());
		assert_eq!(app_id(), "dropdown");
		set_app_id("later".into());
		assert_eq!(app_id(), "dropdown", "only the first call counts");
	}
}
