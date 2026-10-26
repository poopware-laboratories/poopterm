//! Tests of the main window's GPU glue: which windows Vulkan draws in,
//! when a CPU device may draw, and what each frame's outcome leads to.
use super::{Next, allows_cpu_device, next, surface_window};
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
