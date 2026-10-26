//! Tests of opening the GPU for the main window against the fake Vulkan
//! driver: the window's handles and Vulkan's loader checked in turn, and a
//! device drawn by the CPU used only when the GPU is asked for by name.
use super::Gpu;
use crate::{
	config::RendererPreference::{self, Auto, Gpu as Named},
	gpu::vk::{
		call::Error,
		entry::Entry,
		outside_tests::{fake_finish, fake_vulkan},
	},
};
use std::{cell::Cell, ffi::c_void, num::NonZeroU32, ptr::NonNull};
use winit::raw_window_handle::{
	RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, XcbDisplayHandle,
	XcbWindowHandle,
};

fn wayland() -> Option<(RawWindowHandle, RawDisplayHandle)> {
	let pointer = |address| NonNull::new(address as *mut c_void).unwrap();
	let window = WaylandWindowHandle::new(pointer(0x5000_usize));
	let display = WaylandDisplayHandle::new(pointer(0xd000_usize));
	Some((window.into(), display.into()))
}

/// Opens the GPU for a window with `handles` on the Vulkan `load` gives,
/// saying whether it was loaded.
fn open(
	handles: Option<(RawWindowHandle, RawDisplayHandle)>,
	load: impl FnOnce() -> Result<Entry, Error>,
	preference: RendererPreference,
) -> (Result<Gpu<()>, String>, bool) {
	let loaded = Cell::new(false);
	let load = || {
		loaded.set(true);
		load()
	};
	// SAFETY: the fake driver only records the handles.
	let gpu = unsafe { Gpu::open((), handles, load, preference) };
	(gpu, loaded.get())
}

/// The name of the device `gpu` draws on, after closing it.
fn device(gpu: Result<Gpu<()>, String>) -> Result<String, String> {
	let name = gpu.map(|gpu| gpu.gpu.device_name().to_owned());
	fake_finish();
	name
}

#[test]
fn a_wayland_window_draws_on_the_device_that_shows_in_it() {
	let (gpu, loaded) = open(wayland(), || Ok(fake_vulkan(false)), Auto);
	assert!(loaded);
	assert_eq!(device(gpu), Ok("Integrated".to_owned()));
}

#[test]
fn a_window_without_handles_or_of_another_kind_never_loads_vulkan() {
	let (gpu, loaded) = open(None, || Ok(fake_vulkan(false)), Auto);
	assert_eq!(gpu.err().as_deref(), Some("the window has no handles"));
	assert!(!loaded);
	let window = XcbWindowHandle::new(NonZeroU32::new(7).unwrap());
	let display = XcbDisplayHandle::new(None, 0);
	let xcb = Some((window.into(), display.into()));
	let (gpu, loaded) = open(xcb, || Ok(fake_vulkan(false)), Auto);
	let cannot = "Vulkan cannot draw in this kind of window";
	assert_eq!(gpu.err().as_deref(), Some(cannot));
	assert!(!loaded);
}

#[test]
fn vulkan_failing_to_load_says_why() {
	let missing = Error::Missing(c"vkGetInstanceProcAddr");
	let said = missing.to_string();
	let (gpu, loaded) = open(wayland(), || Err(missing), Named);
	assert_eq!((gpu.err(), loaded), (Some(said), true));
}

#[test]
fn a_device_drawn_by_the_cpu_is_used_only_when_the_gpu_is_named() {
	// Our software renderer is faster: `auto` leaves the window to it.
	let (gpu, _) = open(wayland(), || Ok(fake_vulkan(true)), Auto);
	let refused = device(gpu).unwrap_err();
	assert!(refused.contains("Cpu draws on the CPU"), "{refused}");
	let (gpu, _) = open(wayland(), || Ok(fake_vulkan(true)), Named);
	assert_eq!(device(gpu), Ok("Cpu".to_owned()));
}
