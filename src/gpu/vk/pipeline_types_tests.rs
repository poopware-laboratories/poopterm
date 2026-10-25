//! The layouts of the shader, sampler, descriptor set layout and pipeline
//! structures against the specification's C declarations on 64-bit
//! targets: size, alignment and every field's offset.
use super::pipeline_types::*;
use std::mem::{align_of, offset_of, size_of};

#[test]
fn shader_module_create_info_is_laid_out_as_vk_shader_module_create_info() {
	assert_eq!(
		(
			size_of::<ShaderModuleCreateInfo>(),
			align_of::<ShaderModuleCreateInfo>()
		),
		(40, 8)
	);
	let offsets = [
		offset_of!(ShaderModuleCreateInfo, s_type),
		offset_of!(ShaderModuleCreateInfo, next),
		offset_of!(ShaderModuleCreateInfo, flags),
		offset_of!(ShaderModuleCreateInfo, code_size),
		offset_of!(ShaderModuleCreateInfo, code),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32]);
}

#[test]
fn sampler_create_info_is_laid_out_as_vk_sampler_create_info() {
	assert_eq!(
		(
			size_of::<SamplerCreateInfo>(),
			align_of::<SamplerCreateInfo>()
		),
		(80, 8)
	);
	let offsets = [
		offset_of!(SamplerCreateInfo, s_type),
		offset_of!(SamplerCreateInfo, next),
		offset_of!(SamplerCreateInfo, flags),
		offset_of!(SamplerCreateInfo, mag_filter),
		offset_of!(SamplerCreateInfo, min_filter),
		offset_of!(SamplerCreateInfo, mipmap_mode),
		offset_of!(SamplerCreateInfo, address_mode_u),
		offset_of!(SamplerCreateInfo, address_mode_v),
		offset_of!(SamplerCreateInfo, address_mode_w),
		offset_of!(SamplerCreateInfo, mip_lod_bias),
		offset_of!(SamplerCreateInfo, anisotropy_enable),
		offset_of!(SamplerCreateInfo, max_anisotropy),
		offset_of!(SamplerCreateInfo, compare_enable),
		offset_of!(SamplerCreateInfo, compare_op),
		offset_of!(SamplerCreateInfo, min_lod),
		offset_of!(SamplerCreateInfo, max_lod),
		offset_of!(SamplerCreateInfo, border_color),
		offset_of!(SamplerCreateInfo, unnormalized_coordinates),
	];
	assert_eq!(
		offsets,
		[
			0, 8, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64, 68, 72, 76
		]
	);
}

#[test]
fn descriptor_set_layout_binding_is_laid_out_as_vk_descriptor_set_layout_binding() {
	assert_eq!(
		(
			size_of::<DescriptorSetLayoutBinding>(),
			align_of::<DescriptorSetLayoutBinding>()
		),
		(24, 8)
	);
	let offsets = [
		offset_of!(DescriptorSetLayoutBinding, binding),
		offset_of!(DescriptorSetLayoutBinding, descriptor_type),
		offset_of!(DescriptorSetLayoutBinding, descriptor_count),
		offset_of!(DescriptorSetLayoutBinding, stage_flags),
		offset_of!(DescriptorSetLayoutBinding, immutable_samplers),
	];
	assert_eq!(offsets, [0, 4, 8, 12, 16]);
}

#[test]
fn descriptor_set_layout_create_info_is_laid_out_as_vk_descriptor_set_layout_create_info() {
	assert_eq!(
		(
			size_of::<DescriptorSetLayoutCreateInfo>(),
			align_of::<DescriptorSetLayoutCreateInfo>()
		),
		(32, 8)
	);
	let offsets = [
		offset_of!(DescriptorSetLayoutCreateInfo, s_type),
		offset_of!(DescriptorSetLayoutCreateInfo, next),
		offset_of!(DescriptorSetLayoutCreateInfo, flags),
		offset_of!(DescriptorSetLayoutCreateInfo, binding_count),
		offset_of!(DescriptorSetLayoutCreateInfo, bindings),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24]);
}

#[test]
fn push_constant_range_is_laid_out_as_vk_push_constant_range() {
	assert_eq!(
		(
			size_of::<PushConstantRange>(),
			align_of::<PushConstantRange>()
		),
		(12, 4)
	);
	let offsets = [
		offset_of!(PushConstantRange, stage_flags),
		offset_of!(PushConstantRange, offset),
		offset_of!(PushConstantRange, size),
	];
	assert_eq!(offsets, [0, 4, 8]);
}

