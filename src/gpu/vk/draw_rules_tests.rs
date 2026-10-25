//! Tests that the fake drawing functions of `draw_driver_tests` report
//! each rule they check, broken one at a time; were they blind to them,
//! the tests of `draw_tests` would prove nothing.
use super::{
	buffer::Buffer,
	buffer_tests::{INTEL, device},
	commands::Commands,
	descriptor_driver_tests::SETS,
	descriptors::QuadSet,
	draw::Draw,
	draw_driver_tests::*,
	draw_types::*,
	driver_tests::{finish, with},
	image::{Format, Image},
	memory::Usage,
	pipeline::QuadPipeline,
	submit_types::RawCommandBuffer,
	timeline::Timeline,
};
use std::{mem::take, num::NonZeroUsize, ptr, rc::Rc};

/// `VK_BUFFER_USAGE_TRANSFER_SRC_BIT`, which image usage shares;
/// `VK_IMAGE_USAGE_SAMPLED_BIT` and `_COLOR_ATTACHMENT_BIT`; and
/// `VK_BUFFER_USAGE_STORAGE_BUFFER_BIT`.
const TRANSFER_SRC: u32 = 1;
const SAMPLED: u32 = 4;
const COLOR_ATTACHMENT: u32 = 0x10;
const STORAGE: u32 = 0x20;
/// `VkShaderStageFlagBits`: vertex and fragment.
const VERTEX: u32 = 0x1;
const FRAGMENT: u32 = 0x10;
/// The first command buffer, as the bindings see it.
const BUFFER: RawCommandBuffer = 0x7c_0001 as RawCommandBuffer;

/// What the rules are broken around: a pipeline and its set, room for
/// three quads, an atlas and a 16×8 target, and buffers and images made
/// for something else.
struct Scene {
	pipeline: Rc<QuadPipeline>,
	set: Rc<QuadSet>,
	quads: Rc<Buffer>,
	atlas: Rc<Image>,
	target: Rc<Image>,
	unstored: Rc<Buffer>,
	rgba: Rc<Image>,
}

fn scene() -> (Commands, Scene) {
	let device = device(&INTEL, |_| {});
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let pipeline = Rc::new(QuadPipeline::new(&device, Format::Bgra).unwrap());
	let buffer = |usage| {
		let size = NonZeroUsize::new(3 * 48).unwrap();
		Rc::new(Buffer::new(&device, size, usage, Usage::Upload).unwrap())
	};
	let image = |format, usage| Rc::new(Image::new(&device, format, [16, 8, 1], usage).unwrap());
	let scene = Scene {
		set: Rc::new(QuadSet::new(&pipeline).unwrap()),
		pipeline,
		quads: buffer(STORAGE),
		atlas: image(Format::Bgra, SAMPLED),
		target: image(Format::Bgra, COLOR_ATTACHMENT | TRANSFER_SRC),
		unstored: buffer(TRANSFER_SRC),
		rgba: image(Format::Rgba, COLOR_ATTACHMENT),
	};
	(Commands::new(&timeline).unwrap(), scene)
}

impl Scene {
	fn draw(&self) -> Draw<'_> {
		Draw {
			set: &self.set,
			quads: &self.quads,
			count: 3,
			atlas: &self.atlas,
			target: &self.target,
		}
	}
}

/// A write of one descriptor of `set`'s `binding`, from `buffer` or
/// `image`.
fn write(
	set: u64,
	binding: u32,
	buffer: Option<&DescriptorBufferInfo>,
	image: Option<&DescriptorImageInfo>,
) -> WriteDescriptorSet {
	WriteDescriptorSet {
		s_type: STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
		next: ptr::null(),
		dst_set: set,
		dst_binding: binding,
		dst_array_element: 0,
		descriptor_count: 1,
		descriptor_type: if binding == 0 { 7 } else { 1 },
		image_info: image.map_or(ptr::null(), ptr::from_ref),
		buffer_info: buffer.map_or(ptr::null(), ptr::from_ref),
		texel_buffer_view: ptr::null(),
	}
}

fn buffer_info(buffer: u64, offset: u64) -> DescriptorBufferInfo {
	DescriptorBufferInfo {
		buffer,
		offset,
		range: !0,
	}
}

