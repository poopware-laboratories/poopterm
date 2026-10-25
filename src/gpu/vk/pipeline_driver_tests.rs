//! The fake driver's shader module, sampler, layout and pipeline
//! functions, for `pipeline_tests`: each checks the specification's valid
//! usage rules for what the quad pipeline makes, records what was made and
//! hands out handles, and destroys them.
use super::{
	buffer_tests::GARBAGE,
	driver_tests::{Driver, with},
	pipeline_types::*,
	types::*,
};
use std::{
	ffi::{CStr, c_char, c_void},
	mem::transmute,
	slice,
};

/// A binding of a set layout: binding, type, count, stages and immutable
/// sampler.
pub(super) type Binding = (u32, i32, u32, u32, Option<RawSampler>);

/// What a pipeline was made with.
#[derive(Debug, PartialEq)]
pub(super) struct MadePipeline {
	/// Each stage, its module and entry point.
	pub stages: Vec<(u32, RawShaderModule, String)>,
	pub topology: i32,
	/// Polygon mode, cull mode, front face and line width.
	pub rasterization: (i32, u32, i32, f32),
	pub samples: u32,
	pub blend: Vec<PipelineColorBlendAttachmentState>,
	pub dynamic: Vec<i32>,
	pub formats: Vec<i32>,
	pub layout: RawPipelineLayout,
}

/// The shader modules, samplers, layouts and pipelines the fake made.
#[derive(Default)]
pub(super) struct Objects {
	/// The code of each module.
	pub modules: Vec<Vec<u32>>,
	/// The filters, mipmap mode and address modes of each sampler.
	pub samplers: Vec<[i32; 6]>,
	pub set_layouts: Vec<Vec<Binding>>,
	/// The set layouts and push constant ranges of each pipeline layout.
	pub pipeline_layouts: Vec<(Vec<RawDescriptorSetLayout>, Vec<[u32; 3]>)>,
	pub pipelines: Vec<MadePipeline>,
	/// The handles of every kind not destroyed yet.
	pub live: Vec<u64>,
}

/// `VkShaderStageFlagBits`: vertex and fragment.
pub(super) const VERTEX: u32 = 0x1;
pub(super) const FRAGMENT: u32 = 0x10;
/// `VkDescriptorType`: a combined image sampler and a storage buffer.
pub(super) const COMBINED_IMAGE_SAMPLER: i32 = 1;
pub(super) const STORAGE_BUFFER: i32 = 7;
/// `VK_DYNAMIC_STATE_VIEWPORT` and `_SCISSOR`.
pub(super) const VIEWPORT: i32 = 0;
pub(super) const SCISSOR: i32 = 1;
/// The handles each kind is numbered from.
const MODULES: u64 = 0x60_0000;
const SAMPLERS: u64 = 0x61_0000;
const SET_LAYOUTS: u64 = 0x62_0000;
const PIPELINE_LAYOUTS: u64 = 0x63_0000;
const PIPELINES: u64 = 0x64_0000;

/// The fake's shader and pipeline function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateShaderModule" => transmute::<CreateShaderModule, VoidFunction>(create_module),
			b"vkDestroyShaderModule" => transmute::<DestroyShaderModule, VoidFunction>(destroy_module),
			b"vkCreateSampler" => transmute::<CreateSampler, VoidFunction>(create_sampler),
			b"vkDestroySampler" => transmute::<DestroySampler, VoidFunction>(destroy_sampler),
			b"vkCreateDescriptorSetLayout" => {
				transmute::<CreateDescriptorSetLayout, VoidFunction>(create_set_layout)
			}
			b"vkDestroyDescriptorSetLayout" => {
				transmute::<DestroyDescriptorSetLayout, VoidFunction>(destroy_set_layout)
			}
			b"vkCreatePipelineLayout" => {
				transmute::<CreatePipelineLayout, VoidFunction>(create_pipeline_layout)
			}
			b"vkDestroyPipelineLayout" => {
				transmute::<DestroyPipelineLayout, VoidFunction>(destroy_pipeline_layout)
			}
			b"vkCreateGraphicsPipelines" => {
				transmute::<CreateGraphicsPipelines, VoidFunction>(create_pipelines)
			}
			b"vkDestroyPipeline" => transmute::<DestroyPipeline, VoidFunction>(destroy_pipeline),
			_ => return None,
		})
	}
}

