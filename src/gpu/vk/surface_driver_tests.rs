//! Tests of window instances and surfaces against the fake driver of
//! `driver_tests`: the extensions enabled, the handles passed on, which
//! queue families show, and the order surfaces and instances are
//! destroyed in. The fake's surface functions live here and check the
//! specification's valid usage rules.
use super::{
	call::Error,
	choose::{Choice, Device, GRAPHICS, Kind, QueueFamily, choose},
	driver_tests::{Driver, finish, install, with, with_devices},
	entry::Entry,
	instance::Instance,
	surface::{Platform, Surface, Window},
	surface_types::*,
	types::*,
	version::api_version,
};
use std::{
	ffi::{CStr, c_ulong, c_void},
	mem::transmute,
	ptr::{self, NonNull},
	rc::Rc,
};

/// What a surface was made from.
#[derive(Debug, PartialEq)]
pub(crate) struct MadeSurface {
	pub instance: RawInstance,
	pub platform: Platform,
	pub display: usize,
	/// The `wl_surface`, or the Xlib window.
	pub window: usize,
}

/// The fake's surface function `name`, if it has one.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateWaylandSurfaceKHR" => {
				transmute::<CreateWaylandSurface, VoidFunction>(create_wayland_surface)
			}
			b"vkCreateXlibSurfaceKHR" => {
				transmute::<CreateXlibSurface, VoidFunction>(create_xlib_surface)
			}
			b"vkCreateHeadlessSurfaceEXT" => {
				transmute::<CreateHeadlessSurface, VoidFunction>(create_headless_surface)
			}
			b"vkDestroySurfaceKHR" => transmute::<DestroySurface, VoidFunction>(destroy_surface),
			b"vkGetPhysicalDeviceSurfaceSupportKHR" => {
				transmute::<GetPhysicalDeviceSurfaceSupport, VoidFunction>(surface_support)
			}
			_ => return None,
		})
	}
}

/// Checks a surface create call, and makes the surface unless it fails.
fn create(
	driver: &mut Driver,
	(call, extension): (&'static CStr, &str),
	instance: RawInstance,
	(s_type, next, flags): (bool, *const c_void, u32),
	made: MadeSurface,
	surface: *mut RawSurface,
) -> i32 {
	let name = call.to_string_lossy();
	driver.check_live(instance, &name);
	let enabled = (driver.created.iter()).any(|created| {
		created.handle == instance && created.extensions.iter().any(|name| name == extension)
	});
	let rules = [
		(enabled, "an instance without the extension"),
		(s_type, "sType"),
		(next.is_null() && flags == 0, "pNext and flags"),
		(
			made.display != 0 && made.window != 0,
			"no display or window",
		),
	];
	let broken = rules.iter().filter(|(holds, _)| !holds);
	(driver.invalid).extend(broken.map(|(_, rule)| format!("{name}: {rule}")));
	if let Some(code) = driver.failing(call) {
		return code;
	}
	let handle = 0x5_0000 + driver.surfaces.len() as RawSurface;
	driver.surfaces.push(made);
	driver.live_surfaces.push((instance, handle));
	// SAFETY: the bindings pass a writable handle.
	unsafe { surface.write(handle) };
	0
}

unsafe extern "system" fn create_wayland_surface(
	instance: RawInstance,
	info: *const WaylandSurfaceCreateInfo,
	allocator: *const c_void,
	surface: *mut RawSurface,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		if !allocator.is_null() {
			(driver.invalid).push("vkCreateWaylandSurfaceKHR: an allocator".into());
		}
		let made = MadeSurface {
			instance,
			platform: Platform::Wayland,
			display: info.display as usize,
			window: info.surface as usize,
		};
		let s_type = info.s_type == STRUCTURE_TYPE_WAYLAND_SURFACE_CREATE_INFO;
		let call = (c"vkCreateWaylandSurfaceKHR", "VK_KHR_wayland_surface");
		create(
			driver,
			call,
			instance,
			(s_type, info.next, info.flags),
			made,
			surface,
		)
	})
}

