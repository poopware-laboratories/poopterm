//! The quad pipeline's descriptor set: where the shaders find the quads
//! and the atlas, allocated from a pool with room for it alone, which
//! frees it when destroyed on drop.
use super::{
	call::{Error, check},
	descriptor_types::*,
	pipeline::QuadPipeline,
	timeline::Timeline,
};
use std::{cell::RefCell, ptr, rc::Rc};

/// `VkDescriptorType`: a combined image sampler and a storage buffer.
const COMBINED_IMAGE_SAMPLER: i32 = 1;
const STORAGE_BUFFER: i32 = 7;

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
		let device = &pipeline.device;
		let info = DescriptorPoolCreateInfo {
			s_type: STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			max_sets: 1,
			pool_size_count: SIZES.len() as u32,
			pool_sizes: SIZES.as_ptr(),
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
		let layout = pipeline.set_layout();
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