impl Driver {
	fn live_object(&self, handle: u64, kind: u64) -> bool {
		handle & !0xffff == kind && self.pipeline_objects.live.contains(&handle)
	}

	/// Checks the rules every create call shares, and makes the next
	/// handle of `kind`, numbered `count` + 1, unless `name` fails.
	fn make_object(
		&mut self,
		device: RawDevice,
		call: &'static CStr,
		(s_type, expected, allocator): (i32, i32, *const c_void),
		kind: u64,
		count: usize,
	) -> Result<u64, i32> {
		let name = call.to_str().unwrap();
		self.check_live_device(device, name);
		self.require(s_type == expected, || format!("{name}: sType"));
		self.require(allocator.is_null(), || format!("{name}: an allocator"));
		if let Some(code) = self.failing(call) {
			return Err(code);
		}
		let handle = kind + count as u64 + 1;
		self.pipeline_objects.live.push(handle);
		Ok(handle)
	}

	/// Destroys `handle` of `kind`, breaking the test unless it is live.
	fn release_object(
		&mut self,
		device: RawDevice,
		(handle, kind, allocator): (u64, u64, *const c_void),
		call: &str,
		what: &str,
	) {
		self.check_live_device(device, call);
		let live = self.live_object(handle, kind) && allocator.is_null();
		self.require(live, || format!("{call}: {handle:#x}, which is not live"));
		self.pipeline_objects.live.retain(|&other| other != handle);
		self.released.push(format!("{what} {handle:#x}"));
	}
}

pub(super) unsafe extern "system" fn create_module(
	device: RawDevice,
	info: *const ShaderModuleCreateInfo,
	allocator: *const c_void,
	module: *mut RawShaderModule,
) -> i32 {
	// SAFETY: the bindings pass a create info whose code is `code_size`
	// bytes.
	let (info, code) = unsafe {
		let info = &*info;
		let code = slice::from_raw_parts(info.code, info.code_size / 4);
		(info, code)
	};
	with(|driver| {
		let rules = [
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(
				info.code_size > 0 && info.code_size.is_multiple_of(4),
				"a size in words",
			),
			(code.first() == Some(&0x0723_0203), "SPIR-V"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateShaderModule: {rule}"));
		}
		let count = driver.pipeline_objects.modules.len();
		let shared = (
			info.s_type,
			STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
			allocator,
		);
		let made = driver.make_object(device, c"vkCreateShaderModule", shared, MODULES, count);
		// SAFETY: the bindings pass a writable handle.
		unsafe { module.write(made.unwrap_or(GARBAGE)) };
		made
			.map(|_| driver.pipeline_objects.modules.push(code.to_vec()))
			.err()
			.unwrap_or(0)
	})
}

pub(super) unsafe extern "system" fn destroy_module(
	device: RawDevice,
	module: RawShaderModule,
	allocator: *const c_void,
) {
	with(|driver| {
		let what = (module, MODULES, allocator);
		driver.release_object(device, what, "vkDestroyShaderModule", "module");
	})
}

pub(super) unsafe extern "system" fn create_sampler(
	device: RawDevice,
	info: *const SamplerCreateInfo,
	allocator: *const c_void,
	sampler: *mut RawSampler,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		let rules = [
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(
				info.anisotropy_enable == 0 && info.compare_enable == 0,
				"no anisotropy or comparison",
			),
			(
				info.unnormalized_coordinates == 0 && info.min_lod <= info.max_lod,
				"levels",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateSampler: {rule}"));
		}
		let count = driver.pipeline_objects.samplers.len();
		let shared = (info.s_type, STRUCTURE_TYPE_SAMPLER_CREATE_INFO, allocator);
		let made = driver.make_object(device, c"vkCreateSampler", shared, SAMPLERS, count);
		// SAFETY: the bindings pass a writable handle.
		unsafe { sampler.write(made.unwrap_or(GARBAGE)) };
		let modes = [
			info.mag_filter,
			info.min_filter,
			info.mipmap_mode,
			info.address_mode_u,
			info.address_mode_v,
			info.address_mode_w,
		];
		made
			.map(|_| driver.pipeline_objects.samplers.push(modes))
			.err()
			.unwrap_or(0)
	})
}

