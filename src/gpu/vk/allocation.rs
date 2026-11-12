//! Allocating the memory of a buffer or image: from the type most preferred
//! for its use, then, when that type's heap is full, from the next type on
//! another heap, as a 256 MB BAR without resizable BAR fills up.
use super::{
	call::{Error, check},
	device::Device,
	memory::{Usage, memory_choices},
	types::*,
};
use std::ptr;

/// `VK_ERROR_OUT_OF_DEVICE_MEMORY`: the heap is full, others may not be.
const OUT_OF_DEVICE_MEMORY: i32 = -2;

/// Memory for a resource that `needs` it, for `usage`. Only a full heap
/// moves on to the next type; any other failure is returned at once.
pub(super) fn allocate(
	device: &Device,
	needs: &MemoryRequirements,
	usage: Usage,
) -> Result<RawMemory, Error> {
	let mut full = Vec::new();
	let mut failed = Error::NoMemory;
	for index in memory_choices(&device.memory, needs.memory_type_bits, usage) {
		let heap = device.memory[index as usize].heap;
		if full.contains(&heap) {
			continue;
		}
		let info = MemoryAllocateInfo {
			s_type: STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
			next: ptr::null(),
			allocation_size: needs.size,
			memory_type_index: index,
		};
		// A failed call may leave anything in its output.
		let mut memory = RawMemory::NULL;
		// SAFETY: the device is live, `info` outlives the call, and
		// `memory` is writable.
		let code =
			unsafe { (device.functions.allocate)(device.handle, &info, ptr::null(), &mut memory) };
		match check(code) {
			Ok(_) => return Ok(memory),
			Err(error @ Error::Failed(OUT_OF_DEVICE_MEMORY)) => {
				full.push(heap);
				failed = error;
			}
			Err(error) => return Err(error),
		}
	}
	Err(failed)
}
