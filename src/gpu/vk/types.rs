//! The Vulkan types the renderer passes to the driver, laid out as the
//! specification's C declarations, and the types of its functions.
use super::memory::MemoryType;
use std::ffi::{c_char, c_void};

/// A `VkInstance`.
pub(crate) type RawInstance = *mut c_void;
/// A `VkPhysicalDevice`.
pub(crate) type RawPhysicalDevice = *mut c_void;
/// A `VkDevice`.
pub(crate) type RawDevice = *mut c_void;
/// A `VkQueue`.
pub(crate) type RawQueue = *mut c_void;

/// `VkStructureType` values.
pub(crate) const STRUCTURE_TYPE_APPLICATION_INFO: i32 = 0;
pub(crate) const STRUCTURE_TYPE_INSTANCE_CREATE_INFO: i32 = 1;
pub(crate) const STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO: i32 = 2;
pub(crate) const STRUCTURE_TYPE_DEVICE_CREATE_INFO: i32 = 3;
pub(crate) const STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES: i32 = 51;
pub(crate) const STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES: i32 = 53;
pub(crate) const STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2: i32 = 1_000_059_000;

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

/// `VkPhysicalDeviceProperties`, with the limits and sparse properties
/// kept opaque until the renderer reads one.
#[repr(C)]
pub(crate) struct PhysicalDeviceProperties {
	pub api_version: u32,
	pub driver_version: u32,
	pub vendor_id: u32,
	pub device_id: u32,
	/// `VkPhysicalDeviceType`.
	pub device_type: i32,
	pub device_name: [c_char; 256],
	pub pipeline_cache_uuid: [u8; 16],
	/// `VkPhysicalDeviceLimits`: 504 bytes, aligned for its 64-bit sizes.
	pub limits: [u64; 63],
	/// `VkPhysicalDeviceSparseProperties`.
	pub sparse: [u32; 5],
}

impl Default for PhysicalDeviceProperties {
	fn default() -> Self {
		Self {
			api_version: 0,
			driver_version: 0,
			vendor_id: 0,
			device_id: 0,
			device_type: 0,
			device_name: [0; 256],
			pipeline_cache_uuid: [0; 16],
			limits: [0; 63],
			sparse: [0; 5],
		}
	}
}

/// `VkQueueFamilyProperties`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct QueueFamilyProperties {
	pub queue_flags: u32,
	pub queue_count: u32,
	pub timestamp_valid_bits: u32,
	pub min_image_transfer_granularity: [u32; 3],
}

/// `VkExtensionProperties`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct ExtensionProperties {
	pub extension_name: [c_char; 256],
	pub spec_version: u32,
}

/// `VkMemoryHeap`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct MemoryHeap {
	pub size: u64,
	pub flags: u32,
}

/// `VkPhysicalDeviceMemoryProperties`.
#[repr(C)]
#[derive(Default)]
pub(crate) struct MemoryProperties {
	pub type_count: u32,
	pub types: [MemoryType; 32],
	pub heap_count: u32,
	pub heaps: [MemoryHeap; 16],
}

/// `VkPhysicalDeviceFeatures2`, with the 1.0 features as their 55
/// `VkBool32`s in declaration order.
#[repr(C)]
pub(crate) struct PhysicalDeviceFeatures2 {
	pub s_type: i32,
	pub next: *mut c_void,
	pub features: [u32; 55],
}

/// `VkPhysicalDeviceVulkan12Features`: its 47 `VkBool32`s in declaration
/// order.
#[repr(C)]
pub(crate) struct Vulkan12Features {
	pub s_type: i32,
	pub next: *mut c_void,
	pub flags: [u32; 47],
}

/// `VkPhysicalDeviceVulkan13Features`: its 15 `VkBool32`s in declaration
/// order.
#[repr(C)]
pub(crate) struct Vulkan13Features {
	pub s_type: i32,
	pub next: *mut c_void,
	pub flags: [u32; 15],
}

/// `VkDeviceQueueCreateInfo`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct DeviceQueueCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub queue_family_index: u32,
	pub queue_count: u32,
	pub queue_priorities: *const f32,
}

/// `VkDeviceCreateInfo`.
#[repr(C)]
pub(crate) struct DeviceCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub queue_create_info_count: u32,
	pub queue_create_infos: *const DeviceQueueCreateInfo,
	pub enabled_layer_count: u32,
	pub enabled_layer_names: *const *const c_char,
	pub enabled_extension_count: u32,
	pub enabled_extension_names: *const *const c_char,
	/// `VkPhysicalDeviceFeatures`; null, as the features are chained.
	pub enabled_features: *const c_void,
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
pub(crate) type GetPhysicalDeviceProperties =
	unsafe extern "system" fn(RawPhysicalDevice, *mut PhysicalDeviceProperties);
pub(crate) type GetPhysicalDeviceQueueFamilyProperties =
	unsafe extern "system" fn(RawPhysicalDevice, *mut u32, *mut QueueFamilyProperties);
pub(crate) type EnumerateDeviceExtensionProperties = unsafe extern "system" fn(
	RawPhysicalDevice,
	*const c_char,
	*mut u32,
	*mut ExtensionProperties,
) -> i32;
pub(crate) type GetPhysicalDeviceMemoryProperties =
	unsafe extern "system" fn(RawPhysicalDevice, *mut MemoryProperties);
pub(crate) type GetPhysicalDeviceFeatures2 =
	unsafe extern "system" fn(RawPhysicalDevice, *mut PhysicalDeviceFeatures2);
pub(crate) type CreateDevice = unsafe extern "system" fn(
	RawPhysicalDevice,
	*const DeviceCreateInfo,
	*const c_void,
	*mut RawDevice,
) -> i32;
pub(crate) type GetDeviceProcAddr =
	unsafe extern "system" fn(RawDevice, *const c_char) -> Option<VoidFunction>;
pub(crate) type DestroyDevice = unsafe extern "system" fn(RawDevice, *const c_void);
pub(crate) type GetDeviceQueue = unsafe extern "system" fn(RawDevice, u32, u32, *mut RawQueue);
