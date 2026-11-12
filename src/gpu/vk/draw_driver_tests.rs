//! The fake driver's descriptor update and drawing functions, for
//! `draw_tests`: each checks the specification's valid usage rules, among
//! them that no recorded or pending commands use a set being updated, that
//! a write is of a binding its set's layout has, that rendering goes to a
//! view in the color attachment layout of the pipeline's format, and that
//! a draw finds its atlas and image readable and its quads in the bound
//! buffer, and records what it did.
use super::{
	descriptor_driver_tests::{POOLS, SETS},
	descriptor_types::RawDescriptorSet,
	draw_types::*,
	driver_tests::erase,
	driver_tests::{Driver, with},
	pipeline_driver_tests::{
		COMBINED_IMAGE_SAMPLER, PIPELINE_LAYOUTS, PIPELINES, SET_LAYOUTS, STORAGE_BUFFER,
	},
	pipeline_types::{RawPipeline, RawPipelineLayout},
	submit_types::RawCommandBuffer,
	types::*,
};
use std::{
	ffi::{CStr, c_void},
	slice,
};

/// What a command buffer has set up for drawing since it began.
#[derive(Debug, Default)]
pub(super) struct Drawing {
	/// The view being rendered to, between beginning and ending rendering.
	pub rendering: Option<RawImageView>,
	pub pipeline: Option<RawPipeline>,
	/// The bound set and the pipeline layout it was bound with.
	pub set: Option<(RawDescriptorSet, RawPipelineLayout)>,
	/// The same of set 1, the image's.
	pub texture: Option<(RawDescriptorSet, RawPipelineLayout)>,
	pub pushed: bool,
	pub viewport: bool,
	pub scissor: bool,
	/// Every set bound, which updating invalidates the buffer.
	pub sets: Vec<RawDescriptorSet>,
	/// Whether a set was updated since it was bound, so that submitting
	/// the buffer is invalid.
	pub stale: bool,
}

/// `VK_BUFFER_USAGE_STORAGE_BUFFER_BIT`, and `VK_IMAGE_USAGE_SAMPLED_BIT`
/// and `_COLOR_ATTACHMENT_BIT`.
const STORAGE: u32 = 0x20;
const SAMPLED: u32 = 0x4;
const COLOR_ATTACHMENT: u32 = 0x10;
/// `VkImageLayout`: a color attachment, and read only by shaders.
const ATTACHMENT_LAYOUT: i32 = 2;
const READ_ONLY_LAYOUT: i32 = 5;
/// `VK_WHOLE_SIZE`.
const WHOLE_SIZE: u64 = !0;
/// `VK_ATTACHMENT_LOAD_OP_DONT_CARE`, the last load op, and
/// `VK_ATTACHMENT_STORE_OP_STORE`.
const LAST_LOAD_OP: i32 = 2;
const STORE: i32 = 0;
/// Bytes a quad in the storage buffer.
const QUAD: u64 = 48;

/// The fake's update and drawing function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	Some(match name.to_bytes() {
		b"vkUpdateDescriptorSets" => erase!(UpdateDescriptorSets, update_sets),
		b"vkCmdBeginRendering" => erase!(CmdBeginRendering, begin_rendering),
		b"vkCmdEndRendering" => erase!(CmdEndRendering, end_rendering),
		b"vkCmdBindPipeline" => erase!(CmdBindPipeline, bind_pipeline),
		b"vkCmdSetViewport" => erase!(CmdSetViewport, set_viewport),
		b"vkCmdSetScissor" => erase!(CmdSetScissor, set_scissor),
		b"vkCmdBindDescriptorSets" => erase!(CmdBindDescriptorSets, bind_sets),
		b"vkCmdPushConstants" => erase!(CmdPushConstants, push),
		b"vkCmdDraw" => erase!(CmdDraw, draw),
		_ => return super::swapchain_driver_tests::function(name),
	})
}

impl Driver {
	/// The image of the live view `view`, if it is one.
	pub(super) fn viewed(&self, view: RawImageView) -> Option<RawImage> {
		let made = self.views.get(view.0.wrapping_sub(0xf0_0001) as usize)?;
		self.live_views.contains(&view).then_some(made.image)
	}

