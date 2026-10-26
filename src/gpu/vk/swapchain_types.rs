//! The Vulkan types for swapchains (`VK_KHR_swapchain`), laid out as the
//! specification's C declarations, and the types of their functions.
use super::{
	surface_types::RawSurface,
	types::{RawDevice, RawImage, RawQueue, RawSemaphore},
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

pub(crate) const STRUCTURE_TYPE_PRESENT_INFO: i32 = 1_000_001_001;

/// `VkPresentInfoKHR`.
#[repr(C)]
pub(crate) struct PresentInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub wait_semaphore_count: u32,
	pub wait_semaphores: *const RawSemaphore,
	pub swapchain_count: u32,
	pub swapchains: *const RawSwapchain,
	pub image_indices: *const u32,
	/// Each swapchain's result; null when one is enough.
	pub results: *mut i32,
}

pub(crate) type AcquireNextImage =
	unsafe extern "system" fn(RawDevice, RawSwapchain, u64, RawSemaphore, u64, *mut u32) -> i32;
pub(crate) type QueuePresent = unsafe extern "system" fn(RawQueue, *const PresentInfo) -> i32;
pub(crate) type QueueWaitIdle = unsafe extern "system" fn(RawQueue) -> i32;
