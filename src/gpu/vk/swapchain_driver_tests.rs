//! The fake driver's swapchain functions, for the tests of `driver_tests`:
//! they check the specification's valid usage rules against what the
//! surface reports, and hand out images the fake's views can be made of.
use super::{
	device_tests::Made,
	driver_tests::{Driver, answer, with},
	image_tests::MadeImage,
	surface_query_tests::{BGRA, BGRA_SRGB, mesa_wayland},
	surface_types::RawSurface,
	swapchain_plan::{FIFO, MAILBOX, SRGB_NONLINEAR, SurfaceFormat},
	swapchain_types::*,
	types::*,
};
use std::{ffi::CStr, mem::transmute};

/// What a swapchain was created with.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MadeSwapchain {
	pub surface: RawSurface,
	pub images: u32,
	pub format: SurfaceFormat,
	pub extent: [u32; 2],
	pub usage: u32,
	pub transform: u32,
	pub alpha: u32,
	pub mode: i32,
	/// Whether pixels other windows hide may go undrawn.
	pub clipped: bool,
	pub old: RawSwapchain,
}

/// A swapchain not yet destroyed.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LiveSwapchain {
	pub handle: RawSwapchain,
	pub device: RawDevice,
	pub surface: RawSurface,
	/// Made the old swapchain of a newer one: it no longer gives images.
	pub retired: bool,
	pub images: Vec<RawImage>,
	/// The indices of the images acquired and not yet presented.
	pub acquired: Vec<u32>,
}

/// The fake's swapchain function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateSwapchainKHR" => transmute::<CreateSwapchain, VoidFunction>(create_swapchain),
			b"vkDestroySwapchainKHR" => transmute::<DestroySwapchain, VoidFunction>(destroy_swapchain),
			b"vkGetSwapchainImagesKHR" => transmute::<GetSwapchainImages, VoidFunction>(images),
			_ => return super::present_driver_tests::function(name),
		})
	}
}

impl Driver {
	/// What `device` was created with, if the driver made it.
	fn made_device(&self, device: RawDevice) -> Option<&Made> {
		let index = (device as usize).checked_sub(0x5000)? / 0x100;
		self.made.get(index.checked_sub(1)?)
	}

	/// The live swapchain `handle`, if it is one.
	pub(super) fn swapchain(&self, handle: RawSwapchain) -> Option<&LiveSwapchain> {
		self
			.live_swapchains
			.iter()
			.find(|live| live.handle == handle)
	}

	/// Whether `image` is one a live swapchain gave.
	pub(super) fn presentable(&self, image: RawImage) -> bool {
		(self.live_swapchains.iter()).any(|live| live.images.contains(&image))
	}

	/// The valid usage rules `info` breaks on `device`, against what the
	/// surface reports.
	fn swapchain_rules(&self, device: RawDevice, info: &SwapchainCreateInfo) -> Vec<&'static str> {
		let enabled = (self.made_device(device)).is_some_and(|made| {
			made
				.extensions
				.iter()
				.any(|name| name == "VK_KHR_swapchain")
		});
		let capabilities = self
			.surface_capabilities
			.clone()
			.unwrap_or_else(mesa_wayland);
		let formats = (self.surface_formats.clone()).unwrap_or(vec![BGRA_SRGB, BGRA]);
		let modes = self.present_modes.clone().unwrap_or(vec![MAILBOX, FIFO, 0]);
		let surface_live = self
			.live_surfaces
			.iter()
			.any(|&(_, live)| live == info.surface);
		let format = SurfaceFormat {
			format: info.image_format,
			color_space: info.image_color_space,
		};
		let any = formats.len() == 1 && formats[0].format == 0;
		let images = info.min_image_count;
		let most = capabilities.max_images;
		let [width, height] = info.image_extent;
		let (min, max) = (capabilities.min_extent, capabilities.max_extent);
		let one_bit = |value: u32, allowed: u32| value.is_power_of_two() && allowed & value != 0;
		let old = (info.old_swapchain != 0).then(|| self.swapchain(info.old_swapchain));
		let others = self.live_swapchains.iter().filter(|live| {
			live.surface == info.surface && !live.retired && live.handle != info.old_swapchain
		});
		let rules = [
			(enabled, "a device without VK_KHR_swapchain"),
			(info.s_type == STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO, "sType"),
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(surface_live, "a live surface"),
			(
				images >= capabilities.min_images && (most == 0 || images <= most),
				"minImageCount",
			),
			(
				formats.contains(&format) || (any && info.image_color_space == SRGB_NONLINEAR),
				"a format and color space the surface lists",
			),
			(
				(min[0]..=max[0]).contains(&width)
					&& (min[1]..=max[1]).contains(&height)
					&& width * height != 0,
				"imageExtent",
			),
			(
				(1..=capabilities.max_layers).contains(&info.image_array_layers),
				"imageArrayLayers",
			),
			(
				info.image_usage != 0 && info.image_usage & !capabilities.usage == 0,
				"imageUsage",
			),
			(info.image_sharing_mode == 0, "exclusive sharing"),
			(
				one_bit(info.pre_transform, capabilities.transforms),
				"preTransform",
			),
			(
				one_bit(info.composite_alpha, capabilities.alpha),
				"compositeAlpha",
			),
			(modes.contains(&info.present_mode), "presentMode"),
			(info.clipped <= 1, "clipped"),
			(
				old.is_none_or(|old| old.is_some_and(|old| old.surface == info.surface && !old.retired)),
				"oldSwapchain",
			),
			(others.count() == 0, "a surface with a swapchain"),
		];
		rules
			.into_iter()
			.filter(|(holds, _)| !holds)
			.map(|(_, rule)| rule)
			.collect()
	}
}