#[test]
fn pipeline_layout_create_info_is_laid_out_as_vk_pipeline_layout_create_info() {
	assert_eq!(
		(
			size_of::<PipelineLayoutCreateInfo>(),
			align_of::<PipelineLayoutCreateInfo>()
		),
		(48, 8)
	);
	let offsets = [
		offset_of!(PipelineLayoutCreateInfo, s_type),
		offset_of!(PipelineLayoutCreateInfo, next),
		offset_of!(PipelineLayoutCreateInfo, flags),
		offset_of!(PipelineLayoutCreateInfo, set_layout_count),
		offset_of!(PipelineLayoutCreateInfo, set_layouts),
		offset_of!(PipelineLayoutCreateInfo, push_constant_range_count),
		offset_of!(PipelineLayoutCreateInfo, push_constant_ranges),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 32, 40]);
}

#[test]
fn pipeline_shader_stage_create_info_is_laid_out_as_vk_pipeline_shader_stage_create_info() {
	assert_eq!(
		(
			size_of::<PipelineShaderStageCreateInfo>(),
			align_of::<PipelineShaderStageCreateInfo>()
		),
		(48, 8)
	);
	let offsets = [
		offset_of!(PipelineShaderStageCreateInfo, s_type),
		offset_of!(PipelineShaderStageCreateInfo, next),
		offset_of!(PipelineShaderStageCreateInfo, flags),
		offset_of!(PipelineShaderStageCreateInfo, stage),
		offset_of!(PipelineShaderStageCreateInfo, module),
		offset_of!(PipelineShaderStageCreateInfo, name),
		offset_of!(PipelineShaderStageCreateInfo, specialization_info),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 32, 40]);
}

#[test]
fn pipeline_vertex_input_state_create_info_is_laid_out_as_vk_pipeline_vertex_input_state_create_info()
 {
	assert_eq!(
		(
			size_of::<PipelineVertexInputStateCreateInfo>(),
			align_of::<PipelineVertexInputStateCreateInfo>()
		),
		(48, 8)
	);
	let offsets = [
		offset_of!(PipelineVertexInputStateCreateInfo, s_type),
		offset_of!(PipelineVertexInputStateCreateInfo, next),
		offset_of!(PipelineVertexInputStateCreateInfo, flags),
		offset_of!(
			PipelineVertexInputStateCreateInfo,
			vertex_binding_description_count
		),
		offset_of!(
			PipelineVertexInputStateCreateInfo,
			vertex_binding_descriptions
		),
		offset_of!(
			PipelineVertexInputStateCreateInfo,
			vertex_attribute_description_count
		),
		offset_of!(
			PipelineVertexInputStateCreateInfo,
			vertex_attribute_descriptions
		),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 32, 40]);
}

#[test]
fn pipeline_input_assembly_state_create_info_is_laid_out_as_vk_pipeline_input_assembly_state_create_info()
 {
	assert_eq!(
		(
			size_of::<PipelineInputAssemblyStateCreateInfo>(),
			align_of::<PipelineInputAssemblyStateCreateInfo>()
		),
		(32, 8)
	);
	let offsets = [
		offset_of!(PipelineInputAssemblyStateCreateInfo, s_type),
		offset_of!(PipelineInputAssemblyStateCreateInfo, next),
		offset_of!(PipelineInputAssemblyStateCreateInfo, flags),
		offset_of!(PipelineInputAssemblyStateCreateInfo, topology),
		offset_of!(
			PipelineInputAssemblyStateCreateInfo,
			primitive_restart_enable
		),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24]);
}

#[test]
fn pipeline_viewport_state_create_info_is_laid_out_as_vk_pipeline_viewport_state_create_info() {
	assert_eq!(
		(
			size_of::<PipelineViewportStateCreateInfo>(),
			align_of::<PipelineViewportStateCreateInfo>()
		),
		(48, 8)
	);
	let offsets = [
		offset_of!(PipelineViewportStateCreateInfo, s_type),
		offset_of!(PipelineViewportStateCreateInfo, next),
		offset_of!(PipelineViewportStateCreateInfo, flags),
		offset_of!(PipelineViewportStateCreateInfo, viewport_count),
		offset_of!(PipelineViewportStateCreateInfo, viewports),
		offset_of!(PipelineViewportStateCreateInfo, scissor_count),
		offset_of!(PipelineViewportStateCreateInfo, scissors),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 32, 40]);
}

