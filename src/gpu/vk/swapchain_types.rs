//! The Vulkan types for swapchains (`VK_KHR_swapchain`), laid out as the
//! specification's C declarations, and the types of their functions.
use super::{
	surface_types::RawSurface,
	types::{RawDevice, RawImage},
};
use std::ffi::c_void;

pub(crate) type RawSwapchain = u64;

pub(crate) const STRUCTURE_TYPE_SWAPCHAIN_CREATE_INFO: i32 = 1_000_001_000;

/// `VkSwapchainCreateInfoKHR`.
#[repr(C)]
pub(crate) struct SwapchainCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub surface: RawSurface,
	pub min_image_count: u32,
	pub image_format: i32,
	pub image_color_space: i32,
	pub image_extent: [u32; 2],
	pub image_array_layers: u32,
	pub image_usage: u32,
	/// `VkSharingMode`: 0, one queue family at a time.
	pub image_sharing_mode: i32,
	pub queue_family_index_count: u32,
	pub queue_family_indices: *const u32,
	pub pre_transform: u32,
	pub composite_alpha: u32,
	pub present_mode: i32,
	/// `VkBool32`: whether pixels hidden by other windows may be skipped.
	pub clipped: u32,
	pub old_swapchain: RawSwapchain,
}

pub(crate) type CreateSwapchain = unsafe extern "system" fn(
	RawDevice,
	*const SwapchainCreateInfo,
	*const c_void,
	*mut RawSwapchain,
) -> i32;
pub(crate) type DestroySwapchain =
	unsafe extern "system" fn(RawDevice, RawSwapchain, *const c_void);
pub(crate) type GetSwapchainImages =
	unsafe extern "system" fn(RawDevice, RawSwapchain, *mut u32, *mut RawImage) -> i32;