unsafe extern "system" fn create_xlib_surface(
	instance: RawInstance,
	info: *const XlibSurfaceCreateInfo,
	allocator: *const c_void,
	surface: *mut RawSurface,
) -> i32 {
	// SAFETY: as above.
	let info = unsafe { &*info };
	with(|driver| {
		if !allocator.is_null() {
			(driver.invalid).push("vkCreateXlibSurfaceKHR: an allocator".into());
		}
		let made = MadeSurface {
			instance,
			platform: Platform::Xlib,
			display: info.display as usize,
			window: info.window as usize,
		};
		let s_type = info.s_type == STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO;
		let call = (c"vkCreateXlibSurfaceKHR", "VK_KHR_xlib_surface");
		create(
			driver,
			call,
			instance,
			(s_type, info.next, info.flags),
			made,
			surface,
		)
	})
}

unsafe extern "system" fn create_headless_surface(
	instance: RawInstance,
	info: *const HeadlessSurfaceCreateInfo,
	allocator: *const c_void,
	surface: *mut RawSurface,
) -> i32 {
	// SAFETY: as above.
	let info = unsafe { &*info };
	with(|driver| {
		if !allocator.is_null() {
			(driver.invalid).push("vkCreateHeadlessSurfaceEXT: an allocator".into());
		}
		// Nothing to show in, so nothing to check.
		let made = MadeSurface {
			instance,
			platform: Platform::Headless,
			display: 1,
			window: 1,
		};
		let s_type = info.s_type == STRUCTURE_TYPE_HEADLESS_SURFACE_CREATE_INFO;
		let call = (c"vkCreateHeadlessSurfaceEXT", "VK_EXT_headless_surface");
		create(
			driver,
			call,
			instance,
			(s_type, info.next, info.flags),
			made,
			surface,
		)
	})
}

unsafe extern "system" fn destroy_surface(
	instance: RawInstance,
	surface: RawSurface,
	allocator: *const c_void,
) {
	with(|driver| {
		driver.check_live(instance, "vkDestroySurfaceKHR");
		if !allocator.is_null() {
			(driver.invalid).push("vkDestroySurfaceKHR: an allocator".into());
		}
		let before = driver.live_surfaces.len();
		(driver.live_surfaces).retain(|&live| live != (instance, surface));
		if driver.live_surfaces.len() == before {
			let message = format!("vkDestroySurfaceKHR of {surface:#x}, not live on {instance:?}");
			driver.invalid.push(message);
		}
		driver.released.push(format!("surface {surface:#x}"));
	})
}

unsafe extern "system" fn surface_support(
	physical: RawPhysicalDevice,
	family: u32,
	surface: RawSurface,
	supported: *mut u32,
) -> i32 {
	with(|driver| {
		const CALL: &str = "vkGetPhysicalDeviceSurfaceSupportKHR";
		let Some(device) = driver.device(physical, CALL) else {
			return -3;
		};
		if !driver
			.live_surfaces
			.iter()
			.any(|&(_, live)| live == surface)
		{
			driver.invalid.push(format!("{CALL} on a surface not live"));
		}
		let Some(found) = device.families.get(family as usize) else {
			driver.invalid.push(format!(
				"{CALL}: family {family} of {}",
				device.families.len()
			));
			return -3;
		};
		if let Some(code) = driver.failing(c"vkGetPhysicalDeviceSurfaceSupportKHR") {
			return code;
		}
		// SAFETY: the bindings pass a writable flag.
		unsafe { supported.write(found.present.into()) };
		0
	})
}

const DISPLAY: NonNull<c_void> =
	NonNull::without_provenance(std::num::NonZero::new(0xd15).unwrap());
const WL_SURFACE: NonNull<c_void> =
	NonNull::without_provenance(std::num::NonZero::new(0x5ace).unwrap());
const WAYLAND: Window = Window::Wayland {
	display: DISPLAY,
	surface: WL_SURFACE,
};
const XLIB: Window = Window::Xlib {
	display: DISPLAY,
	window: 0x0420_0007 as c_ulong,
};

fn window_instance(entry: Entry, platform: Platform) -> Rc<Instance> {
	Rc::new(
		entry
			.create_window_instance(c"poopterm", platform)
			.expect("instance"),
	)
}

