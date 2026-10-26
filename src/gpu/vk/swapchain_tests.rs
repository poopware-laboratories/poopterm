//! Tests of swapchains against the fake driver of `driver_tests`: what
//! each is made with, the views of its images, remaking it, what is undone
//! when a step fails, and the order things are destroyed in.
use super::{
	call::Error,
	choose::{GRAPHICS, Kind},
	device::Device,
	driver_tests::{Driver, finish, install, installed, with, with_devices},
	image::Format,
	image_tests::MadeView,
	surface::{Platform, Surface},
	surface_driver_tests::{WAYLAND, XLIB, gpu, present, surface, window_instance},
	swapchain::Swapchain,
	swapchain_driver_tests::{LiveSwapchain, MadeSwapchain},
	swapchain_plan::{
		Capabilities, FIFO, MAILBOX, OPAQUE, PRE_MULTIPLIED, Plan, SRGB_NONLINEAR, SurfaceFormat, USAGE,
	},
	types::SubresourceRange,
};
use std::rc::Rc;

/// A device for windows, and a Wayland surface it shows on, on `driver`.
fn window(driver: Driver) -> (Rc<Device>, Rc<Surface>) {
	let instance = window_instance(install(driver), Platform::Wayland);
	let physical = instance.physical_devices().unwrap()[0];
	let device = Device::for_windows(instance.clone(), physical, 0).expect("device");
	let made = surface(&instance, WAYLAND).expect("surface");
	(Rc::new(device), Rc::new(made))
}

/// A driver with one integrated GPU that shows on any surface.
fn showing() -> Driver {
	with_devices(&[gpu(Kind::Integrated, vec![present(GRAPHICS, true)])])
}

/// What Mesa plans on Wayland for a 1280×720 window.
fn plan() -> Plan {
	Plan {
		format: Format::Bgra,
		mode: MAILBOX,
		extent: [1280, 720],
		images: 4,
		transform: 1,
		alpha: OPAQUE,
	}
}

const BGRA: SurfaceFormat = SurfaceFormat {
	format: 44,
	color_space: SRGB_NONLINEAR,
};

#[test]
fn a_swapchain_is_made_to_its_plan_with_a_view_of_each_image() {
	let (device, surface) = window(showing());
	let swapchain = Swapchain::new(&device, &surface, plan()).expect("swapchain");
	assert_eq!(swapchain.plan(), &plan());
	let images: Vec<_> = (swapchain.images().iter())
		.map(|image| (image.handle(), image.view()))
		.collect();
	let expected: Vec<_> = (1..=4).map(|n| (0xa0_0000 + n, 0xf0_0000 + n)).collect();
	assert_eq!(images, expected);
	let (made, views) = with(|driver| (driver.swapchains.clone(), driver.views.len()));
	let expected = MadeSwapchain {
		surface: 0x5_0000,
		images: 4,
		format: BGRA,
		extent: [1280, 720],
		usage: USAGE,
		transform: 1,
		alpha: OPAQUE,
		mode: MAILBOX,
		clipped: true,
		old: 0,
	};
	assert_eq!((made, views), (vec![expected], 4));
	let view = with(|driver| driver.views.remove(0));
	let range = SubresourceRange {
		aspect_mask: 1,
		base_mip_level: 0,
		level_count: 1,
		base_array_layer: 0,
		layer_count: 1,
	};
	let first = MadeView {
		image: 0xa0_0001,
		view_type: 5,
		format: 44,
		components: [0; 4],
		range,
	};
	assert_eq!(view, first);
	drop((swapchain, device, surface));
	finish();
}

#[test]
fn the_views_go_before_the_swapchain_and_the_swapchain_before_its_surface() {
	let (device, surface) = window(showing());
	let swapchain = Swapchain::new(&device, &surface, plan()).unwrap();
	drop((device, surface));
	assert_eq!(with(|driver| driver.released.clone()), Vec::<String>::new());
	drop(swapchain);
	let driver = finish();
	let views = (1..=4).map(|n| format!("view {:#x}", 0xf0_0000 + n));
	let mut expected: Vec<_> = views.collect();
	expected.extend(["swapchain 0x60001".into(), "surface 0x50000".into()]);
	assert_eq!(
		driver.released, expected,
		"the images are the swapchain's to destroy"
	);
	assert!(driver.live_images.is_empty() && driver.live_devices.is_empty());
}

#[test]
fn a_recreated_swapchain_retires_the_old_one_and_destroys_it_after() {
	let (device, surface) = window(showing());
	let swapchain = Swapchain::new(&device, &surface, plan()).unwrap();
	let bigger = Plan {
		extent: [1920, 1080],
		..plan()
	};
	let swapchain = swapchain.recreate(bigger.clone()).expect("recreated");
	assert_eq!(swapchain.plan(), &bigger);
	assert_eq!(swapchain.images()[0].handle(), 0xa0_0005);
	let driver = with(|driver| (driver.swapchains.clone(), driver.live_swapchains.clone()));
	assert_eq!(driver.0[1].old, 0x6_0001);
	assert_eq!(driver.0[1].extent, [1920, 1080]);
	let live = LiveSwapchain {
		handle: 0x6_0002,
		device: device.handle,
		surface: 0x5_0000,
		retired: false,
		images: (5..=8).map(|n| 0xa0_0000 + n).collect(),
	};
	assert_eq!(driver.1, [live], "the old one is destroyed");
	let released = with(|driver| driver.released.clone());
	assert_eq!(
		released.last().map(String::as_str),
		Some("swapchain 0x60001")
	);
	assert_eq!(released.len(), 5, "its four views first");
	drop((swapchain, device, surface));
	finish();
}

