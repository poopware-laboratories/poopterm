//! The layouts of our Vulkan structures against the specification's C
//! declarations on 64-bit targets: size, alignment and every field's offset.
use super::{
	memory::MemoryType,
	types::{
		ApplicationInfo, BufferCreateInfo, DeviceCreateInfo, DeviceQueueCreateInfo,
		ExtensionProperties, ImageCreateInfo, ImageViewCreateInfo, InstanceCreateInfo, Limits,
		MemoryAllocateInfo, MemoryHeap, MemoryProperties, MemoryRequirements, PhysicalDeviceFeatures2,
		PhysicalDeviceProperties, QueueFamilyProperties, SemaphoreCreateInfo, SemaphoreTypeCreateInfo,
		SemaphoreWaitInfo, SubresourceRange, Vulkan12Features, Vulkan13Features,
	},
};
use std::mem::{align_of, offset_of, size_of, size_of_val};

#[test]
fn application_info_is_laid_out_as_vk_application_info() {
	assert_eq!(
		(size_of::<ApplicationInfo>(), align_of::<ApplicationInfo>()),
		(48, 8)
	);
	let offsets = [
		offset_of!(ApplicationInfo, s_type),
		offset_of!(ApplicationInfo, next),
		offset_of!(ApplicationInfo, application_name),
		offset_of!(ApplicationInfo, application_version),
		offset_of!(ApplicationInfo, engine_name),
		offset_of!(ApplicationInfo, engine_version),
		offset_of!(ApplicationInfo, api_version),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32, 40, 44]);
}

#[test]
fn instance_create_info_is_laid_out_as_vk_instance_create_info() {
	assert_eq!(
		(
			size_of::<InstanceCreateInfo>(),
			align_of::<InstanceCreateInfo>()
		),
		(64, 8)
	);
	let offsets = [
		offset_of!(InstanceCreateInfo, s_type),
		offset_of!(InstanceCreateInfo, next),
		offset_of!(InstanceCreateInfo, flags),
		offset_of!(InstanceCreateInfo, application_info),
		offset_of!(InstanceCreateInfo, enabled_layer_count),
		offset_of!(InstanceCreateInfo, enabled_layer_names),
		offset_of!(InstanceCreateInfo, enabled_extension_count),
		offset_of!(InstanceCreateInfo, enabled_extension_names),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32, 40, 48, 56]);
}

#[test]
fn physical_device_properties_are_laid_out_as_vk_physical_device_properties() {
	// The limits hold 64-bit sizes, so they start 8-aligned at 296, after
	// four bytes of padding, and the whole is padded to 824.
	let size = (
		size_of::<PhysicalDeviceProperties>(),
		align_of::<PhysicalDeviceProperties>(),
	);
	assert_eq!(size, (824, 8));
	let offsets = [
		offset_of!(PhysicalDeviceProperties, api_version),
		offset_of!(PhysicalDeviceProperties, driver_version),
		offset_of!(PhysicalDeviceProperties, vendor_id),
		offset_of!(PhysicalDeviceProperties, device_id),
		offset_of!(PhysicalDeviceProperties, device_type),
		offset_of!(PhysicalDeviceProperties, device_name),
		offset_of!(PhysicalDeviceProperties, pipeline_cache_uuid),
		offset_of!(PhysicalDeviceProperties, limits),
		offset_of!(PhysicalDeviceProperties, sparse),
	];
	assert_eq!(offsets, [0, 4, 8, 12, 16, 20, 276, 296, 800]);
	// VkPhysicalDeviceLimits is 504 bytes, the sparse properties five
	// VkBool32s.
	let properties = PhysicalDeviceProperties::default();
	assert_eq!(
		(
			size_of_val(&properties.limits),
			size_of_val(&properties.sparse)
		),
		(504, 20)
	);
	assert_eq!((properties.api_version, properties.device_type), (0, 0));
}

#[test]
fn queue_family_and_extension_properties_are_laid_out_as_vulkan_declares_them() {
	assert_eq!(
		(
			size_of::<QueueFamilyProperties>(),
			align_of::<QueueFamilyProperties>()
		),
		(24, 4)
	);
	let offsets = [
		offset_of!(QueueFamilyProperties, queue_flags),
		offset_of!(QueueFamilyProperties, queue_count),
		offset_of!(QueueFamilyProperties, timestamp_valid_bits),
		offset_of!(QueueFamilyProperties, min_image_transfer_granularity),
	];
	assert_eq!(offsets, [0, 4, 8, 12]);
	assert_eq!(
		(
			size_of::<ExtensionProperties>(),
			align_of::<ExtensionProperties>()
		),
		(260, 4)
	);
	let offsets = [
		offset_of!(ExtensionProperties, extension_name),
		offset_of!(ExtensionProperties, spec_version),
	];
	assert_eq!(offsets, [0, 256]);
}