	/// Whether `set` was allocated from a live pool.
	fn live_set(&self, set: RawDescriptorSet) -> bool {
		let made = self
			.pipeline_objects
			.sets
			.get(set.0.wrapping_sub(SETS + 1) as usize);
		made.is_some_and(|&(pool, _)| self.live_object(pool.0, POOLS))
	}

	/// What `buffer` has set up for drawing, breaking the test unless it
	/// is being recorded.
	fn drawing(&mut self, buffer: RawCommandBuffer, call: &str) -> Option<&mut Drawing> {
		self.check_recording(buffer, call);
		self
			.command_buffer(buffer)
			.map(|buffer| &mut buffer.drawing)
	}

	/// Whether the layout of `set` has a binding numbered `binding` of
	/// `kind`.
	fn declares(&self, set: RawDescriptorSet, binding: u32, kind: i32) -> bool {
		let sets = &self.pipeline_objects.sets;
		let layout = sets
			.get(set.0.wrapping_sub(SETS + 1) as usize)
			.map(|&(_, layout)| layout);
		let layouts = &self.pipeline_objects.set_layouts;
		let bindings =
			layout.and_then(|layout| layouts.get(layout.0.wrapping_sub(SET_LAYOUTS + 1) as usize));
		bindings.is_some_and(|bindings| {
			bindings
				.iter()
				.any(|&(at, of, ..)| (at, of) == (binding, kind))
		})
	}

	/// Breaks the test unless the descriptor in `write` suits its set's
	/// layout, and returns what it points to and its range or layout.
	fn check_write(&mut self, write: &WriteDescriptorSet) -> (u64, u64) {
		let one = write.dst_array_element == 0 && write.descriptor_count == 1;
		let declared = self.declares(write.dst_set, write.dst_binding, write.descriptor_type);
		let (rules, written) = match (declared, write.descriptor_type) {
			(true, STORAGE_BUFFER) if !write.buffer_info.is_null() => {
				// SAFETY: the bindings pass one buffer info for one descriptor.
				let info = unsafe { &*write.buffer_info };
				let made = self.made_buffer(info.buffer);
				let within = made.is_some_and(|(size, _)| info.range == WHOLE_SIZE || info.range <= size);
				let rules = [
					(
						made.is_some_and(|(_, usage)| usage & STORAGE != 0),
						"a live storage buffer",
					),
					(info.offset == 0 && within, "the quads from the start"),
				];
				(rules, (info.buffer.0, info.range))
			}
			(true, COMBINED_IMAGE_SAMPLER) if !write.image_info.is_null() => {
				// SAFETY: the bindings pass one image info for one descriptor.
				let info = unsafe { &*write.image_info };
				let image = self.viewed(info.image_view);
				let sampled = image.and_then(|image| self.made_image(image));
				let rules = [
					(
						sampled.is_some_and(|(.., usage)| usage & SAMPLED != 0),
						"a live sampled view",
					),
					(
						info.image_layout == READ_ONLY_LAYOUT,
						"read only by shaders",
					),
				];
				(rules, (info.image_view.0, info.image_layout as u64))
			}
			_ => (
				[(false, "a binding of the set's layout"), (true, "")],
				(0, 0),
			),
		};
		let pending = (self.command_buffers.iter())
			.any(|buffer| self.pending(buffer) && buffer.drawing.sets.contains(&write.dst_set));
		let general = [
			(
				write.s_type == STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET && write.next.is_null(),
				"sType and pNext",
			),
			(self.live_set(write.dst_set), "a set of a live pool"),
			(one, "one descriptor"),
			(!pending, "a set no pending commands use"),
		];
		for (holds, rule) in general.into_iter().chain(rules) {
			self.require(holds, || format!("vkUpdateDescriptorSets: {rule}"));
		}
		written
	}

	/// What `set`'s `binding` was last written with.
	fn written(&self, set: RawDescriptorSet, binding: u32) -> Option<(u64, u64)> {
		let writes = self.pipeline_objects.writes.iter().rev();
		let mut found = writes.filter(|&&(of, at, ..)| of == set && at == binding);
		found.next().map(|&(.., handle, detail)| (handle, detail))
	}
}

