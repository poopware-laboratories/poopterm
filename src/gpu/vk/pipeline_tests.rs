//! Tests of the quad pipeline against the fake driver of `driver_tests`:
//! the sampler, descriptor set layout, pipeline layout and pipeline made
//! and the shader modules they are made from, the order they go in, and
//! failures, and that the fake functions of `pipeline_driver_tests` report
//! each rule they check. The real driver makes the pipeline too, under the
//! validation layer.
use super::{
	buffer_tests::{INTEL, device},
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind},
	device::Device,
	driver_tests::{family, finish, install, with, with_devices},
	image::Format,
	loader_tests::{entry, offscreen},
	pipeline::QuadPipeline,
	pipeline_driver_tests::*,
	pipeline_types::*,
	shader::{QUAD_FRAGMENT, QUAD_VERTEX, words},
	version::api_version,
};
use std::{ptr, rc::Rc};

/// `VkFormat`: `B8G8R8A8_UNORM` and `R8G8B8A8_UNORM`.
const BGRA: i32 = 44;
const RGBA: i32 = 37;

/// What blending the quads over the target takes: straight alpha.
const OVER: PipelineColorBlendAttachmentState = PipelineColorBlendAttachmentState {
	blend_enable: 1,
	// Source alpha, one minus source alpha, add; then one for alpha.
	src_color_blend_factor: 6,
	dst_color_blend_factor: 7,
	color_blend_op: 0,
	src_alpha_blend_factor: 1,
	dst_alpha_blend_factor: 7,
	alpha_blend_op: 0,
	color_write_mask: 0xf,
};

#[test]
fn the_quad_pipeline_draws_the_quad_shaders_over_the_target() {
	let device = device(&INTEL, |_| {});
	let pipeline = QuadPipeline::new(&device, Format::Bgra).unwrap();
	assert_eq!(
		(pipeline.handle(), pipeline.layout(), pipeline.set_layout()),
		(0x64_0001, 0x63_0001, 0x62_0001)
	);
	// The shader modules go once the pipeline is made.
	let released = with(|driver| std::mem::take(&mut driver.released));
	assert_eq!(released, ["module 0x600001", "module 0x600002"]);
	drop(pipeline);
	drop(device);
	let driver = finish();
	let objects = driver.pipeline_objects;
	// Nearest texels, clamped: the shaders fetch texels by index.
	assert_eq!(objects.samplers, [[0, 0, 0, 2, 2, 2]]);
	// The quads for both stages, and the atlas sampled by the fragments.
	let bindings: [Binding; 2] = [
		(0, STORAGE_BUFFER, 1, VERTEX | FRAGMENT, None),
		(1, COMBINED_IMAGE_SAMPLER, 1, FRAGMENT, Some(0x61_0001)),
	];
	assert_eq!(objects.set_layouts, [bindings]);
	// The target's width and height, two floats, for the vertex stage.
	assert_eq!(
		objects.pipeline_layouts,
		[(vec![0x62_0001], vec![[VERTEX, 0, 8]])]
	);
	let expected = MadePipeline {
		stages: vec![
			(VERTEX, 0x60_0001, "main".into()),
			(FRAGMENT, 0x60_0002, "main".into()),
		],
		// A triangle list, filled, neither face culled.
		topology: 3,
		rasterization: (0, 0, 0, 1.0),
		samples: 1,
		blend: vec![OVER],
		dynamic: vec![VIEWPORT, SCISSOR],
		formats: vec![BGRA],
		layout: 0x63_0001,
	};
	assert_eq!(objects.pipelines, [expected]);
	let code = [QUAD_VERTEX, QUAD_FRAGMENT].map(|bytes| words(bytes).unwrap());
	assert_eq!(objects.modules, code);
	// The rest goes in the reverse order it was made in.
	let expected = [
		"pipeline 0x640001",
		"pipeline layout 0x630001",
		"set layout 0x620001",
		"sampler 0x610001",
	];
	assert_eq!(driver.released, expected);
}

