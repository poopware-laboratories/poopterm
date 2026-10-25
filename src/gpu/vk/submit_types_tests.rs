//! The layouts of the command and submit structures against the
//! specification's C declarations on 64-bit targets.
use super::submit_types::{
	CommandBufferAllocateInfo, CommandBufferBeginInfo, CommandBufferSubmitInfo,
	CommandPoolCreateInfo, SemaphoreSubmitInfo, SubmitInfo2,
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
