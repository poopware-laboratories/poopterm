//! Tests of devices made with optional extensions against the fake driver
//! of `driver_tests`: the device's extensions listed, those found among
//! them enabled, and the variable keeping them off.
use super::{
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind, TRANSFER},
	device::Device,
	driver_tests::{Driver, family, finish, install, with, with_devices},
	instance::Instance,
	optional::Optional,
	types::RawPhysicalDevice,
	version::api_version,
};
use std::rc::Rc;

const INCREMENTAL: &str = "VK_KHR_incremental_present";

/// A device drawing on its second family, showing in windows if `swapchain`.
fn described(swapchain: bool) -> Described {
	Described {
		name: "fake".into(),
		kind: Kind::Integrated,
		api: api_version(1, 3),
		families: vec![family(TRANSFER, 1), family(GRAPHICS | TRANSFER, 2)],
		swapchain,
	}
}

/// A driver whose one device also offers `extra`.
fn offering(extra: Vec<&'static str>) -> Driver {
	let mut driver = with_devices(&[described(true)]);
	driver.device_extensions = extra;
	driver
}

fn open(driver: Driver) -> (Rc<Instance>, RawPhysicalDevice) {
	let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
	let physical = instance.physical_devices().unwrap()[0];
	(instance, physical)
}

/// The extensions each device made so far was made with.
fn made() -> Vec<Vec<String>> {
	finish()
		.made
		.into_iter()
		.map(|made| made.extensions)
		.collect()
}

#[test]
fn a_devices_extensions_are_listed_as_the_driver_offers_them() {
	let (instance, physical) = open(offering(vec![INCREMENTAL]));
	let listed = instance.device_extensions(physical).unwrap();
	let expected = [
		"VK_KHR_maintenance5",
		"VK_KHR_swapchain",
		"VK_KHR_swapchain_mutable_format",
		INCREMENTAL,
	];
	assert_eq!(listed, expected);
	drop(instance);
	finish();
	// One that shows in no window offers no swapchain.
	let (instance, physical) = open(with_devices(&[described(false)]));
	let listed = instance.device_extensions(physical).unwrap();
	assert_eq!(
		listed,
		["VK_KHR_maintenance5", "VK_KHR_swapchain_mutable_format"]
	);
	drop(instance);
	finish();
}

#[test]
fn listing_a_devices_extensions_fails_with_the_drivers_code() {
	let mut driver = offering(vec![INCREMENTAL]);
	driver.fails = vec![(c"vkEnumerateDeviceExtensionProperties", -1)];
	let (instance, physical) = open(driver);
	assert_eq!(instance.device_extensions(physical), Err(Error::Failed(-1)));
	drop(instance);
	finish();
}

#[test]
fn an_offered_optional_extension_is_enabled_after_the_swapchain() {
	let (instance, physical) = open(offering(vec![INCREMENTAL]));
	let offered = instance.device_extensions(physical).unwrap();
	let optional = Optional::found(&offered, None);
	let device = Device::with_optional(instance, physical, 1, optional).unwrap();
	assert!(device.optional.incremental_present);
	drop(device);
	assert_eq!(
		made(),
		[vec!["VK_KHR_swapchain".to_owned(), INCREMENTAL.into()]]
	);
}

#[test]
fn the_variable_keeps_an_offered_extension_off_the_device() {
	let (instance, physical) = open(offering(vec![INCREMENTAL]));
	let offered = instance.device_extensions(physical).unwrap();
	let optional = Optional::found(&offered, Some("incremental_present"));
	let device = Device::with_optional(instance, physical, 1, optional).unwrap();
	assert_eq!(device.optional, Optional::default());
	drop(device);
	assert_eq!(made(), [vec!["VK_KHR_swapchain".to_owned()]]);
}

#[test]
fn a_device_lacking_the_extension_never_has_it_asked_for() {
	let (instance, physical) = open(offering(vec![]));
	let offered = instance.device_extensions(physical).unwrap();
	let optional = Optional::found(&offered, None);
	assert_eq!(optional, Optional::default());
	let device = Device::with_optional(instance, physical, 1, optional).unwrap();
	drop(device);
	assert_eq!(made(), [vec!["VK_KHR_swapchain".to_owned()]]);
	// Asked for anyway, the driver refuses: the reason it is looked for.
	let (instance, physical) = open(offering(vec![]));
	let forced = Optional {
		incremental_present: true,
	};
	let refused = Device::with_optional(instance, physical, 1, forced).err();
	assert_eq!(refused, Some(Error::Failed(-7)));
	with(|driver| driver.invalid.clear());
	assert_eq!(made(), Vec::<Vec<String>>::new());
}