#[test]
fn the_pipeline_draws_to_the_format_asked_for() {
	let device = device(&INTEL, |_| {});
	drop(QuadPipeline::new(&device, Format::Rgba).unwrap());
	drop(device);
	assert_eq!(finish().pipeline_objects.pipelines[0].formats, [RGBA]);
}

#[test]
fn a_failure_releases_what_was_made_before_it() {
	let steps = [
		(c"vkCreateSampler", &[][..]),
		(c"vkCreateDescriptorSetLayout", &["sampler 0x610001"]),
		(
			c"vkCreatePipelineLayout",
			&["set layout 0x620001", "sampler 0x610001"],
		),
		(
			c"vkCreateShaderModule",
			&[
				"pipeline layout 0x630001",
				"set layout 0x620001",
				"sampler 0x610001",
			],
		),
		(
			c"vkCreateGraphicsPipelines",
			&[
				"module 0x600001",
				"module 0x600002",
				"pipeline layout 0x630001",
				"set layout 0x620001",
				"sampler 0x610001",
			],
		),
	];
	for (failing, released) in steps {
		let device = device(&INTEL, |driver| driver.fails.push((failing, -1)));
		let made = QuadPipeline::new(&device, Format::Bgra);
		assert_eq!(made.err(), Some(Error::Failed(-1)), "{failing:?}");
		drop(device);
		let driver = finish();
		assert_eq!(driver.released, released, "{failing:?}");
		assert!(driver.pipeline_objects.live.is_empty(), "{failing:?}");
	}
}

#[test]
fn shaders_that_are_not_spirv_make_nothing() {
	let device = device(&INTEL, |_| {});
	for shaders in [[&[1, 2, 3][..], QUAD_FRAGMENT], [QUAD_VERTEX, &[][..]]] {
		let made = QuadPipeline::build(&device, Format::Bgra, shaders);
		assert_eq!(made.err(), Some(Error::Spirv));
	}
	drop(device);
	let driver = finish();
	assert!(driver.pipeline_objects.samplers.is_empty());
	assert!(driver.released.is_empty());
}

#[test]
fn a_device_missing_a_pipeline_function_is_not_made() {
	let names = [
		c"vkCreateShaderModule",
		c"vkDestroyShaderModule",
		c"vkCreateSampler",
		c"vkDestroySampler",
		c"vkCreateDescriptorSetLayout",
		c"vkDestroyDescriptorSetLayout",
		c"vkCreatePipelineLayout",
		c"vkDestroyPipelineLayout",
		c"vkCreateGraphicsPipelines",
		c"vkDestroyPipeline",
	];
	for name in names {
		let mut driver = with_devices(&[Described {
			name: "lacking".into(),
			kind: Kind::Integrated,
			api: api_version(1, 3),
			families: vec![family(GRAPHICS, 1)],
			swapchain: true,
		}]);
		driver.lacks.push(name);
		let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
		let physical = instance.physical_devices().unwrap()[0];
		let made = Device::new(instance, physical, 0);
		assert_eq!(made.err(), Some(Error::Missing(name)));
		assert_eq!(finish().destroyed_devices.len(), 1, "{name:?}");
	}
}