fn surface(instance: &Rc<Instance>, window: Window) -> Result<Surface, Error> {
	// SAFETY: the fake driver only records the handles.
	unsafe { Surface::new(instance, window) }
}

fn present(flags: u32, present: bool) -> QueueFamily {
	QueueFamily {
		flags,
		count: 1,
		present,
	}
}

fn gpu(kind: Kind, families: Vec<QueueFamily>) -> Device {
	Device {
		name: format!("{kind:?}"),
		kind,
		api: api_version(1, 3),
		families,
		swapchain: true,
	}
}

#[test]
fn window_instances_enable_the_surface_extensions_of_their_platform() {
	let cases = [
		(Platform::Wayland, "VK_KHR_wayland_surface"),
		(Platform::Xlib, "VK_KHR_xlib_surface"),
		(Platform::Headless, "VK_EXT_headless_surface"),
	];
	for (platform, own) in cases {
		drop(window_instance(install(with_devices(&[])), platform));
		let driver = finish();
		assert_eq!(driver.created[0].extensions, ["VK_KHR_surface", own]);
		assert_eq!(driver.live, []);
	}
}

#[test]
fn a_window_instance_needs_both_extensions_and_names_the_one_missing() {
	let cases = [
		(vec!["VK_KHR_wayland_surface"], c"VK_KHR_surface"),
		(
			vec!["VK_KHR_surface", "VK_KHR_xlib_surface"],
			c"VK_KHR_wayland_surface",
		),
		(vec![], c"VK_KHR_surface"),
	];
	for (offered, missing) in cases {
		let mut driver = with_devices(&[]);
		driver.instance_extensions = Some(offered);
		let entry = install(driver);
		let made = entry.create_window_instance(c"poopterm", Platform::Wayland);
		assert_eq!(made.err(), Some(Error::Missing(missing)));
		let driver = finish();
		assert_eq!(driver.created, [], "nothing is created without them");
	}
}

#[test]
fn listing_instance_extensions_fails_with_the_loaders_code_or_its_absence() {
	let mut driver = with_devices(&[]);
	driver.fails = vec![(c"vkEnumerateInstanceExtensionProperties", -1)];
	let entry = install(driver);
	assert_eq!(entry.instance_extensions(), Err(Error::Failed(-1)));
	let made = entry.create_window_instance(c"poopterm", Platform::Xlib);
	assert_eq!(made.err(), Some(Error::Failed(-1)));
	assert_eq!(finish().created, []);

	let mut driver = with_devices(&[]);
	driver.lacks = vec![c"vkEnumerateInstanceExtensionProperties"];
	let entry = install(driver);
	let missing = Error::Missing(c"vkEnumerateInstanceExtensionProperties");
	assert_eq!(entry.instance_extensions(), Err(missing));
	finish();
}

#[test]
fn instance_extensions_are_listed_as_the_loader_offers_them() {
	let entry = install(with_devices(&[]));
	let listed = entry.instance_extensions().unwrap();
	assert_eq!(listed, super::driver_tests::OFFERED);
	finish();
}

#[test]
fn a_wayland_surface_is_made_from_the_windows_handles_and_destroyed_first() {
	let instance = window_instance(install(with_devices(&[])), Platform::Wayland);
	let made = surface(&instance, WAYLAND).expect("surface");
	drop(instance);
	with(|driver| assert_eq!(driver.live.len(), 1, "the surface keeps the instance"));
	drop(made);
	let driver = finish();
	let expected = MadeSurface {
		instance: 0x1010 as RawInstance,
		platform: Platform::Wayland,
		display: 0xd15,
		window: 0x5ace,
	};
	assert_eq!(driver.surfaces, [expected]);
	assert_eq!(driver.released, ["surface 0x50000"]);
	assert_eq!((driver.live_surfaces, driver.live), (vec![], vec![]));
}

