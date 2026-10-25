//! The pipeline that draws quads: the quad shaders, reading the quads from
//! a storage buffer and glyph texels from the atlas through a sampler
//! fixed in the descriptor set layout, blended over a target of one
//! format with dynamic rendering. Destroyed when dropped.
use super::{
	call::{Error, check},
	device::Device,
	image::Format,
	pipeline_types::*,
	shader::{QUAD_FRAGMENT, QUAD_VERTEX, ShaderModule, words},
};
use std::{ptr, rc::Rc};

/// `VkShaderStageFlagBits`: vertex and fragment.
const VERTEX: u32 = 0x1;
const FRAGMENT: u32 = 0x10;
/// `VkDescriptorType`: a combined image sampler and a storage buffer.
const COMBINED_IMAGE_SAMPLER: i32 = 1;
const STORAGE_BUFFER: i32 = 7;
/// `VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE`.
const CLAMP_TO_EDGE: i32 = 2;
/// `VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST`.
const TRIANGLE_LIST: i32 = 3;
/// `VK_DYNAMIC_STATE_VIEWPORT` and `_SCISSOR`.
const DYNAMIC: [i32; 2] = [0, 1];
/// The push constants: the target's width and height, two floats.
const PUSHED: u32 = 8;

/// Straight alpha over what is below: the color at its alpha, and the
/// alpha added to what is left of the target's.
const OVER: PipelineColorBlendAttachmentState = PipelineColorBlendAttachmentState {
	blend_enable: 1,
	// `VK_BLEND_FACTOR_SRC_ALPHA`, `_ONE_MINUS_SRC_ALPHA` and `_ONE`.
	src_color_blend_factor: 6,
	dst_color_blend_factor: 7,
	// `VK_BLEND_OP_ADD`.
	color_blend_op: 0,
	src_alpha_blend_factor: 1,
	dst_alpha_blend_factor: 7,
	alpha_blend_op: 0,
	// Red, green, blue and alpha.
	color_write_mask: 0xf,
};

pub(crate) struct QuadPipeline {
	/// Each zero until made.
	pipeline: RawPipeline,
	layout: RawPipelineLayout,
	set_layout: RawDescriptorSetLayout,
	sampler: RawSampler,
	/// Dropped after the rest is destroyed, as it was made from it.
	device: Rc<Device>,
}

impl QuadPipeline {
	/// The pipeline drawing quads to a target of `format`.
	pub(crate) fn new(device: &Rc<Device>, format: Format) -> Result<Self, Error> {
		Self::build(device, format, [QUAD_VERTEX, QUAD_FRAGMENT])
	}

