//! Windows drawn by the GPU for tests outside this module: on the fake
//! driver of `driver_tests`, opened, made to fail, and what it recorded, or
//! on the Vulkan of a machine to open one with; and headless on the
//! system's Vulkan.
use super::{
	choose::{GRAPHICS, Kind},
	driver_tests::{finish, install, with},
	entry::Entry,
	loader_tests::{entry, required},
	present_driver_tests::OUT_OF_DATE,
	renderer_tests::drawn,
	surface::Window,
	surface_driver_tests::{gpu, present},
	window::GpuWindow,
	window_tests::{close, machine, open, showing},
};

/// A window on a machine with one device that shows in it.
pub(crate) fn fake_window() -> GpuWindow {
	open(showing(), false).unwrap()
}

/// Drops the window, checking it left nothing behind.
pub(crate) fn fake_close(window: GpuWindow) {
	close(window);
}

/// The uploads and draws recorded since the last call.
pub(crate) fn fake_recorded() -> Vec<String> {
	with(|driver| {
		let calls = drawn(driver).into_iter().map(str::to_owned).collect();
		driver.recorded.clear();
		calls
	})
}

/// How many frames were presented.
pub(crate) fn fake_presents() -> usize {
	with(|driver| driver.presents.len())
}

/// Has the next acquisition find the swapchain out of date.
pub(crate) fn fake_out_of_date() {
	with(|driver| driver.acquire_answers = vec![OUT_OF_DATE]);
}

/// Has `function` fail with `code` from now on, or, with none, nothing.
pub(crate) fn fake_failing(function: Option<(&'static std::ffi::CStr, i32)>) {
	with(|driver| driver.fails = function.into_iter().collect());
}

/// The Vulkan of a machine with one device that shows in windows, drawn by
/// the CPU when `cpu`, else an integrated GPU.
pub(crate) fn fake_vulkan(cpu: bool) -> Entry {
	let kind = if cpu { Kind::Cpu } else { Kind::Integrated };
	install(machine(&[gpu(kind, vec![present(GRAPHICS, true)])]))
}

/// Checks the windows opened since the last call left nothing behind.
pub(crate) fn fake_finish() {
	let driver = finish();
	assert!(driver.live.is_empty() && driver.live_surfaces.is_empty());
	assert!(driver.live_devices.is_empty() && driver.live_swapchains.is_empty());
}

/// A window on a headless surface of the system's Vulkan, which may draw on
/// the CPU, as CI has no GPU; `None` where there is no Vulkan or it makes no
/// headless surfaces, and neither is required.
pub(crate) fn headless_window(atlas: (usize, usize)) -> Option<GpuWindow> {
	let entry = entry()?;
	let offered = entry.instance_extensions().expect("extensions");
	if !offered.iter().any(|name| name == "VK_EXT_headless_surface") {
		assert!(!required(), "no headless surfaces: {offered:?}");
		eprintln!("skipped: no headless surfaces");
		return None;
	}
	// SAFETY: a headless surface has no window handles to keep live.
	Some(unsafe { GpuWindow::new(entry, Window::Headless, true, atlas) }.expect("a window"))
}
