//! Tests of the main window's GPU glue: which windows Vulkan draws in,
//! when a CPU device may draw, what a frame is drawn as, what each
//! frame's outcome leads to, which frames the next redraws rows of, and
//! which renderer draws the next frame.
use super::{Next, after_drawn, allows_cpu_device, next, remembers, surface_window};
use crate::{
	config::RendererPreference,
	gpu::vk::{call::Error, surface::Window, swapchain_plan::Unfit, window::Drawn},
};
use std::{ffi::c_void, num::NonZeroU32, ptr::NonNull};
use winit::raw_window_handle::{
	RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, XcbDisplayHandle,
	XcbWindowHandle, XlibDisplayHandle, XlibWindowHandle,
};

fn pointer(address: usize) -> NonNull<c_void> {
	NonNull::new(address as *mut c_void).unwrap()
}

fn wayland() -> (RawWindowHandle, RawDisplayHandle) {
	let window = WaylandWindowHandle::new(pointer(0x5000));
	let display = WaylandDisplayHandle::new(pointer(0xd000));
	(window.into(), display.into())
}

fn xlib(display: Option<NonNull<c_void>>) -> (RawWindowHandle, RawDisplayHandle) {
	let window = XlibWindowHandle::new(0x4a0_0007);
	(window.into(), XlibDisplayHandle::new(display, 0).into())
}

#[test]
fn a_cpu_vulkan_device_is_used_only_when_the_gpu_is_asked_for_by_name() {
	// Our software renderer is faster than Vulkan drawn by the CPU, so
	// `auto` falls back to it instead.
	assert!(allows_cpu_device(RendererPreference::Gpu));
	assert!(!allows_cpu_device(RendererPreference::Auto));
	assert!(!allows_cpu_device(RendererPreference::Software));
}

#[test]
fn a_wayland_window_is_drawn_in_through_its_display_and_surface() {
	let (window, display) = wayland();
	let Some(Window::Wayland { display, surface }) = surface_window(window, display) else {
		panic!("a Wayland window");
	};
	assert_eq!((display, surface), (pointer(0xd000), pointer(0x5000)));
}

#[test]
fn an_xlib_window_is_drawn_in_through_its_display_and_number() {
	let (window, display) = xlib(Some(pointer(0xd000)));
	let Some(Window::Xlib { display, window }) = surface_window(window, display) else {
		panic!("an Xlib window");
	};
	assert_eq!((display, window), (pointer(0xd000), 0x4a0_0007));
}

#[test]
fn windows_vulkan_cannot_draw_in_are_left_to_software() {
	// Xlib's default display is not one Vulkan can name.
	let (window, display) = xlib(None);
	assert!(surface_window(window, display).is_none());
	// Nor a window and display of different systems.
	let (wayland, _) = wayland();
	let (_, xlib) = xlib(Some(pointer(0xd000)));
	assert!(surface_window(wayland, xlib).is_none());
	// Nor XCB, for which we make no surface.
	let xcb_window = XcbWindowHandle::new(NonZeroU32::new(7).unwrap());
	let xcb_display = XcbDisplayHandle::new(Some(pointer(0xc000)), 0);
	assert!(surface_window(xcb_window.into(), xcb_display.into()).is_none());
}

#[test]
fn a_frame_shown_or_with_nothing_to_show_is_done() {
	assert_eq!(next(Ok(Drawn::Shown)), Next::Done);
	// A minimized window draws again once it has area, not before.
	assert_eq!(next(Ok(Drawn::Skipped)), Next::Done);
}

#[test]
fn a_frame_the_swapchain_could_not_take_is_drawn_again() {
	assert_eq!(next(Ok(Drawn::Again)), Next::Redraw);
}

#[test]
fn a_frame_the_gpu_cannot_draw_is_painted_by_software() {
	assert_eq!(next(Err(Error::Draw)), Next::Software);
}

#[test]
fn any_other_failure_abandons_the_gpu_and_says_why() {
	for error in [
		Error::Failed(-4),
		Error::Unfit(Unfit::Format),
		Error::NoMemory,
	] {
		let said = error.to_string();
		assert_eq!(next(Err(error)), Next::Abandon(said));
	}
}

#[test]
fn a_frame_whose_grid_rows_the_pen_keeps_is_remembered_however_it_was_shown() {
	for next in [Next::Done, Next::Redraw, Next::Software] {
		assert!(remembers(true, &next), "{next:?}");
		assert!(!remembers(false, &next), "{next:?}");
	}
}

#[test]
fn a_frame_after_the_gpu_failed_is_not_remembered() {
	// Software never painted the grid the GPU drew.
	assert!(!remembers(true, &Next::Abandon("lost".into())));
}

/// What `after_drawn` did with a GPU after a frame came to `next`, when
/// software would come to `software`: the renderer it hands back, and what
/// it asked for on the way, in order.
fn after(next: Next, software: Result<&str, &str>) -> (Option<String>, Vec<String>) {
	let calls = std::cell::RefCell::new(Vec::new());
	let renderer = after_drawn(
		"gpu",
		next,
		|| calls.borrow_mut().push("again".to_owned()),
		|gpu| format!("kept {gpu}"),
		|gpu, error| {
			calls.borrow_mut().push(format!("{gpu} failed: {error}"));
			software.map(str::to_owned).map_err(str::to_owned)
		},
	);
	(renderer, calls.into_inner())
}

#[test]
fn the_gpu_draws_on_after_a_frame_it_showed_or_left_to_software() {
	// A frame software painted was shown through the GPU, which stays.
	for next in [Next::Done, Next::Software] {
		let (renderer, calls) = after(next, Ok("software"));
		assert_eq!(renderer.as_deref(), Some("kept gpu"));
		assert!(calls.is_empty(), "{calls:?}");
	}
}

#[test]
fn the_gpu_draws_on_and_asks_for_another_frame_after_one_it_could_not_show() {
	let (renderer, calls) = after(Next::Redraw, Ok("software"));
	assert_eq!(renderer.as_deref(), Some("kept gpu"));
	assert_eq!(calls, ["again"]);
}

#[test]
fn a_gpu_that_failed_hands_its_window_to_software_which_draws_the_frame_again() {
	let (renderer, calls) = after(Next::Abandon("device lost".into()), Ok("software"));
	assert_eq!(renderer.as_deref(), Some("software"));
	assert_eq!(calls, ["again", "gpu failed: device lost"]);
}

#[test]
fn a_window_whose_gpu_and_software_both_failed_has_no_renderer() {
	let (renderer, calls) = after(Next::Abandon("device lost".into()), Err("no shm"));
	assert_eq!(renderer, None);
	assert_eq!(calls, ["again", "gpu failed: device lost"]);
}