#[test]
fn the_fake_driver_reports_broken_pipeline_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing.
	let device = device(&INTEL, |_| {});
	let handle = with(|driver| driver.live_devices[0]);
	let reported = || with(|driver| std::mem::take(&mut driver.invalid));
	let code = words(QUAD_VERTEX).unwrap();
	let module = |code: &[u32], size| ShaderModuleCreateInfo {
		s_type: STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		code_size: size,
		code: code.as_ptr(),
	};
	let mut out = 0;
	// SAFETY: the fake reads only what the pointers point to.
	unsafe {
		create_module(
			handle,
			&module(&code[1..], 4 * (code.len() - 1)),
			ptr::null(),
			&mut out,
		);
		create_module(handle, &module(&code, 6), ptr::null(), &mut out);
		destroy_module(handle, 0x60_0009, ptr::null());
		destroy_sampler(handle, 0x60_0001, ptr::null());
	}
	let expected = [
		"vkCreateShaderModule: SPIR-V",
		"vkCreateShaderModule: a size in words",
		"vkDestroyShaderModule: 0x600009, which is not live",
		"vkDestroySampler: 0x600001, which is not live",
	];
	assert_eq!(reported(), expected);
	let sampler = |anisotropy_enable, unnormalized_coordinates| SamplerCreateInfo {
		s_type: STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		mag_filter: 0,
		min_filter: 0,
		mipmap_mode: 0,
		address_mode_u: 2,
		address_mode_v: 2,
		address_mode_w: 2,
		mip_lod_bias: 0.0,
		anisotropy_enable,
		max_anisotropy: 1.0,
		compare_enable: 0,
		compare_op: 0,
		min_lod: 0.0,
		max_lod: 0.0,
		border_color: 0,
		unnormalized_coordinates,
	};
	let binding =
		|binding, descriptor_type, immutable: *const RawSampler| DescriptorSetLayoutBinding {
			binding,
			descriptor_type,
			descriptor_count: 1,
			stage_flags: FRAGMENT,
			immutable_samplers: immutable,
		};
	let set_layout = |bindings: &[DescriptorSetLayoutBinding]| DescriptorSetLayoutCreateInfo {
		s_type: STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		binding_count: bindings.len() as u32,
		bindings: bindings.as_ptr(),
	};
	let dead: RawSampler = 0x61_0009;
	let ranges = |offset, size| {
		[PushConstantRange {
			stage_flags: VERTEX,
			offset,
			size,
		}]
	};
	let pipeline_layout =
		|sets: &[RawDescriptorSetLayout], ranges: &[PushConstantRange]| PipelineLayoutCreateInfo {
			s_type: STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			set_layout_count: sets.len() as u32,
			set_layouts: sets.as_ptr(),
			push_constant_range_count: ranges.len() as u32,
			push_constant_ranges: ranges.as_ptr(),
		};
	// SAFETY: as above.
	unsafe {
		create_sampler(handle, &sampler(1, 0), ptr::null(), &mut out);
		create_sampler(handle, &sampler(0, 1), ptr::null(), &mut out);
		let twice = [
			binding(0, STORAGE_BUFFER, ptr::null()),
			binding(0, STORAGE_BUFFER, ptr::null()),
		];
		create_set_layout(handle, &set_layout(&twice), ptr::null(), &mut out);
		let dead_sampler = [binding(0, COMBINED_IMAGE_SAMPLER, &dead)];
		create_set_layout(handle, &set_layout(&dead_sampler), ptr::null(), &mut out);
		create_pipeline_layout(
			handle,
			&pipeline_layout(&[0x62_0009], &[]),
			ptr::null(),
			&mut out,
		);
		create_pipeline_layout(
			handle,
			&pipeline_layout(&[], &ranges(124, 8)),
			ptr::null(),
			&mut out,
		);
		create_pipeline_layout(
			handle,
			&pipeline_layout(&[], &ranges(0, 6)),
			ptr::null(),
			&mut out,
		);
	}
	let expected = [
		"vkCreateSampler: no anisotropy or comparison",
		"vkCreateSampler: levels",
		"vkCreateDescriptorSetLayout: bindings numbered once",
		"vkCreateDescriptorSetLayout: live immutable samplers, for samplers",
		"vkCreatePipelineLayout: live set layouts",
		"vkCreatePipelineLayout: push constants within the 128 bytes every device has",
		"vkCreatePipelineLayout: push constants within the 128 bytes every device has",
	];
	assert_eq!(reported(), expected);
	// A pipeline breaking every rule its states can: no rendering info,
	// one stage of a dead module entered elsewhere, vertex input, primitive
	// restart, two viewports set when made, depth clamping, a sample mask,
	// a depth state, no blend state, a cache and a render pass, and a dead
	// layout.
	let entry = c"start";
	let stage = PipelineShaderStageCreateInfo {
		s_type: STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		stage: VERTEX,
		module: 0x60_0009,
		name: entry.as_ptr(),
		specialization_info: ptr::null(),
	};
	let input = PipelineVertexInputStateCreateInfo {
		s_type: STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		vertex_binding_description_count: 1,
		vertex_binding_descriptions: ptr::null(),
		vertex_attribute_description_count: 0,
		vertex_attribute_descriptions: ptr::null(),
	};
	let assembly = PipelineInputAssemblyStateCreateInfo {
		s_type: STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		topology: 3,
		primitive_restart_enable: 1,
	};
	let viewport = PipelineViewportStateCreateInfo {
		s_type: STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		viewport_count: 2,
		viewports: ptr::null(),
		scissor_count: 1,
		scissors: ptr::null(),
	};
	let raster = PipelineRasterizationStateCreateInfo {
		s_type: STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		depth_clamp_enable: 1,
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
	let mask = 1u32;
	let multisample = PipelineMultisampleStateCreateInfo {
		s_type: STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		rasterization_samples: 1,
		sample_shading_enable: 0,
		min_sample_shading: 0.0,
		sample_mask: &mask,
		alpha_to_coverage_enable: 0,
		alpha_to_one_enable: 0,
	};
	let info = GraphicsPipelineCreateInfo {
		s_type: STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		stage_count: 1,
		stages: &stage,
		vertex_input_state: &input,
		input_assembly_state: &assembly,
		tessellation_state: ptr::null(),
		viewport_state: &viewport,
		rasterization_state: &raster,
		multisample_state: &multisample,
		depth_stencil_state: ptr::from_ref(&mask).cast(),
		color_blend_state: ptr::null(),
		dynamic_state: ptr::null(),
		layout: 0x63_0009,
		render_pass: 1,
		subpass: 0,
		base_pipeline_handle: 0,
		base_pipeline_index: -1,
	};
	// SAFETY: as above.
	unsafe { create_pipelines(handle, 7, 1, &info, ptr::null(), &mut out) };
	let expected = [
		"vkCreateGraphicsPipelines: dynamic rendering to color alone",
		"vkCreateGraphicsPipelines: no render pass or base",
		"vkCreateGraphicsPipelines: a vertex and a fragment stage",
		"vkCreateGraphicsPipelines: no vertex input",
		"vkCreateGraphicsPipelines: input assembly",
		"vkCreateGraphicsPipelines: one viewport and scissor",
		"vkCreateGraphicsPipelines: the viewport and scissor set when drawing",
		"vkCreateGraphicsPipelines: rasterization",
		"vkCreateGraphicsPipelines: multisampling",
		"vkCreateGraphicsPipelines: a blend state for each color attachment and no depth",
		"vkCreateGraphicsPipelines: blending without logic operations",
		"vkCreateGraphicsPipelines: one pipeline, no cache",
		"vkCreateGraphicsPipelines: live shader modules",
		"vkCreateGraphicsPipelines: stages entered at main",
		"vkCreateGraphicsPipelines: a live layout",
	];
	assert_eq!(reported(), expected);
	// Destroying the device while one of them lives.
	with(|driver| driver.pipeline_objects.live = vec![0x64_0001]);
	drop(device);
	assert_eq!(
		reported(),
		["vkDestroyDevice while shaders, samplers, layouts or pipelines are live"]
	);
	with(|driver| driver.pipeline_objects.live.clear());
	finish();
}

#[test]
fn a_real_device_makes_the_quad_pipeline() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	for format in [Format::Bgra, Format::Rgba] {
		let pipeline = QuadPipeline::new(&device, format).expect("pipeline");
		assert!(pipeline.handle() != 0 && pipeline.layout() != 0 && pipeline.set_layout() != 0);
	}
}
