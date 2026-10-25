//! A Vulkan timeline semaphore: a counter the GPU raises as it finishes
//! work, which the CPU reads and waits for, destroyed when dropped.
use super::{
	call::{Error, check},
	device::Device,
	types::*,
};
use std::{cell::Cell, ptr, rc::Rc, time::Duration};

/// `VK_SEMAPHORE_TYPE_TIMELINE`.
const TIMELINE: i32 = 1;
/// `VK_TIMEOUT`.
const TIMEOUT: i32 = 2;

pub(crate) struct Timeline {
	handle: RawSemaphore,
	/// The last value handed out for work to raise the timeline to.
	last: Cell<u64>,
	/// Dropped after the semaphore is destroyed, as it was made from it.
	pub(super) device: Rc<Device>,
}

impl Timeline {
	/// A timeline at zero.
	pub(crate) fn new(device: &Rc<Device>) -> Result<Self, Error> {
		let kind = SemaphoreTypeCreateInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_TYPE_CREATE_INFO,
			next: ptr::null(),
			semaphore_type: TIMELINE,
			initial_value: 0,
		};
		let info = SemaphoreCreateInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO,
			next: ptr::from_ref(&kind).cast(),
			flags: 0,
		};
		let mut handle = 0;
		let create = device.functions.create_semaphore;
		// SAFETY: the device is live and made with timelineSemaphore, the
		// chain outlives the call, and `handle` is writable.
		check(unsafe { create(device.handle, &info, ptr::null(), &mut handle) })?;
		Ok(Self {
			handle,
			last: Cell::new(0),
			device: device.clone(),
		})
	}

	pub(crate) fn handle(&self) -> RawSemaphore {
		self.handle
	}

	/// The value for the next submission to raise the timeline to: above
	/// every value before, as Vulkan requires.
	pub(super) fn next(&self) -> u64 {
		self.last.set(self.last.get() + 1);
		self.last.get()
	}

	/// How far the GPU got.
	pub(crate) fn reached(&self) -> Result<u64, Error> {
		let mut value = 0;
		let read = self.device.functions.semaphore_value;
		// SAFETY: the timeline is the device's, and `value` is writable.
		check(unsafe { read(self.device.handle, self.handle, &mut value) })?;
		Ok(value)
	}

	/// Waits up to `timeout` for the GPU to reach `value`: whether it did.
	/// Timeouts beyond what Vulkan holds (584 years) wait forever.
	pub(crate) fn wait(&self, value: u64, timeout: Duration) -> Result<bool, Error> {
		let info = SemaphoreWaitInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_WAIT_INFO,
			next: ptr::null(),
			flags: 0,
			semaphore_count: 1,
			semaphores: &self.handle,
			values: &value,
		};
		let nanoseconds = u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX);
		let wait = self.device.functions.wait_semaphores;
		// SAFETY: the timeline is the device's, and `info` and what it points
		// to outlive the call.
		let code = check(unsafe { wait(self.device.handle, &info, nanoseconds) })?;
		Ok(code != TIMEOUT)
	}
}

impl Drop for Timeline {
	fn drop(&mut self) {
		let destroy = self.device.functions.destroy_semaphore;
		// SAFETY: the timeline is the device's, and nothing waits for it:
		// the commands that raise it hold an `Rc` of it and wait for the GPU
		// to finish before they let go.
		unsafe { destroy(self.device.handle, self.handle, ptr::null()) };
	}
}