#[test]
fn an_xlib_surface_passes_the_display_and_window_id() {
	let instance = window_instance(install(with_devices(&[])), Platform::Xlib);
	drop(surface(&instance, XLIB).expect("surface"));
	drop(instance);
	let driver = finish();
	let expected = MadeSurface {
		instance: 0x1010 as RawInstance,
		platform: Platform::Xlib,
		display: 0xd15,
		window: 0x0420_0007,
	};
	assert_eq!(driver.surfaces, [expected]);
	assert_eq!(driver.released, ["surface 0x50000"]);
}

#[test]
fn a_headless_surface_needs_no_window() {
	let instance = window_instance(install(with_devices(&[])), Platform::Headless);
	drop(surface(&instance, Window::Headless).expect("surface"));
	drop(instance);
	let driver = finish();
	assert_eq!(driver.surfaces.len(), 1);
	assert_eq!(driver.surfaces[0].platform, Platform::Headless);
	assert_eq!(driver.released, ["surface 0x50000"]);
}

#[test]
fn a_surface_needs_an_instance_made_for_its_platform() {
	let entry = install(with_devices(&[]));
	let offscreen = Rc::new(entry.create_instance(c"poopterm").unwrap());
	let error = surface(&offscreen, XLIB).err();
	assert_eq!(error, Some(Error::Missing(c"VK_KHR_xlib_surface")));
	drop(offscreen);
	assert_eq!(finish().surfaces, []);
	let instance = window_instance(install(with_devices(&[])), Platform::Xlib);
	let error = surface(&instance, WAYLAND).err();
	assert_eq!(error, Some(Error::Missing(c"VK_KHR_wayland_surface")));
	drop(instance);
	let driver = finish();
	assert_eq!(driver.surfaces, []);
	let looked_up = driver
		.lookups
		.iter()
		.any(|(_, name)| name.contains("Surface"));
	assert!(!looked_up, "no surface function is looked up");
}

#[test]
fn a_failed_surface_is_an_error_and_nothing_is_destroyed() {
	let mut driver = with_devices(&[]);
	driver.fails = vec![(c"vkCreateWaylandSurfaceKHR", -1_000_000_001)];
	let instance = window_instance(install(driver), Platform::Wayland);
	let error = surface(&instance, WAYLAND).err();
	assert_eq!(error, Some(Error::Failed(-1_000_000_001)));
	drop(instance);
	let driver = finish();
	assert_eq!(driver.released, Vec::<String>::new());
	assert_eq!(driver.live, []);
}

#[test]
fn a_surface_function_the_driver_lacks_is_missing_before_anything_is_made() {
	let lacked = [
		c"vkDestroySurfaceKHR",
		c"vkGetPhysicalDeviceSurfaceSupportKHR",
		c"vkCreateXlibSurfaceKHR",
	];
	for name in lacked {
		let mut driver = with_devices(&[]);
		driver.lacks = vec![name];
		let instance = window_instance(install(driver), Platform::Xlib);
		assert_eq!(surface(&instance, XLIB).err(), Some(Error::Missing(name)));
		drop(instance);
		assert_eq!(finish().surfaces, []);
	}
}

#[test]
fn present_support_is_asked_of_every_family_and_replaces_what_was_marked() {
	let truth = gpu(
		Kind::Integrated,
		vec![
			present(GRAPHICS, true),
			present(4, false),
			present(GRAPHICS, true),
		],
	);
	let driver = with_devices(std::slice::from_ref(&truth));
	let instance = window_instance(install(driver), Platform::Wayland);
	let made = surface(&instance, WAYLAND).unwrap();
	let physical = instance.physical_devices().unwrap()[0];
	let mut described = instance.describe(physical).unwrap();
	described.families[1].present = true;
	made.mark_present(physical, &mut described).unwrap();
	assert_eq!(described, truth);
	drop((made, instance));
	finish();
}

#[test]
fn a_failed_support_query_is_an_error() {
	let mut driver = with_devices(&[gpu(Kind::Discrete, vec![present(GRAPHICS, true)])]);
	driver.fails = vec![(c"vkGetPhysicalDeviceSurfaceSupportKHR", -1_000_000_000)];
	let instance = window_instance(install(driver), Platform::Xlib);
	let made = surface(&instance, XLIB).unwrap();
	let physical = instance.physical_devices().unwrap()[0];
	let mut described = instance.describe(physical).unwrap();
	let error = made.mark_present(physical, &mut described);
	assert_eq!(error, Err(Error::Failed(-1_000_000_000)));
	drop((made, instance));
	finish();
}

