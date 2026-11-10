//! Tests of the driver's facts: the structures laid out as the
//! specification declares them, the text taken from them, and the query
//! against the fake driver, with and without the function.
use super::{
	choose::{Device, GRAPHICS, Kind, QueueFamily},
	driver_info::*,
	driver_tests::{finish, install, with_devices},
	types::PhysicalDeviceProperties,
	version::api_version,
};
use std::{
	ffi::c_char,
	mem::{align_of, offset_of, size_of},
};

#[test]
fn driver_properties_are_laid_out_as_vulkan_declares_them() {
	assert_eq!(
		(
			size_of::<DriverProperties>(),
			align_of::<DriverProperties>()
		),
		(536, 8)
	);
	assert_eq!(
		[
			offset_of!(DriverProperties, s_type),
			offset_of!(DriverProperties, next),
			offset_of!(DriverProperties, driver_id),
			offset_of!(DriverProperties, driver_name),
			offset_of!(DriverProperties, driver_info),
			offset_of!(DriverProperties, conformance),
		],
		[0, 8, 16, 20, 276, 532]
	);
}

#[test]
fn properties2_wrap_the_properties_after_the_chain_header() {
	assert_eq!(offset_of!(PhysicalDeviceProperties2, s_type), 0);
	assert_eq!(offset_of!(PhysicalDeviceProperties2, next), 8);
	assert_eq!(offset_of!(PhysicalDeviceProperties2, properties), 16);
	assert_eq!(
		size_of::<PhysicalDeviceProperties2>(),
		16 + size_of::<PhysicalDeviceProperties>()
	);
	// VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2 and _DRIVER_PROPERTIES.
	assert_eq!(STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2, 1_000_059_001);
	assert_eq!(
		STRUCTURE_TYPE_PHYSICAL_DEVICE_DRIVER_PROPERTIES,
		1_000_196_000
	);
}

/// `text` in a 256-byte C string field.
fn field(text: &str) -> [c_char; 256] {
	let mut field = [0; 256];
	for (out, &byte) in field[..255].iter_mut().zip(text.as_bytes()) {
		*out = byte as c_char;
	}
	field
}

/// Driver properties naming `name` with `info`.
fn said(name: &str, info: &str) -> Option<String> {
	let mut properties = DriverProperties::empty();
	properties.driver_name = field(name);
	properties.driver_info = field(info);
	properties.text()
}

#[test]
fn the_info_wins_as_it_carries_the_version() {
	assert_eq!(said("radv", "Mesa 26.2.1").as_deref(), Some("Mesa 26.2.1"));
	assert_eq!(said("radv", "").as_deref(), Some("radv"));
	assert_eq!(said("radv", "   ").as_deref(), Some("radv"));
	assert_eq!(said("", ""), None);
	// Padding a driver adds is not shown.
	assert_eq!(said("", " 570.86.16 ").as_deref(), Some("570.86.16"));
}

#[test]
fn empty_properties_say_what_they_are_and_nothing_else() {
	let properties = DriverProperties::empty();
	assert_eq!(
		properties.s_type,
		STRUCTURE_TYPE_PHYSICAL_DEVICE_DRIVER_PROPERTIES
	);
	assert!(properties.next.is_null());
	assert_eq!(properties.text(), None);
}

fn one_gpu() -> Device {
	Device {
		name: "Intel(R) Graphics (ADL-N)".into(),
		kind: Kind::Integrated,
		api: api_version(1, 4) | 363,
		families: vec![QueueFamily {
			flags: GRAPHICS,
			count: 1,
			present: false,
		}],
		swapchain: true,
	}
}

#[test]
fn the_driver_names_itself_through_the_properties_chain() {
	let mut driver = with_devices(&[one_gpu()]);
	driver.driver_info = Some(("Intel open-source Mesa driver", "Mesa 26.2.1"));
	let instance = install(driver).create_instance(c"poopterm").unwrap();
	let handles = instance.physical_devices().unwrap();
	assert_eq!(instance.driver(handles[0]).as_deref(), Some("Mesa 26.2.1"));
	drop(instance);
	finish();
}

#[test]
fn a_driver_saying_nothing_has_no_facts() {
	let instance = install(with_devices(&[one_gpu()]))
		.create_instance(c"poopterm")
		.unwrap();
	let handles = instance.physical_devices().unwrap();
	assert_eq!(instance.driver(handles[0]), None);
	drop(instance);
	finish();
}

#[test]
fn a_loader_without_properties2_still_makes_an_instance_without_facts() {
	let mut driver = with_devices(&[one_gpu()]);
	driver.lacks.push(c"vkGetPhysicalDeviceProperties2");
	driver.driver_info = Some(("radv", "Mesa 26.2.1"));
	let instance = install(driver).create_instance(c"poopterm").unwrap();
	let handles = instance.physical_devices().unwrap();
	assert!(instance.describe(handles[0]).is_ok());
	assert_eq!(instance.driver(handles[0]), None);
	drop(instance);
	assert_eq!(finish().destroyed.len(), 1);
}
