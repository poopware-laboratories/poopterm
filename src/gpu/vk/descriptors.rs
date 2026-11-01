//! The quad pipeline's descriptor sets: where the shaders find the quads
//! and the atlas, and the image a run of quads shows, each allocated from a
//! pool with room for it alone, which frees it when destroyed on drop.
use super::{
	call::{Error, check},
	descriptor_types::*,
	draw_types::{DescriptorImageInfo, STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET, WriteDescriptorSet},
	image::Image,
	pipeline::QuadPipeline,
	pipeline_types::RawDescriptorSetLayout,
	recording::Layout,
	timeline::Timeline,
};
use std::{cell::RefCell, ptr, rc::Rc};

/// `VkDescriptorType`: a combined image sampler and a storage buffer.
const COMBINED_IMAGE_SAMPLER: i32 = 1;
const STORAGE_BUFFER: i32 = 7;
/// `VK_IMAGE_USAGE_SAMPLED_BIT`.
const SAMPLED: u32 = 0x4;

/// What the quad set layout binds: the quads and the atlas.
const SIZES: [DescriptorPoolSize; 2] = [
	DescriptorPoolSize {
		descriptor_type: STORAGE_BUFFER,
		count: 1,
	},
	DescriptorPoolSize {
		descriptor_type: COMBINED_IMAGE_SAMPLER,
		count: 1,
	},
];

/// What the texture set layout binds: the image.
const TEXTURE_SIZES: [DescriptorPoolSize; 1] = [DescriptorPoolSize {
	descriptor_type: COMBINED_IMAGE_SAMPLER,
	count: 1,
}];

pub(crate) struct QuadSet {
	pool: RawDescriptorPool,
	set: RawDescriptorSet,
	/// The timeline and value of the last submission that drew with it:
	/// until the GPU gets there, the set must not be updated.
	pub(super) used: RefCell<Option<(Rc<Timeline>, u64)>>,
	/// Dropped after the pool is destroyed: the set is of its layout, and
	/// the pool of its device.
	pub(super) pipeline: Rc<QuadPipeline>,
}

impl QuadSet {
	/// A set of `pipeline`'s set layout.
	pub(crate) fn new(pipeline: &Rc<QuadPipeline>) -> Result<Self, Error> {
		Self::of(pipeline, pipeline.set_layout(), &SIZES)
	}

	/// A set of `pipeline`'s `layout`, which binds descriptors of `sizes`.
	fn of(
		pipeline: &Rc<QuadPipeline>,
		layout: RawDescriptorSetLayout,
		sizes: &[DescriptorPoolSize],
	) -> Result<Self, Error> {
		let device = &pipeline.device;
		let info = DescriptorPoolCreateInfo {
			s_type: STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			max_sets: 1,
			pool_size_count: sizes.len() as u32,
			pool_sizes: sizes.as_ptr(),
		};
		let mut pool = 0;
		let create = device.functions.create_descriptor_pool;
		// SAFETY: the device is live, `info` and the sizes outlive the call,
		// and the handle is writable.
		check(unsafe { create(device.handle, &info, ptr::null(), &mut pool) })?;
		// From here on, dropping `made` destroys the pool.
		let mut made = Self {
			pool,
			set: 0,
			used: RefCell::new(None),
			pipeline: pipeline.clone(),
		};
		let info = DescriptorSetAllocateInfo {
			s_type: STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
			next: ptr::null(),
			pool,
			set_count: 1,
			set_layouts: &layout,
		};
		let mut set = 0;
		let allocate = device.functions.allocate_descriptor_sets;
		// SAFETY: the pool and the set layout are the device's and live,
		// `info` outlives the call, and there is room for one handle.
		check(unsafe { allocate(device.handle, &info, &mut set) })?;
		made.set = set;
		Ok(made)
	}

	pub(crate) fn handle(&self) -> RawDescriptorSet {
		self.set
	}
}

impl Drop for QuadSet {
	fn drop(&mut self) {
		let device = &self.pipeline.device;
		let destroy = device.functions.destroy_descriptor_pool;
		// SAFETY: the pool is the device's, and its set no longer used.
		unsafe { destroy(device.handle, self.pool, ptr::null()) };
	}
}

/// Set 1 of the quad pipeline: an image that runs of quads show, written
/// once, read only by shaders.
pub(crate) struct TextureSet {
	set: QuadSet,
	/// Held while the set points to it.
	pub(super) image: Rc<Image>,
}

impl TextureSet {
	/// A set of `pipeline`'s texture layout pointing at `image`, to be in
	/// the shader read only layout when drawn; refused unless `image` was
	/// made for sampling.
	pub(crate) fn new(pipeline: &Rc<QuadPipeline>, image: &Rc<Image>) -> Result<Self, Error> {
		if image.usage & SAMPLED == 0 {
			return Err(Error::Draw);
		}
		let set = QuadSet::of(pipeline, pipeline.texture_layout(), &TEXTURE_SIZES)?;
		let info = DescriptorImageInfo {
			// The set layout's sampler is immutable.
			sampler: 0,
			image_view: image.view(),
			image_layout: Layout::ShaderReadOnly as i32,
		};
		let write = WriteDescriptorSet {
			s_type: STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
			next: ptr::null(),
			dst_set: set.handle(),
			dst_binding: 0,
			dst_array_element: 0,
			descriptor_count: 1,
			descriptor_type: COMBINED_IMAGE_SAMPLER,
			image_info: &info,
			buffer_info: ptr::null(),
			texel_buffer_view: ptr::null(),
		};
		let device = &pipeline.device;
		// SAFETY: the set is new, so no commands use it, the view is the
		// device's, and the write and what it points to outlive the call.
		unsafe { (device.functions.update_sets)(device.handle, 1, &write, 0, ptr::null()) };
		Ok(Self {
			set,
			image: image.clone(),
		})
	}

	pub(crate) fn handle(&self) -> RawDescriptorSet {
		self.set.handle()
	}

	pub(crate) fn pipeline(&self) -> &Rc<QuadPipeline> {
		&self.set.pipeline
	}
}
