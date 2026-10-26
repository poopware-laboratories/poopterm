//! The Vulkan types for window surfaces (`VK_KHR_surface` and the
//! Wayland, Xlib and headless extensions that make them), laid out as the
//! specification's C declarations, and the types of their functions.
use super::types::{ExtensionProperties, RawInstance, RawPhysicalDevice};
use std::ffi::{c_char, c_ulong, c_void};

pub(crate) type RawSurface = u64;

pub(crate) const STRUCTURE_TYPE_XLIB_SURFACE_CREATE_INFO: i32 = 1_000_004_000;
pub(crate) const STRUCTURE_TYPE_WAYLAND_SURFACE_CREATE_INFO: i32 = 1_000_006_000;
pub(crate) const STRUCTURE_TYPE_HEADLESS_SURFACE_CREATE_INFO: i32 = 1_000_256_000;

/// `VkWaylandSurfaceCreateInfoKHR`.
#[repr(C)]
pub(crate) struct WaylandSurfaceCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	/// A `wl_display`.
	pub display: *mut c_void,
	/// A `wl_surface`.
	pub surface: *mut c_void,
}

/// `VkXlibSurfaceCreateInfoKHR`.
#[repr(C)]
pub(crate) struct XlibSurfaceCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	/// An Xlib `Display`.
	pub display: *mut c_void,
	/// An Xlib `Window`.
	pub window: c_ulong,
}

/// `VkHeadlessSurfaceCreateInfoEXT`.
#[repr(C)]
pub(crate) struct HeadlessSurfaceCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
}

pub(crate) type EnumerateInstanceExtensionProperties =
	unsafe extern "system" fn(*const c_char, *mut u32, *mut ExtensionProperties) -> i32;
/// `vkCreateWaylandSurfaceKHR`, `vkCreateXlibSurfaceKHR` and
/// `vkCreateHeadlessSurfaceEXT`, which differ only in the type their
/// create info points to.
pub(crate) type CreateSurface =
	unsafe extern "system" fn(RawInstance, *const c_void, *const c_void, *mut RawSurface) -> i32;
pub(crate) type DestroySurface = unsafe extern "system" fn(RawInstance, RawSurface, *const c_void);
pub(crate) type GetPhysicalDeviceSurfaceSupport =
	unsafe extern "system" fn(RawPhysicalDevice, u32, RawSurface, *mut u32) -> i32;
