//! The surface's function table against the instance's lookups.
use super::super::{
	driver_tests::{install, with_devices},
	surface::{Platform, Window},
	surface_driver_tests::{surface, window_instance},
};
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
	let instance = window_instance(install(with_devices(&[])), Platform::Headless);
	let made = surface(&instance, Window::Headless).unwrap();
	let lookup = |name: &'static CStr| address(instance.function(name).ok());
	let fields: [(&CStr, usize, usize); 5] = [
		(
			c"vkDestroySurfaceKHR",
			made.functions.destroy as usize,
			lookup(c"vkDestroySurfaceKHR"),
		),
		(
			c"vkGetPhysicalDeviceSurfaceSupportKHR",
			made.functions.support as usize,
			lookup(c"vkGetPhysicalDeviceSurfaceSupportKHR"),
		),
		(
			c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR",
			made.functions.capabilities as usize,
			lookup(c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR"),
		),
		(
			c"vkGetPhysicalDeviceSurfaceFormatsKHR",
			made.functions.formats as usize,
			lookup(c"vkGetPhysicalDeviceSurfaceFormatsKHR"),
		),
		(
			c"vkGetPhysicalDeviceSurfacePresentModesKHR",
			made.functions.modes as usize,
			lookup(c"vkGetPhysicalDeviceSurfacePresentModesKHR"),
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
