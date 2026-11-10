//! Tests of FIFO_LATEST_READY against the fake driver of `driver_tests`:
//! chosen when the surface has no mailbox, only on a device made with the
//! extension and its feature, by whichever name the device offers it; and
//! the fake checking the rules each step must keep.
use super::{
	call::Error,
	device::Device,
	driver_tests::{Driver, finish, install, with},
	optional::{FIFO_LATEST_READY_FEATURE, Optional},
	surface::Platform,
	surface_driver_tests::{WAYLAND, surface, window_instance},
	swapchain::Swapchain,
	swapchain_plan::{FIFO, FIFO_LATEST_READY, MAILBOX},
	window_tests::{SIZE, close, draw, open, showing},
};
use crate::gpu::facts::mode_name;
use std::rc::Rc;

const KHR: &str = "VK_KHR_present_mode_fifo_latest_ready";
const EXT: &str = "VK_EXT_present_mode_fifo_latest_ready";

/// A machine whose device offers `extensions`, on a surface offering
/// `modes`.
fn offering(extensions: Vec<&'static str>, modes: Vec<i32>) -> Driver {
	let mut driver = showing();
	driver.device_extensions = extensions;
	driver.present_modes = Some(modes);
	driver
}

/// The present mode and device a window on `driver` draws a frame with:
/// its extensions and whether the feature was turned on.
fn drawn_with(driver: Driver) -> (i32, Vec<String>, bool) {
	let mut window = open(driver, false).unwrap();
	draw(&mut window, SIZE).unwrap();
	let driver = close(window);
	let made = &driver.made[0];
	let feature = made.enabled.contains(&(FIFO_LATEST_READY_FEATURE, 0));
	(driver.swapchains[0].mode, made.extensions.clone(), feature)
}