unsafe extern "system" fn create_swapchain(
	device: RawDevice,
	info: *const SwapchainCreateInfo,
	allocator: *const std::ffi::c_void,
	swapchain: *mut RawSwapchain,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		driver.check_live_device(device, "vkCreateSwapchainKHR");
		let mut broken = driver.swapchain_rules(device, info);
		if !allocator.is_null() {
			broken.push("an allocator");
		}
		(driver.invalid).extend(
			broken
				.iter()
				.map(|rule| format!("vkCreateSwapchainKHR: {rule}")),
		);
		// The old one is retired even when this fails.
		let old = info.old_swapchain;
		if let Some(live) = driver
			.live_swapchains
			.iter_mut()
			.find(|live| live.handle == old)
		{
			live.retired = true;
		}
		if let Some(code) = driver.failing(c"vkCreateSwapchainKHR") {
			return code;
		}
		driver.swapchains.push(MadeSwapchain {
			surface: info.surface,
			images: info.min_image_count,
			format: SurfaceFormat {
				format: info.image_format,
				color_space: info.image_color_space,
			},
			extent: info.image_extent,
			usage: info.image_usage,
			transform: info.pre_transform,
			alpha: info.composite_alpha,
			mode: info.present_mode,
			clipped: info.clipped == 1,
			old,
		});
		let made = (0..info.min_image_count).map(|_| make_image(driver, info));
		let images = made.collect();
		let handle = 0x6_0000 + driver.swapchains.len() as RawSwapchain;
		driver.live_swapchains.push(LiveSwapchain {
			handle,
			device,
			surface: info.surface,
			retired: false,
			images,
			acquired: Vec::new(),
		});
		// SAFETY: the bindings pass a writable handle.
		unsafe { swapchain.write(handle) };
		0
	})
}

/// A swapchain image as `info` asks for, live and bound so views can be
/// made of it.
fn make_image(driver: &mut Driver, info: &SwapchainCreateInfo) -> RawImage {
	let [width, height] = info.image_extent;
	driver.images.push(MadeImage {
		format: info.image_format,
		extent: [width, height, 1],
		layers: info.image_array_layers,
		usage: info.image_usage,
		fixed: [1, 1, 0, 0],
	});
	let handle = 0xa0_0000 + driver.images.len() as RawImage;
	driver.live_images.push(handle);
	driver.bound_images.push((handle, 0));
	handle
}