#[test]
fn pipeline_rasterization_state_create_info_is_laid_out_as_vk_pipeline_rasterization_state_create_info()
 {
	// The line width ends at 60, and the whole is padded to the pointer's
	// alignment.
	assert_eq!(
		(
			size_of::<PipelineRasterizationStateCreateInfo>(),
			align_of::<PipelineRasterizationStateCreateInfo>()
		),
		(64, 8)
	);
	let offsets = [
		offset_of!(PipelineRasterizationStateCreateInfo, s_type),
		offset_of!(PipelineRasterizationStateCreateInfo, next),
		offset_of!(PipelineRasterizationStateCreateInfo, flags),
		offset_of!(PipelineRasterizationStateCreateInfo, depth_clamp_enable),
		offset_of!(
			PipelineRasterizationStateCreateInfo,
			rasterizer_discard_enable
		),
		offset_of!(PipelineRasterizationStateCreateInfo, polygon_mode),
		offset_of!(PipelineRasterizationStateCreateInfo, cull_mode),
		offset_of!(PipelineRasterizationStateCreateInfo, front_face),
		offset_of!(PipelineRasterizationStateCreateInfo, depth_bias_enable),
		offset_of!(
			PipelineRasterizationStateCreateInfo,
			depth_bias_constant_factor
		),
		offset_of!(PipelineRasterizationStateCreateInfo, depth_bias_clamp),
		offset_of!(
			PipelineRasterizationStateCreateInfo,
			depth_bias_slope_factor
		),
		offset_of!(PipelineRasterizationStateCreateInfo, line_width),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56]);
}

#[test]
fn pipeline_multisample_state_create_info_is_laid_out_as_vk_pipeline_multisample_state_create_info()
{
	assert_eq!(
		(
			size_of::<PipelineMultisampleStateCreateInfo>(),
			align_of::<PipelineMultisampleStateCreateInfo>()
		),
		(48, 8)
	);
	let offsets = [
		offset_of!(PipelineMultisampleStateCreateInfo, s_type),
		offset_of!(PipelineMultisampleStateCreateInfo, next),
		offset_of!(PipelineMultisampleStateCreateInfo, flags),
		offset_of!(PipelineMultisampleStateCreateInfo, rasterization_samples),
		offset_of!(PipelineMultisampleStateCreateInfo, sample_shading_enable),
		offset_of!(PipelineMultisampleStateCreateInfo, min_sample_shading),
		offset_of!(PipelineMultisampleStateCreateInfo, sample_mask),
		offset_of!(PipelineMultisampleStateCreateInfo, alpha_to_coverage_enable),
		offset_of!(PipelineMultisampleStateCreateInfo, alpha_to_one_enable),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 28, 32, 40, 44]);
}

#[test]
fn pipeline_color_blend_attachment_state_is_laid_out_as_vk_pipeline_color_blend_attachment_state() {
	// An array of these is read with a stride of 32, so no padding may
	// creep in.
	assert_eq!(
		(
			size_of::<PipelineColorBlendAttachmentState>(),
			align_of::<PipelineColorBlendAttachmentState>()
		),
		(32, 4)
	);
	let offsets = [
		offset_of!(PipelineColorBlendAttachmentState, blend_enable),
		offset_of!(PipelineColorBlendAttachmentState, src_color_blend_factor),
		offset_of!(PipelineColorBlendAttachmentState, dst_color_blend_factor),
		offset_of!(PipelineColorBlendAttachmentState, color_blend_op),
		offset_of!(PipelineColorBlendAttachmentState, src_alpha_blend_factor),
		offset_of!(PipelineColorBlendAttachmentState, dst_alpha_blend_factor),
		offset_of!(PipelineColorBlendAttachmentState, alpha_blend_op),
		offset_of!(PipelineColorBlendAttachmentState, color_write_mask),
	];
	assert_eq!(offsets, [0, 4, 8, 12, 16, 20, 24, 28]);
}

