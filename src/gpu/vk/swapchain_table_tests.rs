//! The swapchain's function table against the device's lookups.
use super::super::present_tests::{showing, window};
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
	let made = window(showing());
	let device = &made.device;
	let lookup =
		|name: &'static CStr| address(device.instance.device_function(device.handle, name).ok());
	let fields: [(&CStr, usize, usize); 6] = [
		(
			c"vkCreateSwapchainKHR",
			made.swapchain.functions.create as usize,
			lookup(c"vkCreateSwapchainKHR"),
		),
		(
			c"vkDestroySwapchainKHR",
			made.swapchain.functions.destroy as usize,
			lookup(c"vkDestroySwapchainKHR"),
		),
		(
			c"vkGetSwapchainImagesKHR",
			made.swapchain.functions.images as usize,
			lookup(c"vkGetSwapchainImagesKHR"),
		),
		(
			c"vkAcquireNextImageKHR",
			made.swapchain.functions.acquire as usize,
			lookup(c"vkAcquireNextImageKHR"),
		),
		(
			c"vkQueuePresentKHR",
			made.swapchain.functions.present as usize,
			lookup(c"vkQueuePresentKHR"),
		),
		(
			c"vkQueueWaitIdle",
			made.swapchain.functions.wait_idle as usize,
			lookup(c"vkQueueWaitIdle"),
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