unsafe extern "system" fn destroy_swapchain(
	device: RawDevice,
	swapchain: RawSwapchain,
	allocator: *const std::ffi::c_void,
) {
	with(|driver| {
		driver.check_live_device(device, "vkDestroySwapchainKHR");
		let Some(live) = driver.swapchain(swapchain).cloned() else {
			let message = format!("vkDestroySwapchainKHR of {swapchain:#x}, which is not live");
			return driver.invalid.push(message);
		};
		let viewed = (driver.live_views.iter()).any(|&view| {
			driver
				.viewed(view)
				.is_some_and(|image| live.images.contains(&image))
		});
		let rules = [
			(allocator.is_null(), "an allocator"),
			(live.device == device, "another device's swapchain"),
			(!viewed, "views of its images are live"),
			(!driver.presenting, "presentation may be pending"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkDestroySwapchainKHR: {rule}"));
		}
		driver
			.live_images
			.retain(|image| !live.images.contains(image));
		driver
			.bound_images
			.retain(|(image, _)| !live.images.contains(image));
		driver
			.live_swapchains
			.retain(|other| other.handle != swapchain);
		driver.released.push(format!("swapchain {swapchain:#x}"));
	})
}

unsafe extern "system" fn images(
	device: RawDevice,
	swapchain: RawSwapchain,
	count: *mut u32,
	images: *mut RawImage,
) -> i32 {
	with(|driver| {
		driver.check_live_device(device, "vkGetSwapchainImagesKHR");
		let Some(live) = driver.swapchain(swapchain).cloned() else {
			let message = format!("vkGetSwapchainImagesKHR of {swapchain:#x}, which is not live");
			driver.invalid.push(message);
			return -3;
		};
		if let Some(code) = driver.failing(c"vkGetSwapchainImagesKHR") {
			return code;
		}
		// SAFETY: passed on as the bindings gave them.
		unsafe { answer(&live.images, count, images) }
	})
}

/// A create info Mesa takes on Wayland for `surface`, replacing `old`.
fn good(surface: RawSurface, old: RawSwapchain) -> SwapchainCreateInfo {
	SwapchainCreateInfo {
		s_type: STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO,
		next: std::ptr::null(),
		flags: 0,
		surface,
		min_image_count: 4,
		image_format: 44,
		image_color_space: SRGB_NONLINEAR,
		image_extent: [640, 480],
		image_array_layers: 1,
		image_usage: super::swapchain_plan::USAGE,
		image_sharing_mode: 0,
		queue_family_index_count: 0,
		queue_family_indices: std::ptr::null(),
		pre_transform: 1,
		composite_alpha: super::swapchain_plan::OPAQUE,
		present_mode: MAILBOX,
		clipped: 1,
		old_swapchain: old,
	}
}

/// Calls the fake to create a swapchain on `device` with `info`: the code,
/// the handle, and the rules it reported broken.
fn create(device: RawDevice, info: &SwapchainCreateInfo) -> (i32, RawSwapchain, Vec<String>) {
	let mut handle = 0;
	// SAFETY: the fake reads only the create info.
	let code = unsafe { create_swapchain(device, info, std::ptr::null(), &mut handle) };
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	(code, handle, invalid)
}

fn destroy(device: RawDevice, swapchain: RawSwapchain) -> Vec<String> {
	// SAFETY: the fake only records the handles.
	unsafe { destroy_swapchain(device, swapchain, std::ptr::null()) };
	with(|driver| std::mem::take(&mut driver.invalid))
}

/// A device for windows, its handle, and a live Wayland surface.
fn window() -> (std::rc::Rc<super::device::Device>, super::surface::Surface) {
	use super::{choose::*, driver_tests::*, surface::Platform, surface_driver_tests::*};
	let driver = with_devices(&[gpu(Kind::Integrated, vec![present(GRAPHICS, true)])]);
	let instance = window_instance(install(driver), Platform::Wayland);
	let physical = instance.physical_devices().unwrap()[0];
	let device = super::device::Device::for_windows(instance.clone(), physical, 0).unwrap();
	(
		std::rc::Rc::new(device),
		surface(&instance, WAYLAND).unwrap(),
	)
}

#[test]
fn the_fake_driver_reports_each_broken_swapchain_rule() {
	// Were it blind to them, the swapchain tests would prove nothing.
	let (device, surface) = window();
	let (handle, surface_handle) = (device.handle, surface.handle());
	let base = good(surface_handle, 0);
	let cases: [(SwapchainCreateInfo, &str); 15] = [
		(SwapchainCreateInfo { s_type: 3, ..base }, "sType"),
		(SwapchainCreateInfo { flags: 1, ..base }, "pNext and flags"),
		(
			SwapchainCreateInfo {
				surface: 0x5_0009,
				..base
			},
			"a live surface",
		),
		(
			SwapchainCreateInfo {
				min_image_count: 3,
				..base
			},
			"minImageCount",
		),
		(
			SwapchainCreateInfo {
				image_format: 37,
				..base
			},
			"a format and color space the surface lists",
		),
		(
			// VK_COLOR_SPACE_PASS_THROUGH_EXT.
			SwapchainCreateInfo {
				image_color_space: 1_000_104_013,
				..base
			},
			"a format and color space the surface lists",
		),
		(
			SwapchainCreateInfo {
				image_extent: [0, 480],
				..base
			},
			"imageExtent",
		),
		(
			SwapchainCreateInfo {
				image_extent: [16385, 480],
				..base
			},
			"imageExtent",
		),
		(
			SwapchainCreateInfo {
				image_array_layers: 2,
				..base
			},
			"imageArrayLayers",
		),
		// VK_IMAGE_USAGE_FRAGMENT_DENSITY_MAP_BIT_EXT, which Mesa does not offer.
		(
			SwapchainCreateInfo {
				image_usage: 0x200,
				..base
			},
			"imageUsage",
		),
		(
			SwapchainCreateInfo {
				image_sharing_mode: 1,
				..base
			},
			"exclusive sharing",
		),
		(
			SwapchainCreateInfo {
				pre_transform: 2,
				..base
			},
			"preTransform",
		),
		(
			SwapchainCreateInfo {
				composite_alpha: 1 | 2,
				..base
			},
			"compositeAlpha",
		),
		// VK_PRESENT_MODE_FIFO_RELAXED_KHR, which it does not list.
		(
			SwapchainCreateInfo {
				present_mode: 3,
				..base
			},
			"presentMode",
		),
		(SwapchainCreateInfo { clipped: 2, ..base }, "clipped"),
	];
	let offscreen = super::device::Device::new(device.instance.clone(), device.physical, 0).unwrap();
	let (_, made, invalid) = create(offscreen.handle, &base);
	let rule = "vkCreateSwapchainKHR: a device without VK_KHR_swapchain";
	assert_eq!(invalid, [rule]);
	let invalid = destroy(handle, made);
	assert_eq!(
		invalid,
		["vkDestroySwapchainKHR: another device's swapchain"]
	);
	drop(offscreen);
	for (info, rule) in cases {
		let (code, made, invalid) = create(handle, &info);
		assert_eq!(code, 0, "{rule}");
		assert_eq!(invalid, [format!("vkCreateSwapchainKHR: {rule}")]);
		assert_eq!(destroy(handle, made), Vec::<String>::new());
	}
	// A second swapchain on the surface must replace the first, which is
	// then retired and cannot be replaced again.
	let rule = |rule: &str| format!("vkCreateSwapchainKHR: {rule}");
	let (_, first, invalid) = create(handle, &base);
	assert_eq!(invalid, Vec::<String>::new());
	let (_, second, invalid) = create(handle, &good(surface_handle, first));
	assert_eq!(invalid, Vec::<String>::new());
	assert!(with(|driver| driver.swapchain(first).unwrap().retired));
	let (_, third, invalid) = create(handle, &good(surface_handle, first));
	let both = [rule("oldSwapchain"), rule("a surface with a swapchain")];
	assert_eq!(invalid, both);
	let (_, fourth, invalid) = create(handle, &base);
	assert_eq!(invalid, [rule("a surface with a swapchain")]);
	for made in [first, second, third, fourth] {
		assert_eq!(destroy(handle, made), Vec::<String>::new());
	}
	assert_eq!(
		destroy(handle, first),
		[format!(
			"vkDestroySwapchainKHR of {first:#x}, which is not live"
		)]
	);
	drop((device, surface));
	super::driver_tests::finish();
}

#[test]
fn the_fake_driver_reports_swapchains_misused_around_their_images() {
	let (device, surface) = window();
	let handle = device.handle;
	let (_, swapchain, _) = create(handle, &good(surface.handle(), 0));
	let image = with(|driver| driver.swapchain(swapchain).unwrap().images[0]);
	// A view of an image keeps the swapchain from being destroyed.
	let view = super::image::Image::presented(
		&device,
		image,
		(
			super::image::Format::Bgra,
			[640, 480],
			super::swapchain_plan::USAGE,
		),
		std::rc::Rc::new(()),
	)
	.unwrap();
	assert_eq!(
		destroy(handle, swapchain),
		["vkDestroySwapchainKHR: views of its images are live"]
	);
	drop(view);
	// So does presentation that may still be pending, until the queue idles.
	let (_, swapchain, _) = create(handle, &good(surface.handle(), 0));
	with(|driver| driver.presenting = true);
	let pending = ["vkDestroySwapchainKHR: presentation may be pending"];
	assert_eq!(destroy(handle, swapchain), pending);
	with(|driver| driver.presenting = false);
	// Destroying a swapchain's image, or its surface before it, is broken.
	let (_, swapchain, _) = create(handle, &good(surface.handle(), 0));
	let image = with(|driver| driver.swapchain(swapchain).unwrap().images[0]);
	let destroy_image: super::types::DestroyImage =
		// SAFETY: the fake's own function, cast back to its type.
		unsafe { transmute(super::image_tests::function(c"vkDestroyImage").unwrap()) };
	// SAFETY: the fake only records the handles.
	unsafe { destroy_image(handle, image, std::ptr::null()) };
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(
		invalid,
		[format!("vkDestroyImage: {image:#x}, a swapchain's")]
	);
	let mut count = 0;
	// SAFETY: as above.
	let code = unsafe { images(handle, 0x6_0099, &mut count, std::ptr::null_mut()) };
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!((code, invalid.len()), (-3, 1));
	let live = with(|driver| driver.live_swapchains.len());
	assert_eq!(live, 1);
	drop(surface);
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(invalid, ["vkDestroySurfaceKHR while its swapchain is live"]);
	assert_eq!(destroy(handle, swapchain), Vec::<String>::new());
	drop(device);
	super::driver_tests::finish();
}
