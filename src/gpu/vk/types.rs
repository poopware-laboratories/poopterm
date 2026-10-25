//! The Vulkan types the renderer passes to the driver, laid out as the
//! specification's C declarations, and the types of its functions.
use std::ffi::{c_char, c_void};

/// A `VkInstance`.
pub(crate) type RawInstance = *mut c_void;
/// A `VkPhysicalDevice`.
pub(crate) type RawPhysicalDevice = *mut c_void;

/// `VkStructureType` values.
pub(crate) const STRUCTURE_TYPE_APPLICATION_INFO: i32 = 0;
pub(crate) const STRUCTURE_TYPE_INSTANCE_CREATE_INFO: i32 = 1;

/// `VkApplicationInfo`.
#[repr(C)]
pub(crate) struct ApplicationInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub application_name: *const c_char,
	pub application_version: u32,
	pub engine_name: *const c_char,
	pub engine_version: u32,
	pub api_version: u32,
}

/// `VkInstanceCreateInfo`.
#[repr(C)]
pub(crate) struct InstanceCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub application_info: *const ApplicationInfo,
	pub enabled_layer_count: u32,
	pub enabled_layer_names: *const *const c_char,
	pub enabled_extension_count: u32,
	pub enabled_extension_names: *const *const c_char,
}

/// `PFN_vkVoidFunction`: what the loader returns, cast to the real type.
pub(crate) type VoidFunction = unsafe extern "system" fn();
pub(crate) type GetInstanceProcAddr =
	unsafe extern "system" fn(RawInstance, *const c_char) -> Option<VoidFunction>;
pub(crate) type EnumerateInstanceVersion = unsafe extern "system" fn(*mut u32) -> i32;
pub(crate) type CreateInstance =
	unsafe extern "system" fn(*const InstanceCreateInfo, *const c_void, *mut RawInstance) -> i32;
pub(crate) type DestroyInstance = unsafe extern "system" fn(RawInstance, *const c_void);
pub(crate) type EnumeratePhysicalDevices =
	unsafe extern "system" fn(RawInstance, *mut u32, *mut RawPhysicalDevice) -> i32;