fn image_info(image_view: u64, image_layout: i32) -> DescriptorImageInfo {
	DescriptorImageInfo {
		sampler: 0,
		image_view,
		image_layout,
	}
}

/// The breaks the fake reported since last asked.
fn reported() -> Vec<String> {
	with(|driver| take(&mut driver.invalid))
}

#[test]
fn the_fake_driver_reports_broken_update_rules() {
	let (mut commands, scene) = scene();
	let device = with(|driver| driver.live_devices[0]);
	let set = scene.set.handle();
	let quads = buffer_info(scene.quads.handle(), 0);
	let atlas = image_info(scene.atlas.view(), 5);
	let cases = [
		WriteDescriptorSet {
			s_type: 0,
			..write(set, 0, Some(&quads), None)
		},
		write(SETS + 9, 0, Some(&quads), None),
		WriteDescriptorSet {
			descriptor_count: 2,
			..write(set, 0, Some(&quads), None)
		},
		write(set, 0, Some(&buffer_info(scene.unstored.handle(), 0)), None),
		write(set, 0, Some(&buffer_info(scene.quads.handle(), 16)), None),
		write(set, 1, None, Some(&image_info(scene.target.view(), 5))),
		write(set, 1, None, Some(&image_info(scene.atlas.view(), 7))),
		write(set, 2, None, Some(&atlas)),
		// A buffer descriptor without its buffer.
		write(set, 0, None, None),
	];
	// SAFETY: each write and what it points to outlive the call.
	unsafe {
		for case in &cases {
			update_sets(device, 1, case, 0, ptr::null());
		}
		update_sets(
			device,
			1,
			&write(set, 0, Some(&quads), None),
			1,
			ptr::null(),
		);
	}
	let rules = [
		"sType and pNext",
		"a set of a live pool",
		"one descriptor",
		"a live storage buffer",
		"the quads from the start",
		"a live sampled view",
		"read only by shaders",
		"a binding of the quad set layout",
		"a binding of the quad set layout",
		"no copies",
	];
	assert_eq!(
		reported(),
		rules.map(|rule| format!("vkUpdateDescriptorSets: {rule}"))
	);
	// Updating a set that pending commands draw with, and submitting
	// commands whose set was updated after they bound it.
	let mut recording = commands.record().unwrap();
	recording.draw(scene.draw()).unwrap();
	recording.submit().unwrap();
	// SAFETY: as above.
	unsafe {
		update_sets(
			device,
			1,
			&write(set, 1, None, Some(&atlas)),
			0,
			ptr::null(),
		)
	};
	assert_eq!(
		reported(),
		["vkUpdateDescriptorSets: a set no pending commands use"]
	);
	with(|driver| driver.counters[0] = 1);
	let mut recording = commands.record().unwrap();
	recording.draw(scene.draw()).unwrap();
	// SAFETY: as above.
	unsafe {
		update_sets(
			device,
			1,
			&write(set, 1, None, Some(&atlas)),
			0,
			ptr::null(),
		)
	};
	assert!(recording.submit().is_err());
	assert_eq!(
		reported(),
		["vkQueueSubmit2: no set updated since it was bound"]
	);
	drop((commands, scene));
	finish();
}

fn attachment(image_view: u64) -> RenderingAttachmentInfo {
	RenderingAttachmentInfo {
		s_type: STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO,
		next: ptr::null(),
		image_view,
		image_layout: 2,
		resolve_mode: 0,
		resolve_image_view: 0,
		resolve_image_layout: 0,
		load_op: 0,
		store_op: 0,
		clear_value: [0; 4],
	}
}

fn rendering(attachment: &RenderingAttachmentInfo) -> RenderingInfo {
	RenderingInfo {
		s_type: STRUCTURE_TYPE_RENDERING_INFO,
		next: ptr::null(),
		flags: 0,
		render_area: Rect2D {
			offset: [0, 0],
			extent: [16, 8],
		},
		layer_count: 1,
		view_mask: 0,
		color_attachment_count: 1,
		color_attachments: attachment,
		depth_attachment: ptr::null(),
		stencil_attachment: ptr::null(),
	}
}

