//! Drawing quads: pointing the quad set at the quads and the atlas, moving
//! the atlas to be sampled and the target to be drawn to, and rendering
//! the quads over what the target holds, holding what the draw uses until
//! the GPU has finished with it.
use super::{
	buffer::Buffer,
	call::Error,
	descriptors::QuadSet,
	draw_types::*,
	image::Image,
	recording::{Layout, Recording},
};
use std::{ptr, rc::Rc};

/// `VK_BUFFER_USAGE_STORAGE_BUFFER_BIT`, and `VK_IMAGE_USAGE_SAMPLED_BIT`
/// and `_COLOR_ATTACHMENT_BIT`.
const STORAGE: u32 = 0x20;
const SAMPLED: u32 = 0x4;
const COLOR_ATTACHMENT: u32 = 0x10;
/// `VkDescriptorType`: a combined image sampler and a storage buffer.
const COMBINED_IMAGE_SAMPLER: i32 = 1;
const STORAGE_BUFFER: i32 = 7;
/// `VK_WHOLE_SIZE`.
const WHOLE_SIZE: u64 = !0;
/// `VK_PIPELINE_BIND_POINT_GRAPHICS`.
const GRAPHICS: i32 = 0;
/// `VK_ATTACHMENT_LOAD_OP_LOAD` and `VK_ATTACHMENT_STORE_OP_STORE`.
const LOAD: i32 = 0;
const STORE: i32 = 0;
/// `VK_SHADER_STAGE_VERTEX_BIT`, which reads the push constants.
const VERTEX: u32 = 0x1;
/// Bytes a quad in the buffer, and vertices a quad: two triangles.
const QUAD: u64 = 48;
const VERTICES: u32 = 6;

/// A draw of the first `count` quads in `quads` over `target`, with
/// glyphs from `atlas`, through `set`.
#[derive(Clone, Copy)]
pub(crate) struct Draw<'d> {
	pub set: &'d Rc<QuadSet>,
	pub quads: &'d Rc<Buffer>,
	pub count: u32,
	pub atlas: &'d Rc<Image>,
	pub target: &'d Rc<Image>,
}

impl Recording<'_> {
	/// Records `draw`, after whatever was recorded before used its atlas
	/// and target. A set draws once a submission, and only once the GPU
	/// has finished the last that drew with it.
	pub(crate) fn draw(&mut self, draw: Draw) -> Result<(), Error> {
		let Draw {
			set,
			quads,
			count,
			atlas,
			target,
		} = draw;
		let pipeline = &set.pipeline;
		let devices = [
			&pipeline.device,
			&quads.device,
			&atlas.device,
			&target.device,
		];
		let ours = devices
			.into_iter()
			.all(|made| Rc::ptr_eq(made, &self.commands.device));
		let fits = u64::from(count) * QUAD <= quads.size as u64;
		let made = quads.usage & STORAGE != 0
			&& atlas.usage & SAMPLED != 0
			&& target.usage & COLOR_ATTACHMENT != 0;
		let [width, height, layers] = target.size;
		let drawable = target.format == pipeline.format && layers == 1 && !Rc::ptr_eq(atlas, target);
		if !(ours && fits && made && drawable) {
			return Err(Error::Draw);
		}
		let pending = match &*set.used.borrow() {
			Some((timeline, value)) => timeline.reached()? < *value,
			None => false,
		};
		if pending || self.sets.iter().any(|drawn| Rc::ptr_eq(drawn, set)) {
			return Err(Error::Busy);
		}
		self.update(draw);
		self.transition(atlas, Layout::ShaderReadOnly);
		self.transition(target, Layout::ColorAttachment);
		let attachment = RenderingAttachmentInfo {
			s_type: STRUCTURE_TYPE_RENDERING_ATTACHMENT_INFO,
			next: ptr::null(),
			image_view: target.view(),
			image_layout: Layout::ColorAttachment as i32,
			resolve_mode: 0,
			resolve_image_view: 0,
			resolve_image_layout: 0,
			load_op: LOAD,
			store_op: STORE,
			clear_value: [0; 4],
		};
		let area = Rect2D {
			offset: [0, 0],
			extent: [width, height],
		};
		let info = RenderingInfo {
			s_type: STRUCTURE_TYPE_RENDERING_INFO,
			next: ptr::null(),
			flags: 0,
			render_area: area,
			layer_count: 1,
			view_mask: 0,
			color_attachment_count: 1,
			color_attachments: &attachment,
			depth_attachment: ptr::null(),
			stencil_attachment: ptr::null(),
		};
		let viewport = Viewport {
			x: 0.0,
			y: 0.0,
			width: width as f32,
			height: height as f32,
			min_depth: 0.0,
			max_depth: 1.0,
		};
		let size = [width as f32, height as f32];
		let (buffer, functions) = (self.commands.buffer, &self.commands.device.functions);
		let (layout, handle) = (pipeline.layout(), set.handle());
		// SAFETY: the buffer is being recorded; the pipeline, its layout, the
		// set and the target's view are the device's and held until the GPU
		// is done; the target was moved to the layout rendering names, and
		// the atlas to the one the set does; and every struct and array
		// outlives its call.
		unsafe {
			(functions.begin_rendering)(buffer, &info);
			(functions.bind_pipeline)(buffer, GRAPHICS, pipeline.handle());
			(functions.set_viewport)(buffer, 0, 1, &viewport);
			(functions.set_scissor)(buffer, 0, 1, &area);
			(functions.bind_sets)(buffer, GRAPHICS, layout, 0, 1, &handle, 0, ptr::null());
			(functions.push)(
				buffer,
				layout,
				VERTEX,
				0,
				size_of_val(&size) as u32,
				size.as_ptr().cast(),
			);
			(functions.draw)(buffer, VERTICES, count, 0, 0);
			(functions.end_rendering)(buffer);
		}
		self.commands.held.push(set.clone());
		self.commands.held.push(quads.clone());
		self.commands.held.push(atlas.clone());
		self.commands.held.push(target.clone());
		self.sets.push(set.clone());
		Ok(())
	}

	/// Points `draw`'s set at its quads and its atlas, which the set's
	/// layout samples in the shader read only layout.
	fn update(&self, draw: Draw) {
		let buffer = DescriptorBufferInfo {
			buffer: draw.quads.handle(),
			offset: 0,
			range: WHOLE_SIZE,
		};
		let image = DescriptorImageInfo {
			// The set layout's sampler is immutable.
			sampler: 0,
			image_view: draw.atlas.view(),
			image_layout: Layout::ShaderReadOnly as i32,
		};
		let write = |binding, descriptor_type| WriteDescriptorSet {
			s_type: STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
			next: ptr::null(),
			dst_set: draw.set.handle(),
			dst_binding: binding,
			dst_array_element: 0,
			descriptor_count: 1,
			descriptor_type,
			image_info: ptr::null(),
			buffer_info: ptr::null(),
			texel_buffer_view: ptr::null(),
		};
		let writes = [
			WriteDescriptorSet {
				buffer_info: &buffer,
				..write(0, STORAGE_BUFFER)
			},
			WriteDescriptorSet {
				image_info: &image,
				..write(1, COMBINED_IMAGE_SAMPLER)
			},
		];
		let device = &self.commands.device;
		// SAFETY: the set, buffer and view are the device's; no pending
		// commands use the set, as `draw` checked; and the writes and what
		// they point to outlive the call.
		unsafe { (device.functions.update_sets)(device.handle, 2, writes.as_ptr(), 0, ptr::null()) };
	}
}
