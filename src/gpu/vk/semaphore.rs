//! A Vulkan binary semaphore, which orders work on the GPU against the
//! presentation engine, destroyed when dropped.
use super::{
	call::{Error, check},
	device::Device,
	types::*,
};
use std::{ptr, rc::Rc};

/// `VK_SEMAPHORE_TYPE_BINARY`.
const BINARY: i32 = 0;

pub(crate) struct Semaphore {
	handle: RawSemaphore,
	/// Dropped after the semaphore is destroyed, as it was made from it.
	device: Rc<Device>,
}

impl Semaphore {
	pub(crate) fn new(device: &Rc<Device>) -> Result<Self, Error> {
		let kind = SemaphoreTypeCreateInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_TYPE_CREATE_INFO,
			next: ptr::null(),
			semaphore_type: BINARY,
			initial_value: 0,
		};
		let info = SemaphoreCreateInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO,
			next: ptr::from_ref(&kind).cast(),
			flags: 0,
		};
		let mut handle = 0;
		let create = device.functions.create_semaphore;
		// SAFETY: the device is live, the chain outlives the call, and
		// `handle` is writable.
		check(unsafe { create(device.handle, &info, ptr::null(), &mut handle) })?;
		Ok(Self {
			handle,
			device: device.clone(),
		})
	}

	pub(crate) fn handle(&self) -> RawSemaphore {
		self.handle
	}
}

impl Drop for Semaphore {
	fn drop(&mut self) {
		let destroy = self.device.functions.destroy_semaphore;
		// SAFETY: the semaphore is the device's, and its owner waited for the
		// queue to finish what signals or waits for it.
		unsafe { destroy(self.device.handle, self.handle, ptr::null()) };
	}
}