fn names(list: &[&str]) -> Vec<String> {
	list.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn a_surface_without_mailbox_shows_the_newest_frame_ready() {
	let driver = offering(vec![KHR], vec![FIFO, FIFO_LATEST_READY]);
	let (mode, extensions, feature) = drawn_with(driver);
	assert_eq!(mode, FIFO_LATEST_READY);
	assert_eq!(extensions, names(&["VK_KHR_swapchain", KHR]));
	assert!(feature);
}

#[test]
fn an_older_driver_offering_the_ext_alone_is_made_with_the_ext() {
	let driver = offering(vec![EXT], vec![FIFO_LATEST_READY, FIFO]);
	let (mode, extensions, feature) = drawn_with(driver);
	assert_eq!(mode, FIFO_LATEST_READY);
	assert_eq!(extensions, names(&["VK_KHR_swapchain", EXT]));
	assert!(feature);
	// Offering both, it is made with the KHR alone.
	let driver = offering(vec![EXT, KHR], vec![FIFO_LATEST_READY, FIFO]);
	assert_eq!(drawn_with(driver).1, names(&["VK_KHR_swapchain", KHR]));
}

#[test]
fn mailbox_stays_first_where_the_surface_has_it() {
	let driver = offering(vec![KHR], vec![FIFO_LATEST_READY, MAILBOX, FIFO]);
	let (mode, extensions, _) = drawn_with(driver);
	assert_eq!(mode, MAILBOX);
	// Still made with it: a remade swapchain may lose mailbox.
	assert_eq!(extensions, names(&["VK_KHR_swapchain", KHR]));
}

#[test]
fn a_device_lacking_the_feature_keeps_plain_fifo_and_leaves_the_extension_off() {
	let mut driver = offering(vec![KHR], vec![FIFO, FIFO_LATEST_READY]);
	driver.lacked_features = vec![FIFO_LATEST_READY_FEATURE];
	let (mode, extensions, feature) = drawn_with(driver);
	assert_eq!(mode, FIFO);
	assert_eq!(extensions, names(&["VK_KHR_swapchain"]));
	assert!(!feature);
}

#[test]
fn a_mode_the_surface_lists_for_a_device_without_the_extension_is_not_used() {
	// Surfaces are the instance's: they may list modes some device lacks.
	let driver = offering(vec![], vec![FIFO_LATEST_READY, FIFO]);
	let (mode, extensions, feature) = drawn_with(driver);
	assert_eq!(mode, FIFO);
	assert_eq!(extensions, names(&["VK_KHR_swapchain"]));
	assert!(!feature);
}

#[test]
fn a_resized_window_keeps_the_mode() {
	let mut window = open(offering(vec![KHR], vec![FIFO, FIFO_LATEST_READY]), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	draw(&mut window, [SIZE[0] + 8, SIZE[1]]).unwrap();
	let facts = window.facts().swapchain.unwrap();
	assert_eq!(mode_name(facts.mode), "FIFO_LATEST_READY");
	let driver = close(window);
	let modes: Vec<_> = driver.swapchains.iter().map(|made| made.mode).collect();
	assert_eq!(modes, [FIFO_LATEST_READY; 2]);
}

#[test]
fn the_fake_driver_reports_the_feature_only_with_an_extension_defining_it() {
	// Were it to say yes regardless, the tests above would prove nothing.
	let cases = [
		(vec![KHR], vec![], true),
		(vec![EXT], vec![], true),
		(vec![], vec![], false),
		(vec!["VK_KHR_incremental_present"], vec![], false),
		(vec![KHR, EXT], vec![FIFO_LATEST_READY_FEATURE], false),
	];
	for (extensions, lacked, has) in cases {
		let mut driver = offering(extensions.clone(), vec![FIFO]);
		driver.lacked_features = lacked;
		let instance = install(driver).create_instance(c"poopterm").unwrap();
		let physical = instance.physical_devices().unwrap()[0];
		let answer = instance.has_feature(physical, FIFO_LATEST_READY_FEATURE);
		assert_eq!(answer, has, "{extensions:?}");
		drop(instance);
		finish();
	}
}

#[test]
fn the_fake_driver_refuses_the_feature_without_its_extension_or_support() {
	let open = |driver: Driver, optional: Optional| {
		let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
		let physical = instance.physical_devices().unwrap()[0];
		Device::with_optional(instance, physical, 0, optional).err()
	};
	let [khr, _] = super::optional::FIFO_LATEST_READY;
	let on = Optional {
		fifo_latest_ready: Some(khr),
		..Optional::default()
	};
	// The device lacks the feature: refused, as drivers do.
	let mut driver = offering(vec![KHR], vec![FIFO]);
	driver.lacked_features = vec![FIFO_LATEST_READY_FEATURE];
	assert_eq!(open(driver, on), Some(Error::Failed(-8)));
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(invalid, ["vkCreateDevice: a feature the device lacks"]);
	finish();
	// The feature chained without its extension breaks a rule.
	let instance = install(offering(vec![KHR], vec![FIFO]))
		.create_instance(c"poopterm")
		.unwrap();
	let physical = instance.physical_devices().unwrap()[0];
	let swapchain = [c"VK_KHR_swapchain"];
	let made = instance.create_device(physical, 0, &swapchain, &[FIFO_LATEST_READY_FEATURE]);
	assert!(made.is_ok());
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(
		invalid,
		["vkCreateDevice: 1000361000 without its extension"]
	);
	// A device it made must be destroyed before the instance is.
	with(|driver| driver.live_devices.clear());
	drop(instance);
	finish();
}

#[test]
fn the_fake_driver_refuses_the_mode_on_a_device_without_the_feature() {
	let instance = window_instance(
		install(offering(vec![], vec![FIFO_LATEST_READY, FIFO])),
		Platform::Wayland,
	);
	let physical = instance.physical_devices().unwrap()[0];
	let device = Rc::new(Device::for_windows(instance.clone(), physical, 0).unwrap());
	let made = Rc::new(surface(&instance, WAYLAND).unwrap());
	// Asked to plan as though the device were able to.
	let plan = made.plan(physical, [64, 48], true).unwrap();
	assert_eq!(plan.mode, FIFO_LATEST_READY);
	let swapchain = Swapchain::new(&device, &made, plan).unwrap();
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	let rule = "vkCreateSwapchainKHR: FIFO_LATEST_READY with its feature on";
	assert_eq!(invalid, [rule]);
	drop((swapchain, device, made, instance));
	finish();
}

#[test]
fn the_diagnostics_page_names_the_mode() {
	assert_eq!(mode_name(FIFO_LATEST_READY), "FIFO_LATEST_READY");
	assert_eq!(mode_name(FIFO), "FIFO");
	assert_eq!(mode_name(1_000_361_001), "mode 1000361001");
}