#[test]
fn device_create_infos_are_laid_out_as_vulkan_declares_them() {
	assert_eq!(
		(
			size_of::<DeviceQueueCreateInfo>(),
			align_of::<DeviceQueueCreateInfo>()
		),
		(40, 8)
	);
	let offsets = [
		offset_of!(DeviceQueueCreateInfo, s_type),
		offset_of!(DeviceQueueCreateInfo, next),
		offset_of!(DeviceQueueCreateInfo, flags),
		offset_of!(DeviceQueueCreateInfo, queue_family_index),
		offset_of!(DeviceQueueCreateInfo, queue_count),
		offset_of!(DeviceQueueCreateInfo, queue_priorities),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 32]);
	assert_eq!(
		(
			size_of::<DeviceCreateInfo>(),
			align_of::<DeviceCreateInfo>()
		),
		(72, 8)
	);
	let offsets = [
		offset_of!(DeviceCreateInfo, s_type),
		offset_of!(DeviceCreateInfo, next),
		offset_of!(DeviceCreateInfo, flags),
		offset_of!(DeviceCreateInfo, queue_create_info_count),
		offset_of!(DeviceCreateInfo, queue_create_infos),
		offset_of!(DeviceCreateInfo, enabled_layer_count),
		offset_of!(DeviceCreateInfo, enabled_layer_names),
		offset_of!(DeviceCreateInfo, enabled_extension_count),
		offset_of!(DeviceCreateInfo, enabled_extension_names),
		offset_of!(DeviceCreateInfo, enabled_features),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 32, 40, 48, 56, 64]);
}

#[test]
fn feature_structures_are_laid_out_as_vulkan_declares_them() {
	// Each is a type, a pointer and its VkBool32s, padded to 8: 55 in
	// VkPhysicalDeviceFeatures, 47 for 1.2 and 15 for 1.3.
	let sizes = [
		(
			size_of::<PhysicalDeviceFeatures2>(),
			align_of::<PhysicalDeviceFeatures2>(),
		),
		(
			size_of::<Vulkan12Features>(),
			align_of::<Vulkan12Features>(),
		),
		(
			size_of::<Vulkan13Features>(),
			align_of::<Vulkan13Features>(),
		),
	];
	assert_eq!(sizes, [(240, 8), (208, 8), (80, 8)]);
	let offsets = [
		offset_of!(PhysicalDeviceFeatures2, next),
		offset_of!(PhysicalDeviceFeatures2, features),
		offset_of!(Vulkan12Features, next),
		offset_of!(Vulkan12Features, flags),
		offset_of!(Vulkan13Features, next),
		offset_of!(Vulkan13Features, flags),
	];
	assert_eq!(offsets, [8, 16, 8, 16, 8, 16]);
}

#[test]
fn memory_properties_are_laid_out_as_vk_physical_device_memory_properties() {
	// 32 VkMemoryTypes of two u32s, then 16 VkMemoryHeaps of a u64 size and
	// u32 flags, padded to 16 and starting 8-aligned at 264.
	assert_eq!((size_of::<MemoryType>(), align_of::<MemoryType>()), (8, 4));
	assert_eq!(
		(offset_of!(MemoryType, flags), offset_of!(MemoryType, heap)),
		(0, 4)
	);
	assert_eq!((size_of::<MemoryHeap>(), align_of::<MemoryHeap>()), (16, 8));
	assert_eq!(offset_of!(MemoryHeap, flags), 8);
	assert_eq!(
		(
			size_of::<MemoryProperties>(),
			align_of::<MemoryProperties>()
		),
		(520, 8)
	);
	let offsets = [
		offset_of!(MemoryProperties, type_count),
		offset_of!(MemoryProperties, types),
		offset_of!(MemoryProperties, heap_count),
		offset_of!(MemoryProperties, heaps),
	];
	assert_eq!(offsets, [0, 4, 260, 264]);
}

