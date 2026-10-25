//! The Vulkan types for shaders, samplers, descriptor set layouts and
//! graphics pipelines, laid out as the specification's C declarations, and
//! the types of their functions.
use super::types::RawDevice;
use std::ffi::{c_char, c_void};

/// Non-dispatchable handles: 64 bits on every platform.
pub(crate) type RawShaderModule = u64;
pub(crate) type RawSampler = u64;
pub(crate) type RawDescriptorSetLayout = u64;
pub(crate) type RawPipelineLayout = u64;
pub(crate) type RawPipeline = u64;

/// `VkStructureType` values.
pub(crate) const STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO: i32 = 16;
pub(crate) const STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO: i32 = 18;
pub(crate) const STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO: i32 = 19;
pub(crate) const STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO: i32 = 20;
pub(crate) const STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO: i32 = 22;
pub(crate) const STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO: i32 = 23;
pub(crate) const STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO: i32 = 24;
pub(crate) const STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO: i32 = 26;
pub(crate) const STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO: i32 = 27;
pub(crate) const STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO: i32 = 28;
pub(crate) const STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO: i32 = 30;
pub(crate) const STRUCTURE_TYPE_SAMPLER_CREATE_INFO: i32 = 31;
pub(crate) const STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO: i32 = 32;
pub(crate) const STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO: i32 = 1_000_044_002;

/// `VkShaderModuleCreateInfo`.
#[repr(C)]
pub(crate) struct ShaderModuleCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	/// In bytes, a multiple of 4.
	pub code_size: usize,
	pub code: *const u32,
}

/// `VkSamplerCreateInfo`.
#[repr(C)]
pub(crate) struct SamplerCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub mag_filter: i32,
	pub min_filter: i32,
	pub mipmap_mode: i32,
	pub address_mode_u: i32,
	pub address_mode_v: i32,
	pub address_mode_w: i32,
	pub mip_lod_bias: f32,
	pub anisotropy_enable: u32,
	pub max_anisotropy: f32,
	pub compare_enable: u32,
	pub compare_op: i32,
	pub min_lod: f32,
	pub max_lod: f32,
	pub border_color: i32,
	pub unnormalized_coordinates: u32,
}

/// `VkDescriptorSetLayoutBinding`.
#[repr(C)]
pub(crate) struct DescriptorSetLayoutBinding {
	pub binding: u32,
	/// `VkDescriptorType`.
	pub descriptor_type: i32,
	pub descriptor_count: u32,
	/// `VkShaderStageFlags`.
	pub stage_flags: u32,
	/// Null, or `descriptor_count` samplers.
	pub immutable_samplers: *const RawSampler,
}

/// `VkDescriptorSetLayoutCreateInfo`.
#[repr(C)]
pub(crate) struct DescriptorSetLayoutCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub binding_count: u32,
	pub bindings: *const DescriptorSetLayoutBinding,
}

/// `VkPushConstantRange`.
#[repr(C)]
pub(crate) struct PushConstantRange {
	pub stage_flags: u32,
	pub offset: u32,
	pub size: u32,
}

/// `VkPipelineLayoutCreateInfo`.
#[repr(C)]
pub(crate) struct PipelineLayoutCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub set_layout_count: u32,
	pub set_layouts: *const RawDescriptorSetLayout,
	pub push_constant_range_count: u32,
	pub push_constant_ranges: *const PushConstantRange,
}

/// `VkPipelineShaderStageCreateInfo`.
#[repr(C)]
pub(crate) struct PipelineShaderStageCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	/// `VkShaderStageFlagBits`.
	pub stage: u32,
	pub module: RawShaderModule,
	/// The entry point.
	pub name: *const c_char,
	pub specialization_info: *const c_void,
}

/// `VkPipelineVertexInputStateCreateInfo`, with no vertex buffers: the
/// shaders read what they draw from a storage buffer.
#[repr(C)]
pub(crate) struct PipelineVertexInputStateCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub vertex_binding_description_count: u32,
	pub vertex_binding_descriptions: *const c_void,
	pub vertex_attribute_description_count: u32,
	pub vertex_attribute_descriptions: *const c_void,
}

/// `VkPipelineInputAssemblyStateCreateInfo`.
#[repr(C)]
pub(crate) struct PipelineInputAssemblyStateCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	/// `VkPrimitiveTopology`.
	pub topology: i32,
	pub primitive_restart_enable: u32,
}

/// `VkPipelineViewportStateCreateInfo`, its viewports and scissors set
/// when drawing.
#[repr(C)]
pub(crate) struct PipelineViewportStateCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub viewport_count: u32,
	pub viewports: *const c_void,
	pub scissor_count: u32,
	pub scissors: *const c_void,
}

/// `VkPipelineRasterizationStateCreateInfo`.
#[repr(C)]
pub(crate) struct PipelineRasterizationStateCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub depth_clamp_enable: u32,
	pub rasterizer_discard_enable: u32,
	/// `VkPolygonMode`.
	pub polygon_mode: i32,
	/// `VkCullModeFlags`.
	pub cull_mode: u32,
	/// `VkFrontFace`.
	pub front_face: i32,
	pub depth_bias_enable: u32,
	pub depth_bias_constant_factor: f32,
	pub depth_bias_clamp: f32,
	pub depth_bias_slope_factor: f32,
	pub line_width: f32,
}