	/// The pipeline of the vertex and fragment `shaders`, in SPIR-V.
	pub(super) fn build(
		device: &Rc<Device>,
		format: Format,
		shaders: [&[u8]; 2],
	) -> Result<Self, Error> {
		let [vertex, fragment] = shaders;
		let code = [words(vertex)?, words(fragment)?];
		let (handle, functions) = (device.handle, &device.functions);
		// From here on, dropping `made` undoes what was done so far.
		let mut made = Self {
			pipeline: 0,
			layout: 0,
			set_layout: 0,
			sampler: 0,
			device: device.clone(),
		};
		// Shaders fetch texels by index, so the sampler's settings matter
		// little: nearest, clamped, one level.
		let info = SamplerCreateInfo {
			s_type: STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			mag_filter: 0,
			min_filter: 0,
			mipmap_mode: 0,
			address_mode_u: CLAMP_TO_EDGE,
			address_mode_v: CLAMP_TO_EDGE,
			address_mode_w: CLAMP_TO_EDGE,
			mip_lod_bias: 0.0,
			anisotropy_enable: 0,
			max_anisotropy: 1.0,
			compare_enable: 0,
			compare_op: 0,
			min_lod: 0.0,
			max_lod: 0.0,
			border_color: 0,
			unnormalized_coordinates: 0,
		};
		// A failed call may leave anything in its output.
		let mut sampler = 0;
		// SAFETY: the device is live, `info` outlives the call, and the
		// handle is writable.
		check(unsafe { (functions.create_sampler)(handle, &info, ptr::null(), &mut sampler) })?;
		made.sampler = sampler;
		let bindings = [
			DescriptorSetLayoutBinding {
				binding: 0,
				descriptor_type: STORAGE_BUFFER,
				descriptor_count: 1,
				stage_flags: VERTEX | FRAGMENT,
				immutable_samplers: ptr::null(),
			},
			DescriptorSetLayoutBinding {
				binding: 1,
				descriptor_type: COMBINED_IMAGE_SAMPLER,
				descriptor_count: 1,
				stage_flags: FRAGMENT,
				immutable_samplers: &made.sampler,
			},
		];
		let info = DescriptorSetLayoutCreateInfo {
			s_type: STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			binding_count: bindings.len() as u32,
			bindings: bindings.as_ptr(),
		};
		let mut set_layout = 0;
		let create = functions.create_set_layout;
		// SAFETY: as for the sampler; the bindings and the sampler outlive
		// the call.
		check(unsafe { create(handle, &info, ptr::null(), &mut set_layout) })?;
		made.set_layout = set_layout;
		let range = PushConstantRange {
			stage_flags: VERTEX,
			offset: 0,
			size: PUSHED,
		};
		let info = PipelineLayoutCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			set_layout_count: 1,
			set_layouts: &made.set_layout,
			push_constant_range_count: 1,
			push_constant_ranges: &range,
		};
		let mut layout = 0;
		let create = functions.create_pipeline_layout;
		// SAFETY: as for the sampler; the set layout is live.
		check(unsafe { create(handle, &info, ptr::null(), &mut layout) })?;
		made.layout = layout;
		let modules = [
			ShaderModule::new(device, &code[0])?,
			ShaderModule::new(device, &code[1])?,
		];
		made.pipeline = made.pipeline(&modules, format)?;
		Ok(made)
	}

	/// The graphics pipeline of `modules`, drawing to `format`.
	fn pipeline(&self, modules: &[ShaderModule; 2], format: Format) -> Result<RawPipeline, Error> {
		let stage = |stage, module: &ShaderModule| PipelineShaderStageCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			stage,
			module: module.handle(),
			name: c"main".as_ptr(),
			specialization_info: ptr::null(),
		};
		let stages = [stage(VERTEX, &modules[0]), stage(FRAGMENT, &modules[1])];
		// The vertex shader reads the quads itself.
		let input = PipelineVertexInputStateCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			vertex_binding_description_count: 0,
			vertex_binding_descriptions: ptr::null(),
			vertex_attribute_description_count: 0,
			vertex_attribute_descriptions: ptr::null(),
		};
		let assembly = PipelineInputAssemblyStateCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			topology: TRIANGLE_LIST,
			primitive_restart_enable: 0,
		};
		// Set when drawing, as the target's size changes.
		let viewport = PipelineViewportStateCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			viewport_count: 1,
			viewports: ptr::null(),
			scissor_count: 1,
			scissors: ptr::null(),
		};
		// Filled, neither face culled.
		let raster = PipelineRasterizationStateCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			depth_clamp_enable: 0,
			rasterizer_discard_enable: 0,
			polygon_mode: 0,
			cull_mode: 0,
			front_face: 0,
			depth_bias_enable: 0,
			depth_bias_constant_factor: 0.0,
			depth_bias_clamp: 0.0,
			depth_bias_slope_factor: 0.0,
			line_width: 1.0,
		};
		let multisample = PipelineMultisampleStateCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			rasterization_samples: 1,
			sample_shading_enable: 0,
			min_sample_shading: 0.0,
			sample_mask: ptr::null(),
			alpha_to_coverage_enable: 0,
			alpha_to_one_enable: 0,
		};
		let blend = PipelineColorBlendStateCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			logic_op_enable: 0,
			logic_op: 0,
			attachment_count: 1,
			attachments: &OVER,
			blend_constants: [0.0; 4],
		};
		let dynamic = PipelineDynamicStateCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_DYNAMIC_STATE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			dynamic_state_count: DYNAMIC.len() as u32,
			dynamic_states: DYNAMIC.as_ptr(),
		};
		let format = format as i32;
		let rendering = PipelineRenderingCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO,
			next: ptr::null(),
			view_mask: 0,
			color_attachment_count: 1,
			color_attachment_formats: &format,
			depth_attachment_format: 0,
			stencil_attachment_format: 0,
		};
		let info = GraphicsPipelineCreateInfo {
			s_type: STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
			next: ptr::from_ref(&rendering).cast(),
			flags: 0,
			stage_count: stages.len() as u32,
			stages: stages.as_ptr(),
			vertex_input_state: &input,
			input_assembly_state: &assembly,
			tessellation_state: ptr::null(),
			viewport_state: &viewport,
			rasterization_state: &raster,
			multisample_state: &multisample,
			depth_stencil_state: ptr::null(),
			color_blend_state: &blend,
			dynamic_state: &dynamic,
			layout: self.layout,
			render_pass: 0,
			subpass: 0,
			base_pipeline_handle: 0,
			base_pipeline_index: -1,
		};
		let mut pipeline = 0;
		let (device, create) = (self.device.handle, self.device.functions.create_pipelines);
		// SAFETY: the device is live, the modules and layout are its, `info`
		// and every state it points to outlive the call, and there is room
		// for one pipeline.
		check(unsafe { create(device, 0, 1, &info, ptr::null(), &mut pipeline) })?;
		Ok(pipeline)
	}

	pub(crate) fn handle(&self) -> RawPipeline {
		self.pipeline
	}

	pub(crate) fn layout(&self) -> RawPipelineLayout {
		self.layout
	}

	pub(crate) fn set_layout(&self) -> RawDescriptorSetLayout {
		self.set_layout
	}
}

impl Drop for QuadPipeline {
	fn drop(&mut self) {
		let (device, functions) = (self.device.handle, &self.device.functions);
		// SAFETY: what was made is the device's and no longer used, and each
		// goes before what it was made with.
		unsafe {
			if self.pipeline != 0 {
				(functions.destroy_pipeline)(device, self.pipeline, ptr::null());
			}
			if self.layout != 0 {
				(functions.destroy_pipeline_layout)(device, self.layout, ptr::null());
			}
			if self.set_layout != 0 {
				(functions.destroy_set_layout)(device, self.set_layout, ptr::null());
			}
			if self.sampler != 0 {
				(functions.destroy_sampler)(device, self.sampler, ptr::null());
			}
		}
	}
}