#[test]
fn the_fake_driver_reports_broken_rendering_rules() {
	let (mut commands, scene) = scene();
	let target = attachment(scene.target.view());
	let infos = [
		RenderingInfo {
			s_type: 0,
			..rendering(&target)
		},
		RenderingInfo {
			layer_count: 2,
			..rendering(&target)
		},
		RenderingInfo {
			color_attachment_count: 2,
			..rendering(&target)
		},
		rendering(&RenderingAttachmentInfo {
			next: ptr::from_ref(&target).cast(),
			..attachment(scene.target.view())
		}),
		rendering(&attachment(scene.atlas.view())),
		rendering(&RenderingAttachmentInfo {
			image_layout: 5,
			..attachment(scene.target.view())
		}),
		rendering(&RenderingAttachmentInfo {
			resolve_mode: 1,
			..attachment(scene.target.view())
		}),
		rendering(&RenderingAttachmentInfo {
			store_op: 1,
			..attachment(scene.target.view())
		}),
		RenderingInfo {
			render_area: Rect2D {
				offset: [1, 0],
				extent: [16, 8],
			},
			..rendering(&target)
		},
	];
	let mut recording = commands.record().unwrap();
	// Leaves the target in the color attachment layout and the atlas
	// readable, the pipeline, set and dynamic state bound.
	recording.draw(scene.draw()).unwrap();
	let layout = scene.pipeline.layout();
	let set = scene.set.handle();
	let viewport = Viewport {
		x: 0.0,
		y: 0.0,
		width: 16.0,
		height: 8.0,
		min_depth: 0.0,
		max_depth: 1.0,
	};
	let size = [16.0f32, 8.0];
	// SAFETY: every struct and array outlives its call; the bad handles
	// are only compared.
	unsafe {
		for info in &infos {
			begin_rendering(BUFFER, info);
			end_rendering(BUFFER);
		}
		end_rendering(BUFFER);
		begin_rendering(BUFFER, &rendering(&target));
		begin_rendering(BUFFER, &rendering(&target));
		bind_pipeline(BUFFER, 1, scene.pipeline.handle());
		set_viewport(BUFFER, 0, 2, &viewport);
		set_viewport(
			BUFFER,
			0,
			1,
			&Viewport {
				width: 0.0,
				..viewport
			},
		);
		set_viewport(
			BUFFER,
			0,
			1,
			&Viewport {
				max_depth: 0.5,
				..viewport
			},
		);
		set_scissor(
			BUFFER,
			1,
			1,
			&Rect2D {
				offset: [0, 0],
				extent: [16, 8],
			},
		);
		set_scissor(
			BUFFER,
			0,
			1,
			&Rect2D {
				offset: [-1, 0],
				extent: [16, 8],
			},
		);
		bind_sets(BUFFER, 1, layout, 0, 1, &set, 0, ptr::null());
		bind_sets(BUFFER, 0, layout + 9, 0, 1, &set, 0, ptr::null());
		bind_sets(BUFFER, 0, layout, 0, 1, &(SETS + 9), 0, ptr::null());
		bind_sets(BUFFER, 0, layout, 0, 1, &set, 1, ptr::null());
		bind_sets(BUFFER, 0, layout, 0, 2, [set, set].as_ptr(), 0, ptr::null());
		push(BUFFER, layout + 9, VERTEX, 0, 8, size.as_ptr().cast());
		push(BUFFER, layout, VERTEX, 0, 6, size.as_ptr().cast());
		push(BUFFER, layout, VERTEX, 4, 8, size.as_ptr().cast());
		push(BUFFER, layout, FRAGMENT, 0, 8, size.as_ptr().cast());
		draw(BUFFER, 6, 4, 0, 0);
		draw(BUFFER, 6, 1, 1, 0);
		end_rendering(BUFFER);
	}
	let begin = |rule| format!("vkCmdBeginRendering: {rule}");
	let mut expected: Vec<String> = [
		"sType, pNext and flags",
		"one layer",
		"one color attachment",
		"attachment sType and pNext",
		// The atlas: not an attachment, nor in that layout.
		"a live color attachment",
		"in the color attachment layout",
		"in the color attachment layout",
		"no resolve",
		"stored",
		"an area within the image",
	]
	.map(begin)
	.into();
	expected.extend(
		[
			"vkCmdEndRendering: rendering",
			"vkCmdBeginRendering: not rendering already",
			"vkCmdBindPipeline: a live graphics pipeline",
			"vkCmdSetViewport: one viewport",
			"vkCmdSetViewport: a positive size",
			"vkCmdSetViewport: depths 0 to 1",
			"vkCmdSetScissor: one scissor",
			"vkCmdSetScissor: a positive offset",
			"vkCmdBindDescriptorSets: the first set for graphics",
			"vkCmdBindDescriptorSets: a live pipeline layout",
			"vkCmdBindDescriptorSets: a set of the layout's set layout",
			"vkCmdBindDescriptorSets: a set of a live pool",
			"vkCmdBindDescriptorSets: a set of the layout's set layout",
			"vkCmdBindDescriptorSets: no dynamic offsets",
			"vkCmdBindDescriptorSets: one set",
			"vkCmdPushConstants: a live pipeline layout",
			"vkCmdPushConstants: within a range of the layout for those stages",
			"vkCmdPushConstants: whole words",
			"vkCmdPushConstants: within a range of the layout for those stages",
			"vkCmdPushConstants: within a range of the layout for those stages",
			"vkCmdDraw: every quad drawn in the bound buffer",
			"vkCmdDraw: vertices from the first",
		]
		.map(String::from),
	);
	assert_eq!(reported(), expected);
	drop(recording);
	drop((commands, scene));
	finish();
}