#[test]
fn buffer_and_memory_structures_are_laid_out_as_vulkan_declares_them() {
	// VkDeviceSize is a u64, so the size starts 8-aligned at 24.
	assert_eq!(
		(
			size_of::<BufferCreateInfo>(),
			align_of::<BufferCreateInfo>()
		),
		(56, 8)
	);
	let offsets = [
		offset_of!(BufferCreateInfo, s_type),
		offset_of!(BufferCreateInfo, next),
		offset_of!(BufferCreateInfo, flags),
		offset_of!(BufferCreateInfo, size),
		offset_of!(BufferCreateInfo, usage),
		offset_of!(BufferCreateInfo, sharing_mode),
		offset_of!(BufferCreateInfo, queue_family_index_count),
		offset_of!(BufferCreateInfo, queue_family_indices),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32, 36, 40, 48]);
	assert_eq!(
		(
			size_of::<MemoryRequirements>(),
			align_of::<MemoryRequirements>()
		),
		(24, 8)
	);
	let offsets = [
		offset_of!(MemoryRequirements, size),
		offset_of!(MemoryRequirements, alignment),
		offset_of!(MemoryRequirements, memory_type_bits),
	];
	assert_eq!(offsets, [0, 8, 16]);
	assert_eq!(
		(
			size_of::<MemoryAllocateInfo>(),
			align_of::<MemoryAllocateInfo>()
		),
		(32, 8)
	);
	let offsets = [
		offset_of!(MemoryAllocateInfo, s_type),
		offset_of!(MemoryAllocateInfo, next),
		offset_of!(MemoryAllocateInfo, allocation_size),
		offset_of!(MemoryAllocateInfo, memory_type_index),
	];
	assert_eq!(offsets, [0, 8, 16, 24]);
}

#[test]
fn image_structures_are_laid_out_as_vulkan_declares_them() {
	assert_eq!(
		(size_of::<ImageCreateInfo>(), align_of::<ImageCreateInfo>()),
		(88, 8)
	);
	let offsets = [
		offset_of!(ImageCreateInfo, s_type),
		offset_of!(ImageCreateInfo, next),
		offset_of!(ImageCreateInfo, flags),
		offset_of!(ImageCreateInfo, image_type),
		offset_of!(ImageCreateInfo, format),
		offset_of!(ImageCreateInfo, extent),
		offset_of!(ImageCreateInfo, mip_levels),
		offset_of!(ImageCreateInfo, array_layers),
		offset_of!(ImageCreateInfo, samples),
		offset_of!(ImageCreateInfo, tiling),
		offset_of!(ImageCreateInfo, usage),
		offset_of!(ImageCreateInfo, sharing_mode),
		offset_of!(ImageCreateInfo, queue_family_index_count),
		offset_of!(ImageCreateInfo, queue_family_indices),
		offset_of!(ImageCreateInfo, initial_layout),
	];
	assert_eq!(
		offsets,
		[0, 8, 16, 20, 24, 28, 40, 44, 48, 52, 56, 60, 64, 72, 80]
	);
	assert_eq!(
		(
			size_of::<SubresourceRange>(),
			align_of::<SubresourceRange>()
		),
		(20, 4)
	);
	assert_eq!(
		(
			size_of::<ImageViewCreateInfo>(),
			align_of::<ImageViewCreateInfo>()
		),
		(80, 8)
	);
	let offsets = [
		offset_of!(ImageViewCreateInfo, s_type),
		offset_of!(ImageViewCreateInfo, next),
		offset_of!(ImageViewCreateInfo, flags),
		offset_of!(ImageViewCreateInfo, image),
		offset_of!(ImageViewCreateInfo, view_type),
		offset_of!(ImageViewCreateInfo, format),
		offset_of!(ImageViewCreateInfo, components),
		offset_of!(ImageViewCreateInfo, subresource_range),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32, 36, 40, 56]);
}

#[test]
fn limits_are_vk_physical_device_limits_as_words() {
	// 126 four-byte words, 8-aligned for the VkDeviceSize limits among them.
	assert_eq!((size_of::<Limits>(), align_of::<Limits>()), (504, 8));
}

#[test]
fn semaphore_structures_are_laid_out_as_vulkan_declares_them() {
	let create = (
		size_of::<SemaphoreCreateInfo>(),
		align_of::<SemaphoreCreateInfo>(),
	);
	assert_eq!(create, (24, 8));
	assert_eq!(offset_of!(SemaphoreCreateInfo, flags), 16);
	let kind = (
		size_of::<SemaphoreTypeCreateInfo>(),
		align_of::<SemaphoreTypeCreateInfo>(),
	);
	assert_eq!(kind, (32, 8));
	let offsets = [
		offset_of!(SemaphoreTypeCreateInfo, next),
		offset_of!(SemaphoreTypeCreateInfo, semaphore_type),
		offset_of!(SemaphoreTypeCreateInfo, initial_value),
	];
	assert_eq!(offsets, [8, 16, 24]);
	let wait = (
		size_of::<SemaphoreWaitInfo>(),
		align_of::<SemaphoreWaitInfo>(),
	);
	assert_eq!(wait, (40, 8));
	let offsets = [
		offset_of!(SemaphoreWaitInfo, flags),
		offset_of!(SemaphoreWaitInfo, semaphore_count),
		offset_of!(SemaphoreWaitInfo, semaphores),
		offset_of!(SemaphoreWaitInfo, values),
	];
	assert_eq!(offsets, [16, 20, 24, 32]);
}
