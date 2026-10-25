//! Recording commands and submitting them: copies between buffers and
//! image layers, with the barriers that order them and move images between
//! layouts, holding what they use until the GPU has finished with it.
//!
//! A buffer the GPU copies into must not be read until the timeline passes
//! the submission's value; `Buffer::bytes` does not check.
use super::{
	buffer::Buffer,
	call::{Error, check},
	commands::Commands,
	descriptors::QuadSet,
	image::Image,
	submit_types::*,
};
use std::{ptr, rc::Rc};

/// `VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT`: the timeline is raised once
/// every command has finished.
const ALL_COMMANDS: u64 = 0x1_0000;
/// `VK_PIPELINE_STAGE_2_ALL_TRANSFER_BIT` and `_HOST_BIT`.
const ALL_TRANSFER: u64 = 0x1000;
const HOST: u64 = 0x4000;
/// `VK_ACCESS_2_TRANSFER_READ_BIT`, `_TRANSFER_WRITE_BIT` and
/// `_HOST_READ_BIT`.
const TRANSFER_READ: u64 = 0x800;
const TRANSFER_WRITE: u64 = 0x1000;
const HOST_READ: u64 = 0x2000;
/// `VK_PIPELINE_STAGE_2_COLOR_ATTACHMENT_OUTPUT_BIT` and
/// `_FRAGMENT_SHADER_BIT`; `VK_ACCESS_2_COLOR_ATTACHMENT_READ_BIT`,
/// `_COLOR_ATTACHMENT_WRITE_BIT` and `_SHADER_SAMPLED_READ_BIT`.
const COLOR_OUTPUT: u64 = 0x400;
const FRAGMENT: u64 = 0x80;
const COLOR_READ: u64 = 0x80;
const COLOR_WRITE: u64 = 0x100;
const SAMPLED_READ: u64 = 0x1_0000_0000;
/// `VK_BUFFER_USAGE_TRANSFER_SRC_BIT` and `_DST_BIT`, which image usage
/// shares.
const TRANSFER_SRC: u32 = 1;
const TRANSFER_DST: u32 = 2;
/// Bytes a texel, in every format images are made with.
const TEXEL: u64 = 4;

/// `VkImageLayout`: how an image is laid out for what uses it next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
	/// Contents unknown; made images start here.
	Undefined = 0,
	ColorAttachment = 2,
	ShaderReadOnly = 5,
	TransferSrc = 6,
	TransferDst = 7,
}

impl Layout {
	/// What uses an image in this layout, as a stage and access mask:
	/// what a barrier out of it waits for, and into it makes wait.
	pub(crate) fn scope(self) -> (u64, u64) {
		match self {
			Self::Undefined => (0, 0),
			Self::ColorAttachment => (COLOR_OUTPUT, COLOR_READ | COLOR_WRITE),
			Self::ShaderReadOnly => (FRAGMENT, SAMPLED_READ),
			Self::TransferSrc => (ALL_TRANSFER, TRANSFER_READ),
			Self::TransferDst => (ALL_TRANSFER, TRANSFER_WRITE),
		}
	}
}

/// A rectangle of one layer of an image, in texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Region {
	pub layer: u32,
	/// Its top left corner.
	pub at: [u32; 2],
	/// Its width and height.
	pub size: [u32; 2],
}

/// Commands being recorded. Dropped without being submitted, they are
/// discarded when recording starts again.
pub(crate) struct Recording<'a> {
	pub(super) commands: &'a mut Commands,
	/// The layouts its barriers leave images in, the last one for an image
	/// counting; they take effect once submitted.
	layouts: Vec<(Rc<Image>, Layout)>,
	/// The sets drawn with, which the submission marks used.
	pub(super) sets: Vec<Rc<QuadSet>>,
}

impl<'a> Recording<'a> {
	/// Recording into `commands`, which has begun.
	pub(super) fn new(commands: &'a mut Commands) -> Self {
		Self {
			commands,
			layouts: Vec::new(),
			sets: Vec::new(),
		}
	}

