//! The layouts of the descriptor pool and set structures against the
//! specification's C declarations on 64-bit targets: size, alignment and
//! every field's offset.
use super::descriptor_types::*;
use std::mem::{align_of, offset_of, size_of};

#[test]
fn descriptor_pool_size_is_laid_out_as_vk_descriptor_pool_size() {
	// Read as an array with a stride of 8.
	let size = (
		size_of::<DescriptorPoolSize>(),
		align_of::<DescriptorPoolSize>(),
	);
	assert_eq!(size, (8, 4));
	let offsets = [
		offset_of!(DescriptorPoolSize, descriptor_type),
		offset_of!(DescriptorPoolSize, count),
	];
	assert_eq!(offsets, [0, 4]);
}

#[test]
fn descriptor_pool_create_info_is_laid_out_as_vk_descriptor_pool_create_info() {
	let size = (
		size_of::<DescriptorPoolCreateInfo>(),
		align_of::<DescriptorPoolCreateInfo>(),
	);
	assert_eq!(size, (40, 8));
	let offsets = [
		offset_of!(DescriptorPoolCreateInfo, s_type),
		offset_of!(DescriptorPoolCreateInfo, next),
		offset_of!(DescriptorPoolCreateInfo, flags),
		offset_of!(DescriptorPoolCreateInfo, max_sets),
		offset_of!(DescriptorPoolCreateInfo, pool_size_count),
		offset_of!(DescriptorPoolCreateInfo, pool_sizes),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 32]);
}

#[test]
fn descriptor_set_allocate_info_is_laid_out_as_vk_descriptor_set_allocate_info() {
	let size = (
		size_of::<DescriptorSetAllocateInfo>(),
		align_of::<DescriptorSetAllocateInfo>(),
	);
	assert_eq!(size, (40, 8));
	let offsets = [
		offset_of!(DescriptorSetAllocateInfo, s_type),
		offset_of!(DescriptorSetAllocateInfo, next),
		offset_of!(DescriptorSetAllocateInfo, pool),
		offset_of!(DescriptorSetAllocateInfo, set_count),
		offset_of!(DescriptorSetAllocateInfo, set_layouts),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32]);
}