pub(super) unsafe extern "system" fn update_sets(
	device: RawDevice,
	count: u32,
	writes: *const WriteDescriptorSet,
	copies: u32,
	_: *const c_void,
) {
	// SAFETY: the bindings pass `count` writes.
	let writes = unsafe { slice::from_raw_parts(writes, count as usize) };
	with(|driver| {
		driver.check_live_device(device, "vkUpdateDescriptorSets");
		driver.require(copies == 0, || "vkUpdateDescriptorSets: no copies".into());
		for write in writes {
			let (handle, detail) = driver.check_write(write);
			let made = (write.dst_set, write.dst_binding, handle, detail);
			driver.pipeline_objects.writes.push(made);
			for buffer in &mut driver.command_buffers {
				buffer.drawing.stale |= buffer.drawing.sets.contains(&write.dst_set);
			}
			let kind = if write.descriptor_type == STORAGE_BUFFER {
				"range"
			} else {
				"layout"
			};
			driver.recorded.push(format!(
				"write {:#x} binding {}: {handle:#x} {kind} {detail:#x}",
				write.dst_set, write.dst_binding
			));
		}
	})
}

pub(super) unsafe extern "system" fn begin_rendering(
	buffer: RawCommandBuffer,
	info: *const RenderingInfo,
) {
	// SAFETY: the bindings pass a rendering info that lives across the call.
	let info = unsafe { &*info };
	// SAFETY: and its one attachment, which does too.
	let attachment = unsafe { &*info.color_attachments };
	with(|driver| {
		let call = "vkCmdBeginRendering";
		let rendering = driver
			.drawing(buffer, call)
			.and_then(|drawing| drawing.rendering);
		let image = driver.viewed(attachment.image_view);
		let made = image.and_then(|image| driver.made_image(image));
		let layout = image.map(|image| driver.layout(buffer, image));
		let Rect2D {
			offset: [x, y],
			extent: [width, height],
		} = info.render_area;
		let inside = made.is_some_and(|([right, bottom, _], ..)| {
			x >= 0
				&& y >= 0
				&& width > 0
				&& height > 0
				&& x as u32 + width <= right
				&& y as u32 + height <= bottom
		});
		let rules = [
			(rendering.is_none(), "not rendering already"),
			(
				info.s_type == STRUCTURE_TYPE_RENDERING_INFO && info.next.is_null() && info.flags == 0,
				"sType, pNext and flags",
			),
			(info.layer_count == 1 && info.view_mask == 0, "one layer"),
			(
				info.color_attachment_count == 1
					&& info.depth_attachment.is_null()
					&& info.stencil_attachment.is_null(),
				"one color attachment",
			),
			(
				attachment.s_type == STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO && attachment.next.is_null(),
				"attachment sType and pNext",
			),
			(
				made.is_some_and(|(.., usage)| usage & COLOR_ATTACHMENT != 0),
				"a live color attachment",
			),
			(
				attachment.image_layout == ATTACHMENT_LAYOUT && layout == Some(ATTACHMENT_LAYOUT),
				"in the color attachment layout",
			),
			(attachment.resolve_mode == 0, "no resolve"),
			(
				(0..=LAST_LOAD_OP).contains(&attachment.load_op) && attachment.store_op == STORE,
				"stored",
			),
			(inside, "an area within the image"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("{call}: {rule}"));
		}
		if let Some(drawing) = driver
			.command_buffer(buffer)
			.map(|buffer| &mut buffer.drawing)
		{
			drawing.rendering = Some(attachment.image_view);
		}
		let (view, load) = (attachment.image_view, attachment.load_op);
		driver.recorded.push(format!(
			"begin rendering {view:#x} at {x},{y} size {width}x{height} load {load}"
		));
	})
}

pub(super) unsafe extern "system" fn end_rendering(buffer: RawCommandBuffer) {
	with(|driver| {
		let rendering = driver
			.drawing(buffer, "vkCmdEndRendering")
			.and_then(|drawing| drawing.rendering.take());
		driver.require(rendering.is_some(), || {
			"vkCmdEndRendering: rendering".into()
		});
		driver.recorded.push("end rendering".into());
	})
}