	/// Copies `region` from `buffer`, tightly packed from `offset`, into
	/// `image`, after whatever was recorded before used it.
	pub(crate) fn upload(
		&mut self,
		buffer: &Rc<Buffer>,
		offset: u64,
		image: &Rc<Image>,
		region: Region,
	) -> Result<(), Error> {
		let copy = self.copy(buffer, TRANSFER_SRC, offset, image, TRANSFER_DST, region)?;
		self.transition(image, Layout::TransferDst);
		let (handle, functions) = (self.commands.buffer, &self.commands.device.functions);
		let layout = Layout::TransferDst as i32;
		// SAFETY: the buffer is being recorded, the buffer and image are the
		// device's and held until the GPU is done, and the region was
		// checked to lie within both.
		unsafe { (functions.upload)(handle, buffer.handle(), image.handle(), layout, 1, &copy) };
		Ok(())
	}

	/// Copies `region` of `image` into `buffer`, tightly packed from
	/// `offset`, for the CPU to read once the submission is done.
	pub(crate) fn download(
		&mut self,
		image: &Rc<Image>,
		region: Region,
		buffer: &Rc<Buffer>,
		offset: u64,
	) -> Result<(), Error> {
		let copy = self.copy(buffer, TRANSFER_DST, offset, image, TRANSFER_SRC, region)?;
		self.transition(image, Layout::TransferSrc);
		let (handle, functions) = (self.commands.buffer, &self.commands.device.functions);
		let layout = Layout::TransferSrc as i32;
		// SAFETY: as for uploads.
		unsafe { (functions.download)(handle, image.handle(), layout, buffer.handle(), 1, &copy) };
		let [width, height] = region.size;
		let barrier = BufferMemoryBarrier2 {
			s_type: STRUCTURE_TYPE_BUFFER_MEMORY_BARRIER_2,
			next: ptr::null(),
			src_stage_mask: ALL_TRANSFER,
			src_access_mask: TRANSFER_WRITE,
			dst_stage_mask: HOST,
			dst_access_mask: HOST_READ,
			src_queue_family_index: QUEUE_FAMILY_IGNORED,
			dst_queue_family_index: QUEUE_FAMILY_IGNORED,
			buffer: buffer.handle(),
			offset,
			size: u64::from(width) * u64::from(height) * TEXEL,
		};
		self.barrier(Some(&barrier), None);
		Ok(())
	}

	/// The copy of `region` between `buffer` at `offset` and `image`,
	/// which are held from here on, if both are the device's, made for
	/// `buffer_usage` and `image_usage`, and hold the region.
	fn copy(
		&mut self,
		buffer: &Rc<Buffer>,
		buffer_usage: u32,
		offset: u64,
		image: &Rc<Image>,
		image_usage: u32,
		region: Region,
	) -> Result<BufferImageCopy, Error> {
		let device = &self.commands.device;
		let ours = Rc::ptr_eq(&buffer.device, device) && Rc::ptr_eq(&image.device, device);
		let made = buffer.usage & buffer_usage != 0 && image.usage & image_usage != 0;
		let [width, height, layers] = image.size;
		let Region {
			layer,
			at: [x, y],
			size: [w, h],
		} = region;
		let within =
			|at: u32, size: u32, end| size > 0 && at.checked_add(size).is_some_and(|last| last <= end);
		let inside = within(x, w, width) && within(y, h, height) && layer < layers;
		let bytes = u64::from(w) * u64::from(h) * TEXEL;
		let end = offset.checked_add(bytes);
		let fits = offset.is_multiple_of(TEXEL) && end.is_some_and(|end| end <= buffer.size as u64);
		if !(ours && made && inside && fits) {
			return Err(Error::Copy);
		}
		self.commands.held.push(buffer.clone());
		self.commands.held.push(image.clone());
		// Within the image, so within `i32`.
		Ok(BufferImageCopy {
			buffer_offset: offset,
			buffer_row_length: 0,
			buffer_image_height: 0,
			image_subresource: [1, 0, layer, 1],
			image_offset: [x as i32, y as i32, 0],
			image_extent: [w, h, 1],
		})
	}

