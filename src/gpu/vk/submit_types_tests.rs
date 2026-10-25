//! The layouts of the command, barrier, copy and submit structures against
//! the specification's C declarations on 64-bit targets.
use super::submit_types::{
	BufferImageCopy, BufferMemoryBarrier2, CommandBufferAllocateInfo, CommandBufferBeginInfo,
	CommandBufferSubmitInfo, CommandPoolCreateInfo, DependencyInfo, ImageMemoryBarrier2,
	SemaphoreSubmitInfo, SubmitInfo2,
};
use std::mem::{align_of, offset_of, size_of};

#[test]
fn command_structures_are_laid_out_as_vulkan_declares_them() {
	let pool = (
		size_of::<CommandPoolCreateInfo>(),
		align_of::<CommandPoolCreateInfo>(),
	);
	assert_eq!(pool, (24, 8));
	let offsets = [
		offset_of!(CommandPoolCreateInfo, flags),
		offset_of!(CommandPoolCreateInfo, queue_family_index),
	];
	assert_eq!(offsets, [16, 20]);
	let allocate = (
		size_of::<CommandBufferAllocateInfo>(),
		align_of::<CommandBufferAllocateInfo>(),
	);
	assert_eq!(allocate, (32, 8));
	let offsets = [
		offset_of!(CommandBufferAllocateInfo, command_pool),
		offset_of!(CommandBufferAllocateInfo, level),
		offset_of!(CommandBufferAllocateInfo, command_buffer_count),
	];
	assert_eq!(offsets, [16, 24, 28]);
	let begin = (
		size_of::<CommandBufferBeginInfo>(),
		align_of::<CommandBufferBeginInfo>(),
	);
	assert_eq!(begin, (32, 8));
	let offsets = [
		offset_of!(CommandBufferBeginInfo, flags),
		offset_of!(CommandBufferBeginInfo, inheritance_info),
	];
	assert_eq!(offsets, [16, 24]);
}

#[test]
fn submit_structures_are_laid_out_as_vulkan_declares_them() {
	let semaphore = (
		size_of::<SemaphoreSubmitInfo>(),
		align_of::<SemaphoreSubmitInfo>(),
	);
	assert_eq!(semaphore, (48, 8));
	let offsets = [
		offset_of!(SemaphoreSubmitInfo, semaphore),
		offset_of!(SemaphoreSubmitInfo, value),
		offset_of!(SemaphoreSubmitInfo, stage_mask),
		offset_of!(SemaphoreSubmitInfo, device_index),
	];
	assert_eq!(offsets, [16, 24, 32, 40]);
	let buffer = (
		size_of::<CommandBufferSubmitInfo>(),
		align_of::<CommandBufferSubmitInfo>(),
	);
	assert_eq!(buffer, (32, 8));
	let offsets = [
		offset_of!(CommandBufferSubmitInfo, command_buffer),
		offset_of!(CommandBufferSubmitInfo, device_mask),
	];
	assert_eq!(offsets, [16, 24]);
	assert_eq!(
		(size_of::<SubmitInfo2>(), align_of::<SubmitInfo2>()),
		(64, 8)
	);
	let offsets = [
		offset_of!(SubmitInfo2, flags),
		offset_of!(SubmitInfo2, wait_semaphore_info_count),
		offset_of!(SubmitInfo2, wait_semaphore_infos),
		offset_of!(SubmitInfo2, command_buffer_info_count),
		offset_of!(SubmitInfo2, command_buffer_infos),
		offset_of!(SubmitInfo2, signal_semaphore_info_count),
		offset_of!(SubmitInfo2, signal_semaphore_infos),
	];
	assert_eq!(offsets, [16, 20, 24, 32, 40, 48, 56]);
}

#[test]
fn barrier_structures_are_laid_out_as_vulkan_declares_them() {
	let image = (
		size_of::<ImageMemoryBarrier2>(),
		align_of::<ImageMemoryBarrier2>(),
	);
	assert_eq!(image, (96, 8));
	let offsets = [
		offset_of!(ImageMemoryBarrier2, src_stage_mask),
		offset_of!(ImageMemoryBarrier2, src_access_mask),
		offset_of!(ImageMemoryBarrier2, dst_stage_mask),
		offset_of!(ImageMemoryBarrier2, dst_access_mask),
		offset_of!(ImageMemoryBarrier2, old_layout),
		offset_of!(ImageMemoryBarrier2, new_layout),
		offset_of!(ImageMemoryBarrier2, src_queue_family_index),
		offset_of!(ImageMemoryBarrier2, dst_queue_family_index),
		offset_of!(ImageMemoryBarrier2, image),
		offset_of!(ImageMemoryBarrier2, subresource_range),
	];
	assert_eq!(offsets, [16, 24, 32, 40, 48, 52, 56, 60, 64, 72]);
	let buffer = (
		size_of::<BufferMemoryBarrier2>(),
		align_of::<BufferMemoryBarrier2>(),
	);
	assert_eq!(buffer, (80, 8));
	let offsets = [
		offset_of!(BufferMemoryBarrier2, dst_access_mask),
		offset_of!(BufferMemoryBarrier2, src_queue_family_index),
		offset_of!(BufferMemoryBarrier2, dst_queue_family_index),
		offset_of!(BufferMemoryBarrier2, buffer),
		offset_of!(BufferMemoryBarrier2, offset),
		offset_of!(BufferMemoryBarrier2, size),
	];
	assert_eq!(offsets, [40, 48, 52, 56, 64, 72]);
	let dependency = (size_of::<DependencyInfo>(), align_of::<DependencyInfo>());
	assert_eq!(dependency, (64, 8));
	let offsets = [
		offset_of!(DependencyInfo, dependency_flags),
		offset_of!(DependencyInfo, memory_barrier_count),
		offset_of!(DependencyInfo, memory_barriers),
		offset_of!(DependencyInfo, buffer_memory_barrier_count),
		offset_of!(DependencyInfo, buffer_memory_barriers),
		offset_of!(DependencyInfo, image_memory_barrier_count),
		offset_of!(DependencyInfo, image_memory_barriers),
	];
	assert_eq!(offsets, [16, 20, 24, 32, 40, 48, 56]);
}

#[test]
fn copy_regions_are_laid_out_as_vulkan_declares_them() {
	let copy = (size_of::<BufferImageCopy>(), align_of::<BufferImageCopy>());
	assert_eq!(copy, (56, 8));
	let offsets = [
		offset_of!(BufferImageCopy, buffer_offset),
		offset_of!(BufferImageCopy, buffer_row_length),
		offset_of!(BufferImageCopy, buffer_image_height),
		offset_of!(BufferImageCopy, image_subresource),
		offset_of!(BufferImageCopy, image_offset),
		offset_of!(BufferImageCopy, image_extent),
	];
	assert_eq!(offsets, [0, 8, 12, 16, 32, 44]);
}