pub(super) unsafe extern "system" fn destroy_sampler(
	device: RawDevice,
	sampler: RawSampler,
	allocator: *const c_void,
) {
	with(|driver| {
		let what = (sampler, SAMPLERS, allocator);
		driver.release_object(device, what, "vkDestroySampler", "sampler");
	})
}

pub(super) unsafe extern "system" fn create_set_layout(
	device: RawDevice,
	info: *const DescriptorSetLayoutCreateInfo,
	allocator: *const c_void,
	layout: *mut RawDescriptorSetLayout,
) -> i32 {
	// SAFETY: the bindings pass a create info with `binding_count` bindings,
	// each with null or one immutable sampler.
	let (info, bindings) = unsafe {
		let info = &*info;
		let bindings = slice::from_raw_parts(info.bindings, info.binding_count as usize);
		let bindings: Vec<Binding> = (bindings.iter())
			.map(|binding| {
				let sampler = binding.immutable_samplers.as_ref().copied();
				let kind = binding.descriptor_type;
				(
					binding.binding,
					kind,
					binding.descriptor_count,
					binding.stage_flags,
					sampler,
				)
			})
			.collect();
		(info, bindings)
	};
	with(|driver| {
		driver.require(info.next.is_null() && info.flags == 0, || {
			"vkCreateDescriptorSetLayout: pNext and flags".into()
		});
		for (index, &(binding, kind, count, stages, sampler)) in bindings.iter().enumerate() {
			let unique = bindings[..index].iter().all(|other| other.0 != binding);
			let samples = kind == COMBINED_IMAGE_SAMPLER;
			let rules = [
				(unique, "bindings numbered once"),
				(count == 1 && stages != 0, "one descriptor, for some stage"),
				(
					sampler.is_none_or(|sampler| samples && driver.live_object(sampler, SAMPLERS)),
					"live immutable samplers, for samplers",
				),
			];
			for (holds, rule) in rules {
				driver.require(holds, || format!("vkCreateDescriptorSetLayout: {rule}"));
			}
		}
		let count = driver.pipeline_objects.set_layouts.len();
		let shared = (
			info.s_type,
			STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
			allocator,
		);
		let call = c"vkCreateDescriptorSetLayout";
		let made = driver.make_object(device, call, shared, SET_LAYOUTS, count);
		// SAFETY: the bindings pass a writable handle.
		unsafe { layout.write(made.unwrap_or(GARBAGE)) };
		made
			.map(|_| driver.pipeline_objects.set_layouts.push(bindings))
			.err()
			.unwrap_or(0)
	})
}

unsafe extern "system" fn destroy_set_layout(
	device: RawDevice,
	layout: RawDescriptorSetLayout,
	allocator: *const c_void,
) {
	with(|driver| {
		let what = (layout, SET_LAYOUTS, allocator);
		driver.release_object(device, what, "vkDestroyDescriptorSetLayout", "set layout");
	})
}

