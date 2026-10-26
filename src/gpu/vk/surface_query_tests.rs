//! Tests of asking a surface what it supports and planning its swapchain,
//! against the fake driver of `driver_tests`, whose surface queries live
//! here and answer as Mesa does on Wayland unless a test says otherwise.
use super::{
	call::Error,
	choose::{GRAPHICS, Kind},
	driver_tests::{Driver, answer, finish, install, with, with_devices},
	image::Format,
	instance::Instance,
	surface::{Platform, Surface, Window},
	surface_driver_tests::{WAYLAND, XLIB, gpu, present, surface, window_instance},
	surface_types::RawSurface,
	swapchain_plan::{
		Capabilities, FIFO, MAILBOX, OPAQUE, PRE_MULTIPLIED, Plan, SRGB_NONLINEAR, SurfaceFormat, Unfit,
	},
	types::RawPhysicalDevice,
};
use std::rc::Rc;

const BGRA: SurfaceFormat = SurfaceFormat {
	format: 44,
	color_space: SRGB_NONLINEAR,
};
const BGRA_SRGB: SurfaceFormat = SurfaceFormat {
	format: 50,
	color_space: SRGB_NONLINEAR,
};

/// Mesa's answer on Wayland for an Intel ADL-N.
fn mesa_wayland() -> Capabilities {
	Capabilities {
		min_images: 4,
		max_images: 0,
		current_extent: [u32::MAX; 2],
		min_extent: [1, 1],
		max_extent: [16384, 16384],
		max_layers: 1,
		transforms: 1,
		current_transform: 1,
		alpha: OPAQUE | PRE_MULTIPLIED,
		usage: 0x9f,
	}
}

/// Checks a surface query on `device` and `surface`, recording it; the
/// code it fails with, if it does.
fn query(
	driver: &mut Driver,
	call: &'static str,
	device: RawPhysicalDevice,
	surface: RawSurface,
) -> Option<i32> {
	driver.surface_queries.push(call);
	driver.device(device, call)?;
	if !driver
		.live_surfaces
		.iter()
		.any(|&(_, live)| live == surface)
	{
		driver.invalid.push(format!("{call} on a surface not live"));
	}
	let name = format!("{call}\0");
	let name = std::ffi::CStr::from_bytes_with_nul(name.as_bytes()).unwrap();
	driver.failing(name)
}

pub(super) unsafe extern "system" fn capabilities(
	device: RawPhysicalDevice,
	surface: RawSurface,
	capabilities: *mut Capabilities,
) -> i32 {
	with(|driver| {
		const CALL: &str = "vkGetPhysicalDeviceSurfaceCapabilitiesKHR";
		if let Some(code) = query(driver, CALL, device, surface) {
			return code;
		}
		let answer = driver
			.surface_capabilities
			.clone()
			.unwrap_or_else(mesa_wayland);
		// SAFETY: the bindings pass room for the capabilities.
		unsafe { capabilities.write(answer) };
		0
	})
}

pub(super) unsafe extern "system" fn formats(
	device: RawPhysicalDevice,
	surface: RawSurface,
	count: *mut u32,
	formats: *mut SurfaceFormat,
) -> i32 {
	with(|driver| {
		const CALL: &str = "vkGetPhysicalDeviceSurfaceFormatsKHR";
		if let Some(code) = query(driver, CALL, device, surface) {
			return code;
		}
		let all = driver
			.surface_formats
			.clone()
			.unwrap_or(vec![BGRA_SRGB, BGRA]);
		// SAFETY: passed on as the bindings gave them.
		unsafe { answer(&all, count, formats) }
	})
}

pub(super) unsafe extern "system" fn modes(
	device: RawPhysicalDevice,
	surface: RawSurface,
	count: *mut u32,
	modes: *mut i32,
) -> i32 {
	with(|driver| {
		const CALL: &str = "vkGetPhysicalDeviceSurfacePresentModesKHR";
		if let Some(code) = query(driver, CALL, device, surface) {
			return code;
		}
		let all = driver
			.present_modes
			.clone()
			.unwrap_or(vec![MAILBOX, FIFO, 0]);
		// SAFETY: passed on as the bindings gave them.
		unsafe { answer(&all, count, modes) }
	})
}

/// A driver with one integrated GPU that shows on any surface.
fn showing() -> Driver {
	with_devices(&[gpu(Kind::Integrated, vec![present(GRAPHICS, true)])])
}

/// The plan `driver`'s surface for a window of `window` pixels gets.
fn planned(driver: Driver, platform: Platform, window: [u32; 2]) -> Result<Plan, Error> {
	let instance = window_instance(install(driver), platform);
	let made = surface(
		&instance,
		if platform == Platform::Xlib {
			XLIB
		} else {
			WAYLAND
		},
	);
	let made = made.unwrap();
	let physical = instance.physical_devices().unwrap()[0];
	let planned = made.plan(physical, window);
	drop((made, instance));
	planned
}

