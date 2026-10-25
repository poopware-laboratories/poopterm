//! A Vulkan buffer with memory of its own, mapped for the CPU when it
//! uploads or reads back, destroyed and freed when dropped.
use super::{
	call::{Error, check},
	device::Device,
	memory::{Usage, choose_memory},
	types::*,
};
use std::{num::NonZeroUsize, ptr, rc::Rc, slice};

pub(crate) struct Buffer {
	handle: RawBuffer,
	/// Zero until allocated.
	memory: RawMemory,
	/// The CPU's view of the memory; null unless mapped.
	mapped: *mut u8,
	pub(super) size: usize,
	/// `VkBufferUsageFlags`.
	pub(super) usage: u32,
	/// Dropped after the buffer is destroyed, as it was made from it.
	pub(super) device: Rc<Device>,
}

impl Buffer {
	/// A buffer of `size` bytes for `usage` (`VkBufferUsageFlags`), in
	/// memory suited to its `purpose`.
	pub(crate) fn new(
		device: &Rc<Device>,
		size: NonZeroUsize,
		usage: u32,
		purpose: Usage,
	) -> Result<Self, Error> {
		let functions = &device.functions;
		let info = BufferCreateInfo {
			s_type: STRUCTURE_TYPE_BUFFER_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			size: size.get() as u64,
			usage,
			sharing_mode: 0,
			queue_family_index_count: 0,
			queue_family_indices: ptr::null(),
		};
		let mut handle = 0;
		// SAFETY: the device is live, `info` outlives the call, and `handle`
		// is writable.
		check(unsafe { (functions.create_buffer)(device.handle, &info, ptr::null(), &mut handle) })?;
		// From here on, dropping `buffer` undoes what was done so far.
		let mut buffer = Self {
			handle,
			memory: 0,
			mapped: ptr::null_mut(),
			size: size.get(),
			usage,
			device: device.clone(),
		};
		let mut needs = MemoryRequirements::default();
		// SAFETY: the buffer is the device's, and `needs` is writable.
		unsafe { (functions.buffer_requirements)(device.handle, handle, &mut needs) };
		let index = choose_memory(&device.memory, needs.memory_type_bits, purpose);
		let info = MemoryAllocateInfo {
			s_type: STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
			next: ptr::null(),
			allocation_size: needs.size,
			memory_type_index: index.ok_or(Error::NoMemory)?,
		};
		// A failed call may leave anything in its output.
		let mut memory = 0;
		// SAFETY: as for the buffer.
		check(unsafe { (functions.allocate)(device.handle, &info, ptr::null(), &mut memory) })?;
		buffer.memory = memory;
		// SAFETY: the memory is of a type the buffer allows, and as large as
		// it needs.
		check(unsafe { (functions.bind_buffer)(device.handle, handle, memory, 0) })?;
		if purpose != Usage::Device {
			let mut mapped = ptr::null_mut();
			// SAFETY: the memory is host-visible, as `choose_memory` chose it
			// for the CPU, and not mapped yet.
			let code = unsafe { (functions.map)(device.handle, memory, 0, WHOLE_SIZE, 0, &mut mapped) };
			check(code)?;
			buffer.mapped = mapped.cast();
		}
		Ok(buffer)
	}

	pub(crate) fn handle(&self) -> RawBuffer {
		self.handle
	}

	/// The buffer's bytes, for one made for uploads or readback.
	pub(crate) fn bytes(&self) -> Option<&[u8]> {
		// SAFETY: the mapping covers the buffer's `size` bytes and lasts as
		// long as it does; nothing the GPU does is in flight yet.
		(!self.mapped.is_null()).then(|| unsafe { slice::from_raw_parts(self.mapped, self.size) })
	}

	/// The buffer's bytes to write, for one made for uploads or readback.
	pub(crate) fn bytes_mut(&mut self) -> Option<&mut [u8]> {
		// SAFETY: as above, and `&mut self` keeps other views out.
		(!self.mapped.is_null()).then(|| unsafe { slice::from_raw_parts_mut(self.mapped, self.size) })
	}
}

impl Drop for Buffer {
	fn drop(&mut self) {
		let (device, functions) = (self.device.handle, &self.device.functions);
		// SAFETY: the buffer and its memory are the device's and no longer
		// used; freeing memory unmaps it.
		unsafe {
			(functions.destroy_buffer)(device, self.handle, ptr::null());
			if self.memory != 0 {
				(functions.free)(device, self.memory, ptr::null());
			}
		}
	}
}
