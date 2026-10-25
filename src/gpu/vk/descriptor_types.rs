//! The Vulkan types for descriptor pools and the sets allocated from
//! them, laid out as the specification's C declarations, and the types of
//! their functions.
use super::{pipeline_types::RawDescriptorSetLayout, types::RawDevice};
use std::ffi::c_void;

pub(crate) type RawDescriptorPool = u64;
pub(crate) type RawDescriptorSet = u64;

pub(crate) const STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO: i32 = 33;
pub(crate) const STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO: i32 = 34;

/// `VkDescriptorPoolSize`: room for `count` descriptors of a type.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DescriptorPoolSize {
	pub descriptor_type: i32,
	pub count: u32,
}

#[repr(C)]
pub(crate) struct DescriptorPoolCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub max_sets: u32,
	pub pool_size_count: u32,
	pub pool_sizes: *const DescriptorPoolSize,
}

#[repr(C)]
pub(crate) struct DescriptorSetAllocateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub pool: RawDescriptorPool,
	pub set_count: u32,
	pub set_layouts: *const RawDescriptorSetLayout,
}

pub(crate) type CreateDescriptorPool = unsafe extern "system" fn(
	RawDevice,
	*const DescriptorPoolCreateInfo,
	*const c_void,
	*mut RawDescriptorPool,
) -> i32;
pub(crate) type DestroyDescriptorPool =
	unsafe extern "system" fn(RawDevice, RawDescriptorPool, *const c_void);
pub(crate) type AllocateDescriptorSets = unsafe extern "system" fn(
	RawDevice,
	*const DescriptorSetAllocateInfo,
	*mut RawDescriptorSet,
) -> i32;