pub(super) unsafe extern "system" fn create_pipeline_layout(
	device: RawDevice,
	info: *const PipelineLayoutCreateInfo,
	allocator: *const c_void,
	layout: *mut RawPipelineLayout,
) -> i32 {
	// SAFETY: the bindings pass a create info with arrays as long as their
	// counts.
	let (info, sets, ranges) = unsafe {
		let info = &*info;
		let sets = slice::from_raw_parts(info.set_layouts, info.set_layout_count as usize);
		let count = info.push_constant_range_count as usize;
		let ranges = slice::from_raw_parts(info.push_constant_ranges, count);
		let ranges: Vec<_> = (ranges.iter())
			.map(|range| [range.stage_flags, range.offset, range.size])
			.collect();
		(info, sets.to_vec(), ranges)
	};
	with(|driver| {
		let rules = [
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(
				sets.iter().all(|&set| driver.live_object(set, SET_LAYOUTS)),
				"live set layouts",
			),
			(
				ranges.iter().all(|&[stages, offset, size]| {
					stages != 0 && size > 0 && (offset | size).is_multiple_of(4) && offset + size <= 128
				}),
				"push constants within the 128 bytes every device has",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreatePipelineLayout: {rule}"));
		}
		let count = driver.pipeline_objects.pipeline_layouts.len();
		let shared = (
			info.s_type,
			STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
			allocator,
		);
		let call = c"vkCreatePipelineLayout";
		let made = driver.make_object(device, call, shared, PIPELINE_LAYOUTS, count);
		// SAFETY: the bindings pass a writable handle.
		unsafe { layout.write(made.unwrap_or(GARBAGE)) };
		(made.map(|_| {
			driver
				.pipeline_objects
				.pipeline_layouts
				.push((sets, ranges))
		}))
		.err()
		.unwrap_or(0)
	})
}

unsafe extern "system" fn destroy_pipeline_layout(
	device: RawDevice,
	layout: RawPipelineLayout,
	allocator: *const c_void,
) {
	with(|driver| {
		let what = (layout, PIPELINE_LAYOUTS, allocator);
		driver.release_object(device, what, "vkDestroyPipelineLayout", "pipeline layout");
	})
}

/// What `info` describes, and the rules it breaks.
///
/// # Safety
///
/// `info` and what it points to are valid, as the specification requires.
unsafe fn describe(info: &GraphicsPipelineCreateInfo) -> (MadePipeline, Vec<&'static str>) {
	// SAFETY: the caller passes a valid create info; each pointer read is
	// checked for null first, and each array is as long as its count.
	unsafe {
		let rendering = info.next.cast::<PipelineRenderingCreateInfo>().as_ref();
		let formats = rendering.map_or(&[][..], |rendering| {
			let count = rendering.color_attachment_count as usize;
			slice::from_raw_parts(rendering.color_attachment_formats, count)
		});
		let stages = slice::from_raw_parts(info.stages, info.stage_count as usize);
		let name = |name: *const c_char| CStr::from_ptr(name).to_string_lossy().into_owned();
		let input = info.vertex_input_state.as_ref();
		let assembly = info.input_assembly_state.as_ref();
		let viewport = info.viewport_state.as_ref();
		let raster = info.rasterization_state.as_ref();
		let multisample = info.multisample_state.as_ref();
		let blend = info.color_blend_state.as_ref();
		let attachments = blend.map_or(&[][..], |blend| {
			slice::from_raw_parts(blend.attachments, blend.attachment_count as usize)
		});
		let dynamic = info.dynamic_state.as_ref().map_or(&[][..], |dynamic| {
			slice::from_raw_parts(dynamic.dynamic_states, dynamic.dynamic_state_count as usize)
		});
		let made = MadePipeline {
			stages: (stages.iter())
				.map(|stage| (stage.stage, stage.module, name(stage.name)))
				.collect(),
			topology: assembly.map_or(-1, |assembly| assembly.topology),
			rasterization: raster.map_or((-1, 0, -1, 0.0), |raster| {
				let (mode, cull, front) = (raster.polygon_mode, raster.cull_mode, raster.front_face);
				(mode, cull, front, raster.line_width)
			}),
			samples: multisample.map_or(0, |multisample| multisample.rasterization_samples),
			blend: attachments.to_vec(),
			dynamic: dynamic.to_vec(),
			formats: formats.to_vec(),
			layout: info.layout,
		};
		let stage_rules = stages.iter().all(|stage| {
			stage.s_type == STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO
				&& stage.next.is_null()
				&& stage.flags == 0
				&& stage.specialization_info.is_null()
		});
		let rules = [
			(
				info.flags == 0 && info.tessellation_state.is_null(),
				"flags and no tessellation",
			),
			(
				rendering.is_some_and(|rendering| {
					rendering.s_type == STRUCTURE_TYPE_PIPELINE_RENDERING_CREATE_INFO
						&& rendering.next.is_null()
						&& rendering.view_mask == 0
						&& rendering.depth_attachment_format == 0
						&& rendering.stencil_attachment_format == 0
				}),
				"dynamic rendering to color alone",
			),
			(
				info.render_pass == 0 && info.base_pipeline_handle == 0,
				"no render pass or base",
			),
			(
				stage_rules && stages.len() == 2 && stages[0].stage != stages[1].stage,
				"a vertex and a fragment stage",
			),
			(
				input.is_some_and(|input| {
					input.s_type == STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO
						&& input.vertex_binding_description_count == 0
						&& input.vertex_attribute_description_count == 0
				}),
				"no vertex input",
			),
			(
				assembly.is_some_and(|assembly| {
					assembly.s_type == STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO
						&& assembly.primitive_restart_enable == 0
				}),
				"input assembly",
			),
			(
				viewport.is_some_and(|viewport| {
					viewport.s_type == STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO
						&& viewport.viewport_count == 1
						&& viewport.scissor_count == 1
				}),
				"one viewport and scissor",
			),
			(
				dynamic.contains(&VIEWPORT) && dynamic.contains(&SCISSOR),
				"the viewport and scissor set when drawing",
			),
			(
				raster.is_some_and(|raster| {
					raster.s_type == STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO
						&& raster.depth_clamp_enable == 0
						&& raster.rasterizer_discard_enable == 0
						&& raster.depth_bias_enable == 0
				}),
				"rasterization",
			),
			(
				multisample.is_some_and(|multisample| {
					multisample.s_type == STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO
						&& multisample.sample_mask.is_null()
				}),
				"multisampling",
			),
			(
				info.depth_stencil_state.is_null() && attachments.len() == formats.len(),
				"a blend state for each color attachment and no depth",
			),
			(
				blend.is_some_and(|blend| {
					blend.s_type == STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO
						&& blend.logic_op_enable == 0
				}),
				"blending without logic operations",
			),
		];
		let broken = rules
			.iter()
			.filter(|(holds, _)| !holds)
			.map(|&(_, rule)| rule);
		(made, broken.collect())
	}
}

pub(super) unsafe extern "system" fn create_pipelines(
	device: RawDevice,
	cache: u64,
	count: u32,
	infos: *const GraphicsPipelineCreateInfo,
	allocator: *const c_void,
	pipelines: *mut RawPipeline,
) -> i32 {
	// SAFETY: the bindings pass one valid create info.
	let (info, (made, broken)) = unsafe { (&*infos, describe(&*infos)) };
	with(|driver| {
		for rule in broken {
			(driver.invalid).push(format!("vkCreateGraphicsPipelines: {rule}"));
		}
		let live = made
			.stages
			.iter()
			.all(|&(_, module, _)| driver.live_object(module, MODULES));
		let rules = [
			(cache == 0 && count == 1, "one pipeline, no cache"),
			(live, "live shader modules"),
			(
				made
					.stages
					.iter()
					.all(|(stage, _, name)| [VERTEX, FRAGMENT].contains(stage) && name == "main"),
				"stages entered at main",
			),
			(
				driver.live_object(made.layout, PIPELINE_LAYOUTS),
				"a live layout",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateGraphicsPipelines: {rule}"));
		}
		let number = driver.pipeline_objects.pipelines.len();
		let shared = (
			info.s_type,
			STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
			allocator,
		);
		let call = c"vkCreateGraphicsPipelines";
		let made_handle = driver.make_object(device, call, shared, PIPELINES, number);
		// SAFETY: the bindings pass room for one handle.
		unsafe { pipelines.write(made_handle.unwrap_or(GARBAGE)) };
		made_handle
			.map(|_| driver.pipeline_objects.pipelines.push(made))
			.err()
			.unwrap_or(0)
	})
}

unsafe extern "system" fn destroy_pipeline(
	device: RawDevice,
	pipeline: RawPipeline,
	allocator: *const c_void,
) {
	with(|driver| {
		let what = (pipeline, PIPELINES, allocator);
		driver.release_object(device, what, "vkDestroyPipeline", "pipeline");
	})
}