#[test]
fn the_fake_driver_reports_draws_without_what_they_need() {
	let (mut commands, scene) = scene();
	let mut recording = commands.record().unwrap();
	// The set written, then a recording with nothing bound.
	recording.draw(scene.draw()).unwrap();
	drop(recording);
	let target = attachment(scene.target.view());
	let rgba = attachment(scene.rgba.view());
	let mut recording = commands.record().unwrap();
	recording.transition(&scene.target, super::recording::Layout::ColorAttachment);
	recording.transition(&scene.rgba, super::recording::Layout::ColorAttachment);
	// SAFETY: every struct outlives its call.
	unsafe {
		draw(BUFFER, 6, 1, 0, 0);
		begin_rendering(BUFFER, &rendering(&target));
		draw(BUFFER, 6, 1, 0, 0);
	}
	let draw_rule = |rule| format!("vkCmdDraw: {rule}");
	let nothing = [
		"a bound pipeline",
		"the push constants, viewport and scissor set",
		"a target of the pipeline's format",
		"a set bound with the pipeline's layout",
		"an atlas read only by shaders",
		"every quad drawn in the bound buffer",
	];
	let mut expected: Vec<String> = ["rendering"]
		.into_iter()
		.chain(nothing)
		.map(draw_rule)
		.collect();
	expected.extend(nothing.map(draw_rule));
	assert_eq!(reported(), expected);
	// The same draw, all set up, to a target of another format, and ending
	// the commands while rendering.
	let layout = scene.pipeline.layout();
	let set = scene.set.handle();
	let viewport = Viewport {
		x: 0.0,
		y: 0.0,
		width: 16.0,
		height: 8.0,
		min_depth: 0.0,
		max_depth: 1.0,
	};
	let area = Rect2D {
		offset: [0, 0],
		extent: [16, 8],
	};
	let size = [16.0f32, 8.0];
	// SAFETY: as above.
	unsafe {
		end_rendering(BUFFER);
		begin_rendering(BUFFER, &rendering(&rgba));
		bind_pipeline(BUFFER, 0, scene.pipeline.handle());
		set_viewport(BUFFER, 0, 1, &viewport);
		set_scissor(BUFFER, 0, 1, &area);
		bind_sets(BUFFER, 0, layout, 0, 1, &set, 0, ptr::null());
		push(BUFFER, layout, VERTEX, 0, 8, size.as_ptr().cast());
		draw(BUFFER, 6, 1, 0, 0);
	}
	let rules = [
		"vkCmdDraw: a target of the pipeline's format",
		"vkCmdDraw: an atlas read only by shaders",
	];
	assert_eq!(reported(), rules);
	assert!(recording.submit().is_err());
	assert_eq!(reported(), ["vkEndCommandBuffer: while rendering"]);
	drop((commands, scene));
	finish();
}
