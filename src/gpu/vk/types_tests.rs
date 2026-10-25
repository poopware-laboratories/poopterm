//! The layouts of our Vulkan structures against the specification's C
//! declarations on 64-bit targets: size, alignment and every field's offset.
use super::types::{
	ApplicationInfo, ExtensionProperties, InstanceCreateInfo, PhysicalDeviceProperties,
	QueueFamilyProperties,
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