pub(super) unsafe extern "system" fn bind_pipeline(
	buffer: RawCommandBuffer,
	point: i32,
	pipeline: RawPipeline,
) {
	with(|driver| {
		let live = driver.live_object(pipeline.0, PIPELINES);
		driver.require(point == 0 && live, || {
			"vkCmdBindPipeline: a live graphics pipeline".into()
		});
		if let Some(drawing) = driver.drawing(buffer, "vkCmdBindPipeline") {
			drawing.pipeline = Some(pipeline);
		}
		driver.recorded.push(format!("bind pipeline {pipeline:#x}"));
	})
}

pub(super) unsafe extern "system" fn set_viewport(
	buffer: RawCommandBuffer,
	first: u32,
	count: u32,
	viewports: *const Viewport,
) {
	// SAFETY: the bindings pass one viewport.
	let viewport = unsafe { *viewports };
	with(|driver| {
		let Viewport {
			x,
			y,
			width,
			height,
			min_depth,
			max_depth,
		} = viewport;
		let rules = [
			(first == 0 && count == 1, "one viewport"),
			(width > 0.0 && height > 0.0, "a positive size"),
			((min_depth, max_depth) == (0.0, 1.0), "depths 0 to 1"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCmdSetViewport: {rule}"));
		}
		if let Some(drawing) = driver.drawing(buffer, "vkCmdSetViewport") {
			drawing.viewport = true;
		}
		driver
			.recorded
			.push(format!("viewport {x},{y} size {width}x{height}"));
	})
}

pub(super) unsafe extern "system" fn set_scissor(
	buffer: RawCommandBuffer,
	first: u32,
	count: u32,
	scissors: *const Rect2D,
) {
	// SAFETY: the bindings pass one scissor.
	let Rect2D {
		offset: [x, y],
		extent: [width, height],
	} = unsafe { *scissors };
	with(|driver| {
		let rules = [
			(first == 0 && count == 1, "one scissor"),
			(x >= 0 && y >= 0, "a positive offset"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCmdSetScissor: {rule}"));
		}
		if let Some(drawing) = driver.drawing(buffer, "vkCmdSetScissor") {
			drawing.scissor = true;
		}
		driver
			.recorded
			.push(format!("scissor {x},{y} size {width}x{height}"));
	})
}

#[allow(
	clippy::too_many_arguments,
	reason = "vkCmdBindDescriptorSets takes eight"
)]
pub(super) unsafe extern "system" fn bind_sets(
	buffer: RawCommandBuffer,
	point: i32,
	layout: RawPipelineLayout,
	first: u32,
	count: u32,
	sets: *const RawDescriptorSet,
	offsets: u32,
	_: *const u32,
) {
	// SAFETY: the bindings pass `count` sets.
	let sets = unsafe { slice::from_raw_parts(sets, count as usize) };
	with(|driver| {
		let call = "vkCmdBindDescriptorSets";
		let live_layout = driver.live_object(layout.0, PIPELINE_LAYOUTS);
		let set_layouts = (live_layout).then(|| {
			driver.pipeline_objects.pipeline_layouts[(layout.0 - PIPELINE_LAYOUTS - 1) as usize]
				.0
				.clone()
		});
		let &[set] = sets else {
			driver.require(false, || format!("{call}: one set"));
			return;
		};
		let made = driver
			.pipeline_objects
			.sets
			.get(set.0.wrapping_sub(SETS + 1) as usize)
			.copied();
		let rules = [
			(point == 0 && first <= 1, "set 0 or 1 for graphics"),
			(live_layout, "a live pipeline layout"),
			(driver.live_set(set), "a set of a live pool"),
			(
				made
					.zip(set_layouts)
					.is_some_and(|((_, of), layouts)| layouts.get(first as usize) == Some(&of)),
				"a set of the layout's set layout",
			),
			(offsets == 0, "no dynamic offsets"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("{call}: {rule}"));
		}
		if let Some(drawing) = driver.drawing(buffer, call) {
			match first {
				0 => drawing.set = Some((set, layout)),
				_ => drawing.texture = Some((set, layout)),
			}
			drawing.sets.push(set);
		}
		match first {
			0 => driver.recorded.push(format!("bind set {set:#x}")),
			_ => driver.recorded.push(format!("bind image set {set:#x}")),
		}
	})
}