/// `VkPipelineMultisampleStateCreateInfo`.
#[repr(C)]
pub(crate) struct PipelineMultisampleStateCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	/// `VkSampleCountFlagBits`.
	pub rasterization_samples: u32,
	pub sample_shading_enable: u32,
	pub min_sample_shading: f32,
	pub sample_mask: *const u32,
	pub alpha_to_coverage_enable: u32,
	pub alpha_to_one_enable: u32,
}

/// `VkPipelineColorBlendAttachmentState`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PipelineColorBlendAttachmentState {
	pub blend_enable: u32,
	/// `VkBlendFactor` and `VkBlendOp` values.
	pub src_color_blend_factor: i32,
	pub dst_color_blend_factor: i32,
	pub color_blend_op: i32,
	pub src_alpha_blend_factor: i32,
	pub dst_alpha_blend_factor: i32,
	pub alpha_blend_op: i32,
	/// `VkColorComponentFlags`.
	pub color_write_mask: u32,
}

/// `VkPipelineColorBlendStateCreateInfo`.
#[repr(C)]
pub(crate) struct PipelineColorBlendStateCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub logic_op_enable: u32,
	/// `VkLogicOp`.
	pub logic_op: i32,
	pub attachment_count: u32,
	pub attachments: *const PipelineColorBlendAttachmentState,
	pub blend_constants: [f32; 4],
}

/// `VkPipelineDynamicStateCreateInfo`.
#[repr(C)]
pub(crate) struct PipelineDynamicStateCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub dynamic_state_count: u32,
	/// `VkDynamicState` values.
	pub dynamic_states: *const i32,
}

/// `VkPipelineRenderingCreateInfo`: what a pipeline for dynamic rendering
/// draws to.
#[repr(C)]
pub(crate) struct PipelineRenderingCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub view_mask: u32,
	pub color_attachment_count: u32,
	/// `VkFormat` values.
	pub color_attachment_formats: *const i32,
	pub depth_attachment_format: i32,
	pub stencil_attachment_format: i32,
}

/// `VkGraphicsPipelineCreateInfo`.
#[repr(C)]
pub(crate) struct GraphicsPipelineCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub stage_count: u32,
	pub stages: *const PipelineShaderStageCreateInfo,
	pub vertex_input_state: *const PipelineVertexInputStateCreateInfo,
	pub input_assembly_state: *const PipelineInputAssemblyStateCreateInfo,
	pub tessellation_state: *const c_void,
	pub viewport_state: *const PipelineViewportStateCreateInfo,
	pub rasterization_state: *const PipelineRasterizationStateCreateInfo,
	pub multisample_state: *const PipelineMultisampleStateCreateInfo,
	pub depth_stencil_state: *const c_void,
	pub color_blend_state: *const PipelineColorBlendStateCreateInfo,
	pub dynamic_state: *const PipelineDynamicStateCreateInfo,
	pub layout: RawPipelineLayout,
	pub render_pass: u64,
	pub subpass: u32,
	pub base_pipeline_handle: RawPipeline,
	pub base_pipeline_index: i32,
}

pub(crate) type CreateShaderModule = unsafe extern "system" fn(
	RawDevice,
	*const ShaderModuleCreateInfo,
	*const c_void,
	*mut RawShaderModule,
) -> i32;
pub(crate) type DestroyShaderModule =
	unsafe extern "system" fn(RawDevice, RawShaderModule, *const c_void);
pub(crate) type CreateSampler = unsafe extern "system" fn(
	RawDevice,
	*const SamplerCreateInfo,
	*const c_void,
	*mut RawSampler,
) -> i32;
pub(crate) type DestroySampler = unsafe extern "system" fn(RawDevice, RawSampler, *const c_void);
pub(crate) type CreateDescriptorSetLayout = unsafe extern "system" fn(
	RawDevice,
	*const DescriptorSetLayoutCreateInfo,
	*const c_void,
	*mut RawDescriptorSetLayout,
) -> i32;
pub(crate) type DestroyDescriptorSetLayout =
	unsafe extern "system" fn(RawDevice, RawDescriptorSetLayout, *const c_void);
pub(crate) type CreatePipelineLayout = unsafe extern "system" fn(
	RawDevice,
	*const PipelineLayoutCreateInfo,
	*const c_void,
	*mut RawPipelineLayout,
) -> i32;
pub(crate) type DestroyPipelineLayout =
	unsafe extern "system" fn(RawDevice, RawPipelineLayout, *const c_void);
/// The second argument is a `VkPipelineCache`, which the renderer never
/// passes.
pub(crate) type CreateGraphicsPipelines = unsafe extern "system" fn(
	RawDevice,
	u64,
	u32,
	*const GraphicsPipelineCreateInfo,
	*const c_void,
	*mut RawPipeline,
) -> i32;
pub(crate) type DestroyPipeline = unsafe extern "system" fn(RawDevice, RawPipeline, *const c_void);