#[test]
fn a_swapchain_needs_a_device_for_windows_on_the_surfaces_instance_that_shows_on_it() {
	let (_, surface) = window(showing());
	let instance = surface.instance.clone();
	let physical = instance.physical_devices().unwrap()[0];
	let offscreen = Rc::new(Device::new(instance, physical, 0).unwrap());
	let made = Swapchain::new(&offscreen, &surface, plan()).err();
	assert_eq!(made, Some(Error::Missing(c"VK_KHR_swapchain")));
	drop(offscreen);
	// A second instance, with its own device.
	let other = window_instance(installed(), Platform::Wayland);
	let physical = other.physical_devices().unwrap()[0];
	let device = Rc::new(Device::for_windows(other, physical, 0).unwrap());
	let made = Swapchain::new(&device, &surface, plan()).err();
	assert_eq!(made, Some(Error::Foreign));
	drop((device, surface));
	assert!(finish().swapchains.is_empty());

	let driver = with_devices(&[gpu(Kind::Integrated, vec![present(GRAPHICS, false)])]);
	let (device, surface) = window(driver);
	let made = Swapchain::new(&device, &surface, plan()).err();
	assert_eq!(made, Some(Error::Unshown));
	drop((device, surface));
	assert!(finish().swapchains.is_empty());
}

#[test]
fn asking_whether_the_device_shows_fails_with_the_drivers_code() {
	let mut driver = showing();
	driver.fails = vec![(c"vkGetPhysicalDeviceSurfaceSupportKHR", -3)];
	let (device, surface) = window(driver);
	let made = Swapchain::new(&device, &surface, plan()).err();
	assert_eq!(made, Some(Error::Failed(-3)));
	drop((device, surface));
	finish();
}

#[test]
fn a_driver_lacking_a_swapchain_function_names_it() {
	for name in [
		c"vkCreateSwapchainKHR",
		c"vkDestroySwapchainKHR",
		c"vkGetSwapchainImagesKHR",
	] {
		let mut driver = showing();
		driver.lacks = vec![name];
		let (device, surface) = window(driver);
		let made = Swapchain::new(&device, &surface, plan()).err();
		assert_eq!(made, Some(Error::Missing(name)));
		drop((device, surface));
		assert!(finish().swapchains.is_empty());
	}
}

#[test]
fn a_failing_step_undoes_the_ones_before_it() {
	// VK_ERROR_SURFACE_LOST_KHR, VK_ERROR_NATIVE_WINDOW_IN_USE_KHR and
	// VK_ERROR_OUT_OF_DEVICE_MEMORY.
	let cases = [
		(c"vkCreateSwapchainKHR", -1_000_000_000, 0, 0),
		(c"vkCreateSwapchainKHR", -1_000_000_001, 0, 0),
		(c"vkGetSwapchainImagesKHR", -2, 1, 0),
		(c"vkCreateImageView", -2, 1, 0),
	];
	for (call, code, swapchains, views) in cases {
		let mut driver = showing();
		driver.fails = vec![(call, code)];
		let (device, surface) = window(driver);
		let made = Swapchain::new(&device, &surface, plan()).err();
		assert_eq!(made, Some(Error::Failed(code)), "{call:?}");
		let driver = with(|driver| {
			(
				driver.swapchains.len(),
				driver.live_swapchains.len(),
				driver.live_views.len(),
			)
		});
		assert_eq!(driver, (swapchains, 0, views), "{call:?}");
		drop((device, surface));
		finish();
	}
}

#[test]
fn a_failed_view_destroys_the_new_swapchain_then_the_old_one() {
	let mut driver = showing();
	driver.surface_capabilities = Some(Capabilities {
		min_images: 2,
		..super::surface_query_tests::mesa_wayland()
	});
	let (device, surface) = window(driver);
	let first = Swapchain::new(
		&device,
		&surface,
		Plan {
			images: 2,
			..plan()
		},
	)
	.unwrap();
	// Views fail from now on: the remade swapchain's are never made.
	with(|driver| driver.fails = vec![(c"vkCreateImageView", -1)]);
	let made = first
		.recreate(Plan {
			images: 3,
			..plan()
		})
		.err();
	assert_eq!(made, Some(Error::Failed(-1)));
	let driver = with(|driver| (driver.live_swapchains.clone(), driver.released.clone()));
	assert_eq!(driver.0, [], "both are destroyed");
	// The new one at once, as it has no views yet; then the old one's views
	// and the old one, which they held.
	let released = [
		"swapchain 0x60002",
		"view 0xf00001",
		"view 0xf00002",
		"swapchain 0x60001",
	];
	assert_eq!(driver.1, released);
	drop((device, surface));
	finish();
}