	/// Moves every layer of `image` to `layout`, after what used it in
	/// the layout it is in. Even a move to the same layout orders what
	/// comes after it.
	pub(super) fn transition(&mut self, image: &Rc<Image>, layout: Layout) {
		let pending = self
			.layouts
			.iter()
			.rev()
			.find(|(made, _)| Rc::ptr_eq(made, image));
		let from = pending.map_or(image.layout.get(), |&(_, layout)| layout);
		let (src_stage_mask, src_access_mask) = from.scope();
		let (dst_stage_mask, dst_access_mask) = layout.scope();
		let barrier = ImageMemoryBarrier2 {
			s_type: STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER_2,
			next: ptr::null(),
			src_stage_mask,
			src_access_mask,
			dst_stage_mask,
			dst_access_mask,
			old_layout: from as i32,
			new_layout: layout as i32,
			src_queue_family_index: QUEUE_FAMILY_IGNORED,
			dst_queue_family_index: QUEUE_FAMILY_IGNORED,
			image: image.handle(),
			subresource_range: super::types::SubresourceRange {
				aspect_mask: 1,
				base_mip_level: 0,
				level_count: 1,
				base_array_layer: 0,
				layer_count: image.size[2],
			},
		};
		self.barrier(None, Some(&barrier));
		self.layouts.push((image.clone(), layout));
	}

	/// Records one barrier.
	fn barrier(&self, buffer: Option<&BufferMemoryBarrier2>, image: Option<&ImageMemoryBarrier2>) {
		let info = DependencyInfo {
			s_type: STRUCTURE_TYPE_DEPENDENCY_INFO,
			next: ptr::null(),
			dependency_flags: 0,
			memory_barrier_count: 0,
			memory_barriers: ptr::null(),
			buffer_memory_barrier_count: buffer.is_some().into(),
			buffer_memory_barriers: buffer.map_or(ptr::null(), ptr::from_ref),
			image_memory_barrier_count: image.is_some().into(),
			image_memory_barriers: image.map_or(ptr::null(), ptr::from_ref),
		};
		let functions = &self.commands.device.functions;
		// SAFETY: the buffer is being recorded, and `info` and the barrier it
		// points to outlive the call.
		unsafe { (functions.barrier)(self.commands.buffer, &info) };
	}

	/// Submits the commands to the device's queue: what they raise the
	/// timeline to once finished. The layouts they leave images in take
	/// effect.
	pub(crate) fn submit(self) -> Result<u64, Error> {
		let commands = self.commands;
		let functions = &commands.device.functions;
		// SAFETY: the buffer is being recorded.
		check(unsafe { (functions.end)(commands.buffer) })?;
		// A failed submission skips its value, which nothing waits for.
		let value = commands.timeline.next();
		let buffer = CommandBufferSubmitInfo {
			s_type: STRUCTURE_TYPE_COMMAND_BUFFER_SUBMIT_INFO,
			next: ptr::null(),
			command_buffer: commands.buffer,
			device_mask: 0,
		};
		let signal = SemaphoreSubmitInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_SUBMIT_INFO,
			next: ptr::null(),
			semaphore: commands.timeline.handle(),
			value,
			stage_mask: ALL_COMMANDS,
			device_index: 0,
		};
		let info = SubmitInfo2 {
			s_type: STRUCTURE_TYPE_SUBMIT_INFO_2,
			next: ptr::null(),
			flags: 0,
			wait_semaphore_info_count: 0,
			wait_semaphore_infos: ptr::null(),
			command_buffer_info_count: 1,
			command_buffer_infos: &buffer,
			signal_semaphore_info_count: 1,
			signal_semaphore_infos: &signal,
		};
		// SAFETY: the queue is the device's and used from this thread alone,
		// the buffer is executable, `value` is above every value the timeline
		// was given before, and `info` and what it points to outlive the call.
		check(unsafe { (functions.submit)(commands.device.queue(), 1, &info, 0) })?;
		commands.submitted = value;
		for (image, layout) in self.layouts {
			image.layout.set(layout);
		}
		for set in self.sets {
			*set.used.borrow_mut() = Some((commands.timeline.clone(), value));
		}
		Ok(value)
	}
}
