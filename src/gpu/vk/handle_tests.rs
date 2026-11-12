//! Tests of the handle types: each is laid out as the C handle it wraps,
//! so the driver's functions take them unchanged, is null by default, and
//! prints as the value it holds.
use super::handle::*;
use std::{
	ffi::c_void,
	mem::{align_of, size_of},
	ptr,
};

/// Each handle's name, size and alignment, its null, and its default.
macro_rules! layouts {
	($($name:ident),*) => {
		[$((stringify!($name), size_of::<$name>(), align_of::<$name>(), $name::NULL.0, $name::default().0)),*]
	};
}

#[test]
fn non_dispatchable_handles_are_laid_out_as_64_bits_and_null_at_zero() {
	let handles = layouts!(
		RawBuffer,
		RawMemory,
		RawImage,
		RawImageView,
		RawSemaphore,
		RawCommandPool,
		RawShaderModule,
		RawSampler,
		RawDescriptorSetLayout,
		RawPipelineLayout,
		RawPipeline,
		RawDescriptorPool,
		RawDescriptorSet,
		RawSurface,
		RawSwapchain,
		RawQueryPool
	);
	assert_eq!(handles.len(), 16);
	for (name, size, align, null, default) in handles {
		// `uint64_t` on every platform, 32-bit ones too.
		assert_eq!(
			(size, align),
			(size_of::<u64>(), align_of::<u64>()),
			"{name}"
		);
		assert_eq!((null, default), (0, 0), "{name}");
	}
}

#[test]
fn dispatchable_handles_are_laid_out_as_pointers_and_null_by_default() {
	let handles = layouts!(
		RawInstance,
		RawPhysicalDevice,
		RawDevice,
		RawQueue,
		RawCommandBuffer
	);
	for (name, size, align, null, default) in handles {
		let pointer = (size_of::<*mut c_void>(), align_of::<*mut c_void>());
		assert_eq!((size, align), pointer, "{name}");
		assert!(null.is_null() && default.is_null(), "{name}");
	}
}

#[test]
fn handles_print_as_the_values_they_hold() {
	// The fake driver's messages and the errors name handles this way, as
	// they did when handles were plain numbers and pointers.
	assert_eq!(format!("{:?}", RawImage(0xa0_0001)), "10485761");
	assert_eq!(format!("{:#x}", RawSemaphore(0xc0_0001)), "0xc00001");
	assert_eq!(format!("{:x}", RawBuffer::NULL), "0");
	assert_eq!(format!("{:?}", RawDevice(0x900 as *mut c_void)), "0x900");
	assert_eq!(
		format!("{:?}", RawQueue::NULL),
		format!("{:?}", ptr::null_mut::<c_void>())
	);
}

#[test]
fn handles_compare_by_value_and_null_is_no_live_handle() {
	assert_eq!(RawPipeline(0x64_0001), RawPipeline(0x64_0001));
	assert_ne!(RawPipeline(0x64_0001), RawPipeline(0x64_0002));
	assert_ne!(RawPipeline(0x64_0001), RawPipeline::NULL);
	let device = 0x5100 as *mut c_void;
	assert_eq!(RawDevice(device), RawDevice(device));
	assert_ne!(RawDevice(device), RawDevice::NULL);
	// Handles key the fake driver's maps of live objects.
	let live: std::collections::HashSet<_> = [RawImage(1), RawImage(2), RawImage(1)].into();
	assert_eq!(live.len(), 2);
}
