//! The layouts of our Vulkan structures against the specification's C
//! declarations on 64-bit targets: size, alignment and every field's offset.
use super::types::{ApplicationInfo, InstanceCreateInfo};
use std::mem::{align_of, offset_of, size_of};

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