#[test]
fn a_window_draws_on_the_device_whose_family_shows_on_its_surface() {
	// The integrated GPU is preferred, but only the discrete one shows here,
	// and on it only its second graphics family.
	let devices = [
		gpu(Kind::Integrated, vec![present(GRAPHICS, false)]),
		gpu(
			Kind::Discrete,
			vec![present(GRAPHICS, false), present(GRAPHICS, true)],
		),
	];
	let instance = window_instance(install(with_devices(&devices)), Platform::Wayland);
	let made = surface(&instance, WAYLAND).unwrap();
	let mut described = Vec::new();
	for physical in instance.physical_devices().unwrap() {
		let mut device = instance.describe(physical).unwrap();
		made.mark_present(physical, &mut device).unwrap();
		described.push(device);
	}
	let choice = Some(Choice {
		device: 1,
		family: 1,
	});
	assert_eq!(choose(&described, true, false), choice);
	assert_eq!(choose(&described, false, false).unwrap().device, 0);
	drop((made, instance));
	finish();
}

#[test]
fn the_fake_driver_reports_broken_surface_rules() {
	let instance = window_instance(install(with_devices(&[])), Platform::Wayland);
	// SAFETY: the fake's function for this name has this type.
	let create: CreateWaylandSurface =
		unsafe { transmute(function(c"vkCreateWaylandSurfaceKHR").unwrap()) };
	let info = WaylandSurfaceCreateInfo {
		s_type: STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO,
		next: ptr::null(),
		flags: 1,
		display: ptr::null_mut(),
		surface: WL_SURFACE.as_ptr(),
	};
	let mut handle = 0;
	// SAFETY: the fake reads the info and writes the handle.
	unsafe { create(instance.handle(), &info, ptr::null(), &mut handle) };
	// SAFETY: as above.
	let destroy: DestroySurface = unsafe { transmute(function(c"vkDestroySurfaceKHR").unwrap()) };
	// SAFETY: the fake only records the call.
	unsafe { destroy(instance.handle(), 0x7777, ptr::null()) };
	// SAFETY: as above.
	unsafe { destroy(instance.handle(), handle, ptr::null()) };
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(
		invalid,
		[
			"vkCreateWaylandSurfaceKHR: sType",
			"vkCreateWaylandSurfaceKHR: pNext and flags",
			"vkCreateWaylandSurfaceKHR: no display or window",
			"vkDestroySurfaceKHR of 0x7777, not live on 0x1010",
		]
	);
	drop(instance);
	finish();
}

#[test]
fn the_fake_driver_reports_surfaces_left_live_and_support_out_of_range() {
	let instance = window_instance(install(with_devices(&[])), Platform::Xlib);
	let made = surface(&instance, XLIB).unwrap();
	// SAFETY: as above.
	let support: GetPhysicalDeviceSurfaceSupport =
		unsafe { transmute(function(c"vkGetPhysicalDeviceSurfaceSupportKHR").unwrap()) };
	with(|driver| {
		driver.devices = vec![0x100 as RawPhysicalDevice];
		driver.described = vec![gpu(Kind::Cpu, vec![present(GRAPHICS, true)])];
	});
	let mut supported = 7;
	// SAFETY: the fake writes the flag only for a family it has.
	let code = unsafe { support(0x100 as RawPhysicalDevice, 1, 0x50000, &mut supported) };
	assert_eq!((code, supported), (-3, 7));
	// Destroying the instance first leaves the surface live.
	let handle = instance.handle();
	with(|driver| driver.live_surfaces.push((handle, 0x9)));
	drop(made);
	drop(instance);
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(
		invalid,
		[
			"vkGetPhysicalDeviceSurfaceSupportKHR: family 1 of 1",
			"vkDestroyInstance while a surface is live",
		]
	);
	finish();
}
