//! The instance's function table against the loader's lookups.
use super::super::driver_tests::{install, with_devices};
use crate::gpu::vk::types::VoidFunction;
use std::ffi::CStr;

/// An address to compare functions by; 0 for none.
fn address(function: Option<VoidFunction>) -> usize {
	function.map_or(0, |function| function as usize)
}

/// Each field holds the function looked up for that field's own name: the
/// type each field casts to is right for that name alone, so a swapped name
/// would call a function through the wrong type.
#[test]
fn every_function_is_the_one_its_name_looks_up() {
	let instance = install(with_devices(&[]))
		.create_instance(c"poopterm")
		.unwrap();
	let lookup = |name: &'static CStr| address(instance.entry.function(instance.handle, name).ok());
	let fields: [(&CStr, usize, usize); 8] = [
		(
			c"vkEnumeratePhysicalDevices",
			instance.functions.enumerate_physical_devices as usize,
			lookup(c"vkEnumeratePhysicalDevices"),
		),
		(
			c"vkGetPhysicalDeviceProperties",
			instance.functions.properties as usize,
			lookup(c"vkGetPhysicalDeviceProperties"),
		),
		(
			c"vkGetPhysicalDeviceQueueFamilyProperties",
			instance.functions.queue_families as usize,
			lookup(c"vkGetPhysicalDeviceQueueFamilyProperties"),
		),
		(
			c"vkEnumerateDeviceExtensionProperties",
			instance.functions.extensions as usize,
			lookup(c"vkEnumerateDeviceExtensionProperties"),
		),
		(
			c"vkGetPhysicalDeviceMemoryProperties",
			instance.functions.memory as usize,
			lookup(c"vkGetPhysicalDeviceMemoryProperties"),
		),
		(
			c"vkGetPhysicalDeviceFeatures2",
			instance.functions.features as usize,
			lookup(c"vkGetPhysicalDeviceFeatures2"),
		),
		(
			c"vkCreateDevice",
			instance.functions.create_device as usize,
			lookup(c"vkCreateDevice"),
		),
		(
			c"vkGetDeviceProcAddr",
			instance.functions.device_function as usize,
			lookup(c"vkGetDeviceProcAddr"),
		),
	];
	for (name, field, looked_up) in fields {
		assert_ne!(field, 0, "{name:?}");
		assert_eq!(field, looked_up, "{name:?} holds another function");
	}
	let distinct: std::collections::HashSet<usize> =
		fields.iter().map(|&(_, field, _)| field).collect();
	assert_eq!(distinct.len(), fields.len(), "two fields hold one function");
}

/// The optional functions are loaded by their own names too, and are
/// `None` exactly where the driver has no function of that name.
#[test]
fn every_optional_function_is_the_one_its_name_looks_up_or_none() {
	let instance = install(with_devices(&[]))
		.create_instance(c"poopterm")
		.unwrap();
	let lookup = |name: &'static CStr| address(instance.entry.function(instance.handle, name).ok());
	let fields: [(&CStr, Option<usize>, usize); 2] = [
		(
			c"vkGetPhysicalDeviceProperties2",
			instance.functions.properties2.map(|f| f as usize),
			lookup(c"vkGetPhysicalDeviceProperties2"),
		),
		(
			c"vkGetPhysicalDeviceMemoryProperties2",
			instance.functions.memory2.map(|f| f as usize),
			lookup(c"vkGetPhysicalDeviceMemoryProperties2"),
		),
	];
	for (name, field, looked_up) in fields {
		assert_eq!(field.unwrap_or(0), looked_up, "{name:?}");
	}
}