#[test]
fn a_mesa_wayland_surface_plans_mailbox_bgra_at_the_windows_size() {
	let made = planned(showing(), Platform::Wayland, [1280, 720]);
	let expected = Plan {
		format: Format::Bgra,
		mode: MAILBOX,
		extent: [1280, 720],
		images: 4,
		transform: 1,
		alpha: OPAQUE,
	};
	assert_eq!(made, Ok(expected));
	let driver = finish();
	let asked = [
		"vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
		"vkGetPhysicalDeviceSurfaceFormatsKHR",
		"vkGetPhysicalDeviceSurfaceFormatsKHR",
		"vkGetPhysicalDeviceSurfacePresentModesKHR",
		"vkGetPhysicalDeviceSurfacePresentModesKHR",
	];
	assert_eq!(driver.surface_queries, asked, "lists are asked count first");
}

#[test]
fn what_the_driver_reports_reaches_the_plan() {
	let mut driver = showing();
	driver.surface_capabilities = Some(Capabilities {
		current_extent: [800, 600],
		min_extent: [800, 600],
		max_extent: [800, 600],
		min_images: 2,
		max_images: 2,
		..mesa_wayland()
	});
	driver.surface_formats = Some(vec![SurfaceFormat {
		format: 37,
		color_space: SRGB_NONLINEAR,
	}]);
	driver.present_modes = Some(vec![FIFO]);
	let made = planned(driver, Platform::Xlib, [1, 1]).unwrap();
	let expected = Plan {
		format: Format::Rgba,
		mode: FIFO,
		extent: [800, 600],
		images: 2,
		transform: 1,
		alpha: OPAQUE,
	};
	assert_eq!(made, expected);
	finish();
}

#[test]
fn surfaces_no_swapchain_suits_are_unfit_errors() {
	let mut driver = showing();
	driver.surface_formats = Some(vec![BGRA_SRGB]);
	let made = planned(driver, Platform::Wayland, [10, 10]);
	assert_eq!(made, Err(Error::Unfit(Unfit::Format)));
	finish();

	let mut driver = showing();
	driver.surface_capabilities = Some(Capabilities {
		current_extent: [0, 0],
		..mesa_wayland()
	});
	let made = planned(driver, Platform::Xlib, [10, 10]);
	assert_eq!(made, Err(Error::Unfit(Unfit::Hidden)));
	finish();
}

#[test]
fn a_failed_query_is_an_error_and_stops_the_plan() {
	let calls = [
		("vkGetPhysicalDeviceSurfaceCapabilitiesKHR", 1),
		("vkGetPhysicalDeviceSurfaceFormatsKHR", 2),
		("vkGetPhysicalDeviceSurfacePresentModesKHR", 4),
	];
	for (call, asked) in calls {
		let mut driver = showing();
		let name = format!("{call}\0");
		let name: &'static std::ffi::CStr =
			std::ffi::CStr::from_bytes_with_nul(name.leak().as_bytes()).unwrap();
		// VK_ERROR_SURFACE_LOST_KHR
		driver.fails = vec![(name, -1_000_000_000)];
		let made = planned(driver, Platform::Wayland, [10, 10]);
		assert_eq!(made, Err(Error::Failed(-1_000_000_000)), "{call}");
		assert_eq!(finish().surface_queries.len(), asked, "{call}");
	}
}

#[test]
fn lists_that_grow_while_read_are_read_whole() {
	// The fake answers VK_INCOMPLETE when given less room than it has.
	let mut driver = showing();
	let many: Vec<_> = (100..140)
		.map(|format| SurfaceFormat {
			format,
			color_space: SRGB_NONLINEAR,
		})
		.chain([BGRA])
		.collect();
	driver.surface_formats = Some(many);
	let made = planned(driver, Platform::Wayland, [10, 10]);
	assert_eq!(made.unwrap().format, Format::Bgra);
	finish();
}

#[test]
fn unfit_surfaces_read_as_why_the_gpu_is_not_used() {
	let cases = [
		(Unfit::Hidden, "the window has no area to draw in"),
		(
			Unfit::Format,
			"the Vulkan surface offers no 8-bit UNORM sRGB format",
		),
		(
			Unfit::Usage,
			"Vulkan swapchain images cannot be drawn to or copied into",
		),
		(
			Unfit::Alpha,
			"the Vulkan surface reports no composite alpha",
		),
	];
	for (unfit, text) in cases {
		assert_eq!(Error::Unfit(unfit).to_string(), text);
	}
}

#[test]
fn the_fake_driver_reports_queries_on_surfaces_not_live() {
	let instance: Rc<Instance> = window_instance(install(showing()), Platform::Headless);
	let made: Surface = surface(&instance, Window::Headless).unwrap();
	let physical = instance.physical_devices().unwrap()[0];
	let mut room = Capabilities::default();
	// SAFETY: the fake writes the capabilities only for a live surface.
	let code = unsafe { capabilities(physical, 0x7777, &mut room) };
	assert_eq!(code, 0);
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	let rule = "vkGetPhysicalDeviceSurfaceCapabilitiesKHR on a surface not live";
	assert_eq!(invalid, [rule]);
	drop((made, instance));
	finish();
}