#[test]
fn a_failed_recreation_still_retires_and_destroys_the_old_swapchain() {
	let (device, surface) = window(showing());
	let swapchain = Swapchain::new(&device, &surface, plan()).unwrap();
	with(|driver| driver.fails = vec![(c"vkCreateSwapchainKHR", -1_000_000_000)]);
	let made = swapchain.recreate(plan()).err();
	assert_eq!(made, Some(Error::Failed(-1_000_000_000)));
	let driver = with(|driver| (driver.live_swapchains.len(), driver.released.len()));
	assert_eq!(driver, (0, 5));
	// The surface takes a new swapchain afresh.
	with(|driver| driver.fails.clear());
	let fresh = Swapchain::new(&device, &surface, plan()).expect("swapchain");
	drop((fresh, device, surface));
	assert_eq!(finish().swapchains[1].old, 0);
}

/// Whether the plan for a window of `window` pixels, on a surface of
/// `driver` reached from `platform`, makes a swapchain the driver takes.
fn planned_and_made(mut driver: Driver, platform: Platform, window: [u32; 2]) -> MadeSwapchain {
	driver.lacks.clear();
	let instance = window_instance(install(driver), platform);
	let physical = instance.physical_devices().unwrap()[0];
	let device = Rc::new(Device::for_windows(instance.clone(), physical, 0).unwrap());
	let made = surface(
		&instance,
		if platform == Platform::Xlib {
			XLIB
		} else {
			WAYLAND
		},
	);
	let surface = Rc::new(made.unwrap());
	let plan = surface.plan(physical, window).expect("plan");
	let swapchain = Swapchain::new(&device, &surface, plan).expect("swapchain");
	drop((swapchain, device, surface, instance));
	finish().swapchains.remove(0)
}

#[test]
fn what_a_surface_reports_plans_a_swapchain_the_driver_takes() {
	// Mesa on Wayland: the window sets the size.
	let made = planned_and_made(showing(), Platform::Wayland, [1280, 720]);
	assert_eq!(
		(made.extent, made.images, made.mode),
		([1280, 720], 4, MAILBOX)
	);
	// Mesa on X11: the window's size, three images, FIFO only and an
	// undefined format standing for any.
	let mut driver = showing();
	driver.surface_capabilities = Some(Capabilities {
		min_images: 3,
		max_images: 0,
		current_extent: [800, 600],
		min_extent: [800, 600],
		max_extent: [800, 600],
		..super::surface_query_tests::mesa_wayland()
	});
	driver.surface_formats = Some(vec![SurfaceFormat {
		format: 0,
		color_space: SRGB_NONLINEAR,
	}]);
	driver.present_modes = Some(vec![FIFO]);
	let made = planned_and_made(driver, Platform::Xlib, [1, 1]);
	let expected = MadeSwapchain {
		surface: 0x5_0000,
		images: 3,
		format: SurfaceFormat {
			format: 44,
			color_space: SRGB_NONLINEAR,
		},
		extent: [800, 600],
		usage: USAGE,
		transform: 1,
		alpha: OPAQUE,
		mode: FIFO,
		clipped: true,
		old: 0,
	};
	assert_eq!(made, expected);
	// A window larger than the surface allows, and RGBA alone.
	let mut driver = showing();
	driver.surface_capabilities = Some(Capabilities {
		max_extent: [4096, 4096],
		max_images: 3,
		min_images: 2,
		..super::surface_query_tests::mesa_wayland()
	});
	driver.surface_formats = Some(vec![SurfaceFormat {
		format: 37,
		color_space: SRGB_NONLINEAR,
	}]);
	let made = planned_and_made(driver, Platform::Wayland, [5000, 300]);
	assert_eq!(
		(made.extent, made.images, made.format.format),
		([4096, 300], 3, 37)
	);
}

#[test]
fn the_plans_format_transform_and_alpha_reach_the_swapchain_and_its_views() {
	let mut driver = showing();
	// A screen turned 90°, a compositor blending pre-multiplied alpha.
	driver.surface_capabilities = Some(Capabilities {
		transforms: 1 | 2,
		current_transform: 2,
		alpha: PRE_MULTIPLIED,
		..super::surface_query_tests::mesa_wayland()
	});
	driver.surface_formats = Some(vec![SurfaceFormat {
		format: 37,
		color_space: SRGB_NONLINEAR,
	}]);
	let (device, surface) = window(driver);
	let turned = Plan {
		format: Format::Rgba,
		transform: 2,
		alpha: PRE_MULTIPLIED,
		..plan()
	};
	let swapchain = Swapchain::new(&device, &surface, turned).unwrap();
	let made = with(|driver| driver.swapchains[0].clone());
	assert_eq!(
		(made.format.format, made.transform, made.alpha),
		(37, 2, PRE_MULTIPLIED)
	);
	let views: Vec<_> = with(|driver| driver.views.iter().map(|view| view.format).collect());
	assert_eq!(views, [37; 4]);
	drop((swapchain, device, surface));
	finish();
}
