//! The Vulkan types for recording commands (barriers and copies) and
//! submitting them to a queue, laid out as the specification's C
//! declarations, and the types of their functions.
use super::types::{RawBuffer, RawDevice, RawImage, RawQueue, RawSemaphore, SubresourceRange};
use std::ffi::c_void;

/// A `VkCommandPool`: 64 bits on every platform.
pub(crate) type RawCommandPool = u64;
/// A `VkCommandBuffer`.
pub(crate) type RawCommandBuffer = *mut c_void;

/// `VkStructureType` values.
pub(crate) const STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO: i32 = 39;
pub(crate) const STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO: i32 = 40;
pub(crate) const STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO: i32 = 42;
pub(crate) const STRUCTURE_TYPE_BUFFER_MEMORY_BARRIER_2: i32 = 1_000_314_001;
pub(crate) const STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2: i32 = 1_000_314_002;
pub(crate) const STRUCTURE_TYPE_DEPENDENCY_INFO: i32 = 1_000_314_003;
pub(crate) const STRUCTURE_TYPE_SUBMIT_INFO_2: i32 = 1_000_314_004;
pub(crate) const STRUCTURE_TYPE_SEMAPHORE_SUBMIT_INFO: i32 = 1_000_314_005;
pub(crate) const STRUCTURE_TYPE_COMMAND_BUFFER_SUBMIT_INFO: i32 = 1_000_314_006;

/// `VkCommandPoolCreateInfo`.
#[repr(C)]
pub(crate) struct CommandPoolCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	/// `VkCommandPoolCreateFlags`.
	pub flags: u32,
	pub queue_family_index: u32,
}

/// `VkCommandBufferAllocateInfo`.
#[repr(C)]
pub(crate) struct CommandBufferAllocateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub command_pool: RawCommandPool,
	/// `VkCommandBufferLevel`: 0, primary.
	pub level: i32,
	pub command_buffer_count: u32,
}

/// `VkCommandBufferBeginInfo`.
#[repr(C)]
pub(crate) struct CommandBufferBeginInfo {
	pub s_type: i32,
	pub next: *const c_void,
	/// `VkCommandBufferUsageFlags`.
	pub flags: u32,
	/// Only for secondary command buffers.
	pub inheritance_info: *const c_void,
}

/// `VkSemaphoreSubmitInfo`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct SemaphoreSubmitInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub semaphore: RawSemaphore,
	pub value: u64,
	/// `VkPipelineStageFlags2`.
	pub stage_mask: u64,
	pub device_index: u32,
}

/// `VkCommandBufferSubmitInfo`.
#[repr(C)]
pub(crate) struct CommandBufferSubmitInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub command_buffer: RawCommandBuffer,
	pub device_mask: u32,
}

/// `VkSubmitInfo2`.
#[repr(C)]
pub(crate) struct SubmitInfo2 {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub wait_semaphore_info_count: u32,
	pub wait_semaphore_infos: *const SemaphoreSubmitInfo,
	pub command_buffer_info_count: u32,
	pub command_buffer_infos: *const CommandBufferSubmitInfo,
	pub signal_semaphore_info_count: u32,
	pub signal_semaphore_infos: *const SemaphoreSubmitInfo,
}

/// `VK_QUEUE_FAMILY_IGNORED`: a barrier that transfers nothing between
/// queue families.
pub(crate) const QUEUE_FAMILY_IGNORED: u32 = !0;

/// `VkImageMemoryBarrier2`.
#[repr(C)]
pub(crate) struct ImageMemoryBarrier2 {
	pub s_type: i32,
	pub next: *const c_void,
	/// `VkPipelineStageFlags2` and `VkAccessFlags2`, before and after.
	pub src_stage_mask: u64,
	pub src_access_mask: u64,
	pub dst_stage_mask: u64,
	pub dst_access_mask: u64,
	/// `VkImageLayout`.
	pub old_layout: i32,
	pub new_layout: i32,
	pub src_queue_family_index: u32,
	pub dst_queue_family_index: u32,
	pub image: RawImage,
	pub subresource_range: SubresourceRange,
}

/// `VkBufferMemoryBarrier2`.
#[repr(C)]
pub(crate) struct BufferMemoryBarrier2 {
	pub s_type: i32,
	pub next: *const c_void,
	pub src_stage_mask: u64,
	pub src_access_mask: u64,
	pub dst_stage_mask: u64,
	pub dst_access_mask: u64,
	pub src_queue_family_index: u32,
	pub dst_queue_family_index: u32,
	pub buffer: RawBuffer,
	pub offset: u64,
	pub size: u64,
}

/// `VkDependencyInfo`, with no global memory barriers.
#[repr(C)]
pub(crate) struct DependencyInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub dependency_flags: u32,
	pub memory_barrier_count: u32,
	pub memory_barriers: *const c_void,
	pub buffer_memory_barrier_count: u32,
	pub buffer_memory_barriers: *const BufferMemoryBarrier2,
	pub image_memory_barrier_count: u32,
	pub image_memory_barriers: *const ImageMemoryBarrier2,
}

/// `VkBufferImageCopy`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BufferImageCopy {
	pub buffer_offset: u64,
	/// Texels per row and rows per layer in the buffer; 0 for tightly
	/// packed.
	pub buffer_row_length: u32,
	pub buffer_image_height: u32,
	/// `VkImageSubresourceLayers`: aspect, mip level, first layer, layers.
	pub image_subresource: [u32; 4],
	/// `VkOffset3D` and `VkExtent3D`.
	pub image_offset: [i32; 3],
	pub image_extent: [u32; 3],
}

pub(crate) type CreateCommandPool = unsafe extern "system" fn(
	RawDevice,
	*const CommandPoolCreateInfo,
	*const c_void,
	*mut RawCommandPool,
) -> i32;
pub(crate) type DestroyCommandPool =
	unsafe extern "system" fn(RawDevice, RawCommandPool, *const c_void);
pub(crate) type ResetCommandPool = unsafe extern "system" fn(RawDevice, RawCommandPool, u32) -> i32;
pub(crate) type AllocateCommandBuffers = unsafe extern "system" fn(
	RawDevice,
	*const CommandBufferAllocateInfo,
	*mut RawCommandBuffer,
) -> i32;
pub(crate) type BeginCommandBuffer =
	unsafe extern "system" fn(RawCommandBuffer, *const CommandBufferBeginInfo) -> i32;
pub(crate) type EndCommandBuffer = unsafe extern "system" fn(RawCommandBuffer) -> i32;
/// The last argument is a `VkFence`, which the renderer never passes.
pub(crate) type QueueSubmit2 =
	unsafe extern "system" fn(RawQueue, u32, *const SubmitInfo2, u64) -> i32;
pub(crate) type CmdPipelineBarrier2 =
	unsafe extern "system" fn(RawCommandBuffer, *const DependencyInfo);
pub(crate) type CmdCopyBufferToImage = unsafe extern "system" fn(
	RawCommandBuffer,
	RawBuffer,
	RawImage,
	i32,
	u32,
	*const BufferImageCopy,
);
pub(crate) type CmdCopyImageToBuffer = unsafe extern "system" fn(
	RawCommandBuffer,
	RawImage,
	i32,
	RawBuffer,
	u32,
	*const BufferImageCopy,
);