#[test]
fn pipeline_color_blend_state_create_info_is_laid_out_as_vk_pipeline_color_blend_state_create_info()
{
	assert_eq!(
		(
			size_of::<PipelineColorBlendStateCreateInfo>(),
			align_of::<PipelineColorBlendStateCreateInfo>()
		),
		(56, 8)
	);
	let offsets = [
		offset_of!(PipelineColorBlendStateCreateInfo, s_type),
		offset_of!(PipelineColorBlendStateCreateInfo, next),
		offset_of!(PipelineColorBlendStateCreateInfo, flags),
		offset_of!(PipelineColorBlendStateCreateInfo, logic_op_enable),
		offset_of!(PipelineColorBlendStateCreateInfo, logic_op),
		offset_of!(PipelineColorBlendStateCreateInfo, attachment_count),
		offset_of!(PipelineColorBlendStateCreateInfo, attachments),
		offset_of!(PipelineColorBlendStateCreateInfo, blend_constants),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 28, 32, 40]);
}

#[test]
fn pipeline_dynamic_state_create_info_is_laid_out_as_vk_pipeline_dynamic_state_create_info() {
	assert_eq!(
		(
			size_of::<PipelineDynamicStateCreateInfo>(),
			align_of::<PipelineDynamicStateCreateInfo>()
		),
		(32, 8)
	);
	let offsets = [
		offset_of!(PipelineDynamicStateCreateInfo, s_type),
		offset_of!(PipelineDynamicStateCreateInfo, next),
		offset_of!(PipelineDynamicStateCreateInfo, flags),
		offset_of!(PipelineDynamicStateCreateInfo, dynamic_state_count),
		offset_of!(PipelineDynamicStateCreateInfo, dynamic_states),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24]);
}

#[test]
fn pipeline_rendering_create_info_is_laid_out_as_vk_pipeline_rendering_create_info() {
	// No flags: the view mask follows `pNext`.
	assert_eq!(
		(
			size_of::<PipelineRenderingCreateInfo>(),
			align_of::<PipelineRenderingCreateInfo>()
		),
		(40, 8)
	);
	let offsets = [
		offset_of!(PipelineRenderingCreateInfo, s_type),
		offset_of!(PipelineRenderingCreateInfo, next),
		offset_of!(PipelineRenderingCreateInfo, view_mask),
		offset_of!(PipelineRenderingCreateInfo, color_attachment_count),
		offset_of!(PipelineRenderingCreateInfo, color_attachment_formats),
		offset_of!(PipelineRenderingCreateInfo, depth_attachment_format),
		offset_of!(PipelineRenderingCreateInfo, stencil_attachment_format),
	];
	assert_eq!(offsets, [0, 8, 16, 20, 24, 32, 36]);
}

#[test]
fn graphics_pipeline_create_info_is_laid_out_as_vk_graphics_pipeline_create_info() {
	assert_eq!(
		(
			size_of::<GraphicsPipelineCreateInfo>(),
			align_of::<GraphicsPipelineCreateInfo>()
		),
		(144, 8)
	);
	let offsets = [
		offset_of!(GraphicsPipelineCreateInfo, s_type),
		offset_of!(GraphicsPipelineCreateInfo, next),
		offset_of!(GraphicsPipelineCreateInfo, flags),
		offset_of!(GraphicsPipelineCreateInfo, stage_count),
		offset_of!(GraphicsPipelineCreateInfo, stages),
		offset_of!(GraphicsPipelineCreateInfo, vertex_input_state),
		offset_of!(GraphicsPipelineCreateInfo, input_assembly_state),
		offset_of!(GraphicsPipelineCreateInfo, tessellation_state),
		offset_of!(GraphicsPipelineCreateInfo, viewport_state),
		offset_of!(GraphicsPipelineCreateInfo, rasterization_state),
		offset_of!(GraphicsPipelineCreateInfo, multisample_state),
		offset_of!(GraphicsPipelineCreateInfo, depth_stencil_state),
		offset_of!(GraphicsPipelineCreateInfo, color_blend_state),
		offset_of!(GraphicsPipelineCreateInfo, dynamic_state),
		offset_of!(GraphicsPipelineCreateInfo, layout),
		offset_of!(GraphicsPipelineCreateInfo, render_pass),
		offset_of!(GraphicsPipelineCreateInfo, subpass),
		offset_of!(GraphicsPipelineCreateInfo, base_pipeline_handle),
		offset_of!(GraphicsPipelineCreateInfo, base_pipeline_index),
	];
	assert_eq!(
		offsets,
		[
			0, 8, 16, 20, 24, 32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128, 136
		]
	);
}