pub(super) unsafe extern "system" fn push(
	buffer: RawCommandBuffer,
	layout: RawPipelineLayout,
	stages: u32,
	offset: u32,
	size: u32,
	values: *const c_void,
) {
	// SAFETY: the bindings pass `size` bytes, as floats.
	let values = unsafe { slice::from_raw_parts(values.cast::<f32>(), size as usize / 4) };
	with(|driver| {
		let live = driver.live_object(layout.0, PIPELINE_LAYOUTS);
		let ranges = (live).then(|| {
			driver.pipeline_objects.pipeline_layouts[(layout.0 - PIPELINE_LAYOUTS - 1) as usize]
				.1
				.clone()
		});
		let within = ranges
			.unwrap_or_default()
			.iter()
			.any(|&[range_stages, at, length]| {
				stages == range_stages && offset >= at && offset + size <= at + length
			});
		let rules = [
			(live, "a live pipeline layout"),
			(
				size > 0 && size.is_multiple_of(4) && offset.is_multiple_of(4),
				"whole words",
			),
			(within, "within a range of the layout for those stages"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCmdPushConstants: {rule}"));
		}
		if let Some(drawing) = driver.drawing(buffer, "vkCmdPushConstants") {
			drawing.pushed = true;
		}
		driver.recorded.push(format!("push {values:?}"));
	})
}

pub(super) unsafe extern "system" fn draw(
	buffer: RawCommandBuffer,
	vertices: u32,
	instances: u32,
	first_vertex: u32,
	first_instance: u32,
) {
	with(|driver| {
		let call = "vkCmdDraw";
		let Some(drawing) = driver.drawing(buffer, call) else {
			return;
		};
		let (rendering, pipeline, set) = (drawing.rendering, drawing.pipeline, drawing.set);
		let texture = drawing.texture;
		let ready = drawing.pushed && drawing.viewport && drawing.scissor;
		let made = pipeline.and_then(|pipeline| {
			let index = (pipeline.0 - PIPELINES - 1) as usize;
			let made = driver.pipeline_objects.pipelines.get(index)?;
			Some((made.formats.clone(), made.layout))
		});
		let target = rendering.and_then(|view| driver.viewed(view));
		let format = target.and_then(|image| driver.images.get((image.0 - 0xa0_0001) as usize));
		let formats_match = made
			.as_ref()
			.zip(format)
			.is_some_and(|((formats, _), image)| formats == &[image.format]);
		let layouts_match = made
			.as_ref()
			.zip(set)
			.is_some_and(|(&(_, made), (_, layout))| made == layout);
		let quads = set.and_then(|(set, _)| driver.written(set, 0));
		let atlas = set
			.and_then(|(set, _)| driver.written(set, 1))
			.and_then(|(view, _)| driver.viewed(RawImageView(view)));
		let readable = atlas.is_some_and(|atlas| driver.layout(buffer, atlas) == READ_ONLY_LAYOUT);
		let textured = made
			.as_ref()
			.zip(texture)
			.is_some_and(|(&(_, made), (_, layout))| made == layout);
		let image = texture
			.and_then(|(set, _)| driver.written(set, 0))
			.and_then(|(view, _)| driver.viewed(RawImageView(view)));
		let shown = image.is_some_and(|image| driver.layout(buffer, image) == READ_ONLY_LAYOUT);
		let read = (u64::from(first_instance) + u64::from(instances)) * QUAD;
		let held = quads.and_then(|(quads, range)| {
			let (size, _) = driver.made_buffer(RawBuffer(quads))?;
			Some(range.min(size) >= read)
		});
		let rules = [
			(rendering.is_some(), "rendering"),
			(made.is_some(), "a bound pipeline"),
			(ready, "the push constants, viewport and scissor set"),
			(formats_match, "a target of the pipeline's format"),
			(layouts_match, "a set bound with the pipeline's layout"),
			(readable, "an atlas read only by shaders"),
			(textured, "an image set bound with the pipeline's layout"),
			(shown, "an image read only by shaders"),
			(held == Some(true), "every quad drawn in the bound buffer"),
			(vertices > 0 && first_vertex == 0, "vertices from the first"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("{call}: {rule}"));
		}
		driver.recorded.push(format!(
			"draw {vertices} vertices of {instances} instances from {first_instance}"
		));
	})
}
