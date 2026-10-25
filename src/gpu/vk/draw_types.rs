//! The Vulkan types for updating descriptor sets and drawing with dynamic
//! rendering, laid out as the specification's C declarations, and the
//! types of their functions.
use super::{
	descriptor_types::RawDescriptorSet,
	pipeline_types::{RawPipeline, RawPipelineLayout, RawSampler},
	submit_types::RawCommandBuffer,
	types::{RawBuffer, RawDevice, RawImageView},
};
use std::ffi::c_void;

pub(crate) const STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET: i32 = 35;
pub(crate) const STRUCTURE_TYPE_RENDERING_INFO: i32 = 1_000_044_000;
pub(crate) const STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO: i32 = 1_000_044_001;

/// `VkDescriptorImageInfo`.
#[repr(C)]
pub(crate) struct DescriptorImageInfo {
	pub sampler: RawSampler,
	pub image_view: RawImageView,
	pub image_layout: i32,
}

/// `VkDescriptorBufferInfo`.
#[repr(C)]
pub(crate) struct DescriptorBufferInfo {
	pub buffer: RawBuffer,
	pub offset: u64,
	pub range: u64,
}

#[repr(C)]
pub(crate) struct WriteDescriptorSet {
	pub s_type: i32,
	pub next: *const c_void,
	pub dst_set: RawDescriptorSet,
	pub dst_binding: u32,
	pub dst_array_element: u32,
	pub descriptor_count: u32,
	pub descriptor_type: i32,
	pub image_info: *const DescriptorImageInfo,
	pub buffer_info: *const DescriptorBufferInfo,
	pub texel_buffer_view: *const u64,
}

/// `VkRect2D`: an offset and an extent.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect2D {
	pub offset: [i32; 2],
	pub extent: [u32; 2],
}

/// `VkViewport`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Viewport {
	pub x: f32,
	pub y: f32,
	pub width: f32,
	pub height: f32,
	pub min_depth: f32,
	pub max_depth: f32,
}

#[repr(C)]
pub(crate) struct RenderingAttachmentInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub image_view: RawImageView,
	pub image_layout: i32,
	pub resolve_mode: u32,
	pub resolve_image_view: RawImageView,
	pub resolve_image_layout: i32,
	pub load_op: i32,
	pub store_op: i32,
	/// `VkClearValue`, a union of 16 bytes.
	pub clear_value: [u32; 4],
}

#[repr(C)]
pub(crate) struct RenderingInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub render_area: Rect2D,
	pub layer_count: u32,
	pub view_mask: u32,
	pub color_attachment_count: u32,
	pub color_attachments: *const RenderingAttachmentInfo,
	pub depth_attachment: *const RenderingAttachmentInfo,
	pub stencil_attachment: *const RenderingAttachmentInfo,
}

pub(crate) type UpdateDescriptorSets =
	unsafe extern "system" fn(RawDevice, u32, *const WriteDescriptorSet, u32, *const c_void);
pub(crate) type CmdBeginRendering =
	unsafe extern "system" fn(RawCommandBuffer, *const RenderingInfo);
pub(crate) type CmdEndRendering = unsafe extern "system" fn(RawCommandBuffer);
pub(crate) type CmdBindPipeline = unsafe extern "system" fn(RawCommandBuffer, i32, RawPipeline);
pub(crate) type CmdSetViewport =
	unsafe extern "system" fn(RawCommandBuffer, u32, u32, *const Viewport);
pub(crate) type CmdSetScissor =
	unsafe extern "system" fn(RawCommandBuffer, u32, u32, *const Rect2D);
pub(crate) type CmdBindDescriptorSets = unsafe extern "system" fn(
	RawCommandBuffer,
	i32,
	RawPipelineLayout,
	u32,
	u32,
	*const RawDescriptorSet,
	u32,
	*const u32,
);
pub(crate) type CmdPushConstants =
	unsafe extern "system" fn(RawCommandBuffer, RawPipelineLayout, u32, u32, u32, *const c_void);
pub(crate) type CmdDraw = unsafe extern "system" fn(RawCommandBuffer, u32, u32, u32, u32);
