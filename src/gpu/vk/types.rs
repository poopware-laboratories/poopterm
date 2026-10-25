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
/// A `VkBuffer`, `VkDeviceMemory`, `VkImage` and `VkImageView`: handles
/// that are 64 bits on every platform.
pub(crate) type RawBuffer = u64;
pub(crate) type RawMemory = u64;
pub(crate) type RawImage = u64;
pub(crate) type RawImageView = u64;

/// `VK_WHOLE_SIZE`.
pub(crate) const WHOLE_SIZE: u64 = !0;

/// `VkStructureType` values.
pub(crate) const STRUCTURE_TYPE_APPLICATION_INFO: i32 = 0;
pub(crate) const STRUCTURE_TYPE_INSTANCE_CREATE_INFO: i32 = 1;
pub(crate) const STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO: i32 = 2;
pub(crate) const STRUCTURE_TYPE_DEVICE_CREATE_INFO: i32 = 3;
pub(crate) const STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO: i32 = 5;
pub(crate) const STRUCTURE_TYPE_BUFFER_CREATE_INFO: i32 = 12;
pub(crate) const STRUCTURE_TYPE_IMAGE_CREATE_INFO: i32 = 14;
pub(crate) const STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO: i32 = 15;
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
	pub limits: Limits,
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
			limits: Limits([0; 126]),
			sparse: [0; 5],
		}
	}
}

/// `VkPhysicalDeviceLimits` as its 126 four-byte words, aligned for the
/// 64-bit sizes among them; the renderer reads a limit by its index.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub(crate) struct Limits(pub [u32; 126]);

/// The indices of `maxImageDimension2D` and `maxImageArrayLayers`.
pub(crate) const MAX_IMAGE_DIMENSION_2D: usize = 1;
pub(crate) const MAX_IMAGE_ARRAY_LAYERS: usize = 4;

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

/// `VkBufferCreateInfo`.
#[repr(C)]
pub(crate) struct BufferCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub size: u64,
	/// `VkBufferUsageFlags`.
	pub usage: u32,
	/// `VkSharingMode`: 0, exclusive to one queue family.
	pub sharing_mode: i32,
	pub queue_family_index_count: u32,
	pub queue_family_indices: *const u32,
}

/// `VkMemoryRequirements`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct MemoryRequirements {
	pub size: u64,
	pub alignment: u64,
	pub memory_type_bits: u32,
}

/// `VkMemoryAllocateInfo`.
#[repr(C)]
pub(crate) struct MemoryAllocateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub allocation_size: u64,
	pub memory_type_index: u32,
}

/// `VkImageCreateInfo`.
#[repr(C)]
pub(crate) struct ImageCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	/// `VkImageType`: 1, two-dimensional.
	pub image_type: i32,
	/// `VkFormat`.
	pub format: i32,
	/// `VkExtent3D`.
	pub extent: [u32; 3],
	pub mip_levels: u32,
	pub array_layers: u32,
	/// `VkSampleCountFlagBits`.
	pub samples: u32,
	/// `VkImageTiling`: 0, optimal for the GPU.
	pub tiling: i32,
	/// `VkImageUsageFlags`.
	pub usage: u32,
	pub sharing_mode: i32,
	pub queue_family_index_count: u32,
	pub queue_family_indices: *const u32,
	/// `VkImageLayout`: 0, undefined.
	pub initial_layout: i32,
}

/// `VkImageSubresourceRange`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SubresourceRange {
	/// `VkImageAspectFlags`: 1, color.
	pub aspect_mask: u32,
	pub base_mip_level: u32,
	pub level_count: u32,
	pub base_array_layer: u32,
	pub layer_count: u32,
}

/// `VkImageViewCreateInfo`.
#[repr(C)]
pub(crate) struct ImageViewCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub image: RawImage,
	/// `VkImageViewType`.
	pub view_type: i32,
	pub format: i32,
	/// `VkComponentMapping`: 0 for each, the identity.
	pub components: [i32; 4],
	pub subresource_range: SubresourceRange,
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
pub(crate) type CreateBuffer = unsafe extern "system" fn(
	RawDevice,
	*const BufferCreateInfo,
	*const c_void,
	*mut RawBuffer,
) -> i32;
pub(crate) type DestroyBuffer = unsafe extern "system" fn(RawDevice, RawBuffer, *const c_void);
pub(crate) type GetBufferMemoryRequirements =
	unsafe extern "system" fn(RawDevice, RawBuffer, *mut MemoryRequirements);
pub(crate) type AllocateMemory = unsafe extern "system" fn(
	RawDevice,
	*const MemoryAllocateInfo,
	*const c_void,
	*mut RawMemory,
) -> i32;
pub(crate) type FreeMemory = unsafe extern "system" fn(RawDevice, RawMemory, *const c_void);
pub(crate) type BindBufferMemory =
	unsafe extern "system" fn(RawDevice, RawBuffer, RawMemory, u64) -> i32;
pub(crate) type MapMemory =
	unsafe extern "system" fn(RawDevice, RawMemory, u64, u64, u32, *mut *mut c_void) -> i32;
pub(crate) type CreateImage =
	unsafe extern "system" fn(RawDevice, *const ImageCreateInfo, *const c_void, *mut RawImage) -> i32;
pub(crate) type DestroyImage = unsafe extern "system" fn(RawDevice, RawImage, *const c_void);
pub(crate) type GetImageMemoryRequirements =
	unsafe extern "system" fn(RawDevice, RawImage, *mut MemoryRequirements);
pub(crate) type BindImageMemory =
	unsafe extern "system" fn(RawDevice, RawImage, RawMemory, u64) -> i32;
pub(crate) type CreateImageView = unsafe extern "system" fn(
	RawDevice,
	*const ImageViewCreateInfo,
	*const c_void,
	*mut RawImageView,
) -> i32;
pub(crate) type DestroyImageView =
	unsafe extern "system" fn(RawDevice, RawImageView, *const c_void);
