//! Tests of opening a window drawn by the GPU against the fake driver of
//! `driver_tests`: the surface made for the window's platform, the queue
//! family that draws and shows, the atlas asked for, and every step that
//! can fail leaving nothing behind.
use super::{
	call::Error,
	choose::{GRAPHICS, Kind, TRANSFER},
	driver_tests::{Driver, OFFERED, finish, install},
	surface::Platform,
	surface_driver_tests::{XLIB, gpu, present},
	window::GpuWindow,
	window_tests::{ATLAS, close, machine, open, showing},
};

/// What `driver` saw opening a window that failed, checking it left no
/// object of any kind alive.
fn failed(driver: Driver) -> (Option<Error>, Driver) {
	let error = open(driver, false).err();
	let driver = finish();
	assert!(driver.live.is_empty() && driver.live_surfaces.is_empty());
	assert!(driver.live_devices.is_empty() && driver.live_buffers.is_empty());
	assert!(driver.live_images.is_empty() && driver.live_views.is_empty());
	assert!(driver.live_memory.is_empty() && driver.live_pools.is_empty());
	assert!(driver.live_semaphores.is_empty());
	(error, driver)
}

#[test]
fn every_step_of_opening_that_fails_is_an_error_and_leaves_nothing() {
	// In the order opening takes them: the devices, each one's
	// description and what shows, the device, its timeline, the atlas and
	// the buffers and commands of the renderer.
	let steps = [
		(c"vkEnumeratePhysicalDevices", -1),
		(c"vkEnumerateDeviceExtensionProperties", -2),
		(c"vkGetPhysicalDeviceSurfaceSupportKHR", -1_000_000_000),
		(c"vkCreateDevice", -3),
		(c"vkCreateSemaphore", -1),
		(c"vkCreateCommandPool", -2),
		(c"vkAllocateCommandBuffers", -1),
		(c"vkAllocateDescriptorSets", -1_000_069_000),
		(c"vkCreateImage", -2),
		(c"vkCreateImageView", -1),
		(c"vkCreateBuffer", -1),
		(c"vkAllocateMemory", -2),
		(c"vkMapMemory", -5),
	];
	for (name, code) in steps {
		let mut driver = showing();
		driver.fails = vec![(name, code)];
		assert_eq!(failed(driver).0, Some(Error::Failed(code)), "{name:?}");
	}
}

#[test]
fn a_loader_without_surfaces_for_the_platform_makes_no_instance() {
	let mut driver = showing();
	driver.instance_extensions = Some(
		(OFFERED.into_iter())
			.filter(|name| *name != "VK_KHR_wayland_surface")
			.collect(),
	);
	let (error, driver) = failed(driver);
	assert_eq!(error, Some(Error::Missing(c"VK_KHR_wayland_surface")));
	assert!(driver.created.is_empty());
}

#[test]
fn a_driver_that_cannot_make_the_surface_leaves_no_instance() {
	let mut driver = showing();
	driver.lacks = vec![c"vkCreateWaylandSurfaceKHR"];
	let (error, driver) = failed(driver);
	assert_eq!(error, Some(Error::Missing(c"vkCreateWaylandSurfaceKHR")));
	assert_eq!(driver.created.len(), 1);
	assert!(driver.surfaces.is_empty());
}

#[test]
fn an_xlib_window_gets_an_xlib_surface_of_its_handles() {
	// SAFETY: the fake driver only records the window's handles.
	let window = unsafe { GpuWindow::new(install(showing()), XLIB, false, ATLAS) }.unwrap();
	let driver = close(window);
	let surface = &driver.surfaces[0];
	assert_eq!(surface.platform, Platform::Xlib);
	assert_eq!(surface.window, 0x0420_0007);
}

#[test]
fn the_device_is_made_on_the_family_that_draws_and_shows() {
	let families = vec![
		present(GRAPHICS, false),
		present(TRANSFER, true),
		present(GRAPHICS | TRANSFER, true),
	];
	let window = open(machine(&[gpu(Kind::Discrete, families)]), false).unwrap();
	let driver = close(window);
	assert_eq!(driver.made.len(), 1);
	assert_eq!(driver.made[0].family, 2);
}

#[test]
fn the_atlas_has_the_pages_asked_for() {
	let window = open(showing(), false).unwrap();
	let driver = close(window);
	// Sampled and copied into: the atlas, not a swapchain image.
	let atlas: Vec<_> = (driver.images.iter())
		.filter(|image| image.usage == 0x4 | 0x2)
		.map(|image| (image.extent, image.layers))
		.collect();
	assert_eq!(atlas, [([64, 64, 1], 2)]);
}
