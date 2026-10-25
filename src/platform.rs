//! Platform-specific window tweaks.
#[cfg(target_os = "macos")]
use winit::window::Window;
use winit::window::WindowAttributes;

/// Tags windows with Wayland app_id and X11 WM_CLASS `poopterm`. Desktops
/// match it to `poopterm.desktop` for the window's icon, name, and dock
/// pinning.
#[cfg(any(
	target_os = "linux",
	target_os = "dragonfly",
	target_os = "freebsd",
	target_os = "netbsd",
	target_os = "openbsd"
))]
pub(crate) fn identify(attributes: WindowAttributes) -> WindowAttributes {
	use winit::platform::{wayland::WindowAttributesExtWayland, x11::WindowAttributesExtX11};
	const APP_ID: &str = "poopterm";
	let attributes = WindowAttributesExtWayland::with_name(attributes, APP_ID, APP_ID);
	WindowAttributesExtX11::with_name(attributes, APP_ID, APP_ID)
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
	// SAFETY: `view` is the live NSView winit gave for this window; `window`
	// returns its NSWindow or nil, checked before `setAlphaValue:`, and both
	// selectors take and return the types written here.
	unsafe {
		let native_window: *mut objc2::runtime::AnyObject = objc2::msg_send![view, window];
		if !native_window.is_null() {
			let _: () = objc2::msg_send![native_window, setAlphaValue: opacity as f64 / 100.0];
		}
	}
}
