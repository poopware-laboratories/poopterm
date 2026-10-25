//! The layouts of the descriptor update and dynamic rendering structures
//! against the specification's C declarations on 64-bit targets: size,
//! alignment and every field's offset.
use super::draw_types::*;
use std::mem::{align_of, offset_of, size_of};

#[test]
fn descriptor_image_and_buffer_infos_are_laid_out_as_vulkan_declares_them() {
	// Both are read as arrays with a stride of 24.
	let image = (
		size_of::<DescriptorImageInfo>(),
		align_of::<DescriptorImageInfo>(),
	);
	assert_eq!(image, (24, 8));
	let offsets = [
		offset_of!(DescriptorImageInfo, sampler),
		offset_of!(DescriptorImageInfo, image_view),
		offset_of!(DescriptorImageInfo, image_layout),
	];
	assert_eq!(offsets, [0, 8, 16]);
	let buffer = (
		size_of::<DescriptorBufferInfo>(),
		align_of::<DescriptorBufferInfo>(),
	);
	assert_eq!(buffer, (24, 8));
	let offsets = [
		offset_of!(DescriptorBufferInfo, buffer),
		offset_of!(DescriptorBufferInfo, offset),
		offset_of!(DescriptorBufferInfo, range),
	];
	assert_eq!(offsets, [0, 8, 16]);
}

#[test]
fn write_descriptor_set_is_laid_out_as_vk_write_descriptor_set() {
	let size = (
		size_of::<WriteDescriptorSet>(),
		align_of::<WriteDescriptorSet>(),
	);
	assert_eq!(size, (64, 8));
	let offsets = [
		offset_of!(WriteDescriptorSet, s_type),
		offset_of!(WriteDescriptorSet, next),
		offset_of!(WriteDescriptorSet, dst_set),
		offset_of!(WriteDescriptorSet, dst_binding),
		offset_of!(WriteDescriptorSet, dst_array_element),
		offset_of!(WriteDescriptorSet, descriptor_count),
		offset_of!(WriteDescriptorSet, descriptor_type),
		offset_of!(WriteDescriptorSet, image_info),
		offset_of!(WriteDescriptorSet, buffer_info),
		offset_of!(WriteDescriptorSet, texel_buffer_view),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 28, 32, 36, 40, 48, 56]);
}

#[test]
fn rect_and_viewport_are_laid_out_as_vk_rect_2d_and_vk_viewport() {
	assert_eq!((size_of::<Rect2D>(), align_of::<Rect2D>()), (16, 4));
	assert_eq!(
		[offset_of!(Rect2D, offset), offset_of!(Rect2D, extent)],
		[0, 8]
	);
	assert_eq!((size_of::<Viewport>(), align_of::<Viewport>()), (24, 4));
	let offsets = [
		offset_of!(Viewport, x),
		offset_of!(Viewport, y),
		offset_of!(Viewport, width),
		offset_of!(Viewport, height),
		offset_of!(Viewport, min_depth),
		offset_of!(Viewport, max_depth),
	];
	assert_eq!(offsets, [0, 4, 8, 12, 16, 20]);
}

#[test]
fn rendering_attachment_info_is_laid_out_as_vk_rendering_attachment_info() {
	// The clear value ends at 68, padded to the pointers' alignment.
	let size = (
		size_of::<RenderingAttachmentInfo>(),
		align_of::<RenderingAttachmentInfo>(),
	);
	assert_eq!(size, (72, 8));
	let offsets = [
		offset_of!(RenderingAttachmentInfo, s_type),
		offset_of!(RenderingAttachmentInfo, next),
		offset_of!(RenderingAttachmentInfo, image_view),
		offset_of!(RenderingAttachmentInfo, image_layout),
		offset_of!(RenderingAttachmentInfo, resolve_mode),
		offset_of!(RenderingAttachmentInfo, resolve_image_view),
		offset_of!(RenderingAttachmentInfo, resolve_image_layout),
		offset_of!(RenderingAttachmentInfo, load_op),
		offset_of!(RenderingAttachmentInfo, store_op),
		offset_of!(RenderingAttachmentInfo, clear_value),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 28, 32, 40, 44, 48, 52]);
}

#[test]
fn rendering_info_is_laid_out_as_vk_rendering_info() {
	// The render area, four-aligned, follows the flags directly.
	assert_eq!(
		(size_of::<RenderingInfo>(), align_of::<RenderingInfo>()),
		(72, 8)
	);
	let offsets = [
		offset_of!(RenderingInfo, s_type),
		offset_of!(RenderingInfo, next),
		offset_of!(RenderingInfo, flags),
		offset_of!(RenderingInfo, render_area),
		offset_of!(RenderingInfo, layer_count),
		offset_of!(RenderingInfo, view_mask),
		offset_of!(RenderingInfo, color_attachment_count),
		offset_of!(RenderingInfo, color_attachments),
		offset_of!(RenderingInfo, depth_attachment),
		offset_of!(RenderingInfo, stencil_attachment),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 36, 40, 44, 48, 56, 64]);
}
