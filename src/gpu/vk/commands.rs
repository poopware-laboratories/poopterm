//! Recording work for the GPU and submitting it to the device's queue: a
//! command pool with one primary command buffer, recorded anew once the
//! GPU has finished with it, and a timeline each submission raises.
//! What is recorded is in `recording`.
use super::{
	call::{Error, check},
	device::Device,
	recording::Recording,
	submit_types::*,
	timeline::Timeline,
};
use std::{any::Any, ptr, rc::Rc, time::Duration};

/// `VK_COMMAND_POOL_CREATE_TRANSIENT_BIT`: recorded anew each time.
const TRANSIENT: u32 = 1;
/// `VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT`.
const ONE_TIME_SUBMIT: u32 = 1;

pub(crate) struct Commands {
	pool: RawCommandPool,
	/// Null until allocated; freed with the pool.
	pub(super) buffer: RawCommandBuffer,
	/// What the last submission raises the timeline to; 0 before any.
	pub(super) submitted: u64,
	/// What the commands recorded use, kept until the GPU has finished
	/// with them.
	pub(super) held: Vec<Rc<dyn Any>>,
	/// Outlives the submissions that raise it.
	pub(super) timeline: Rc<Timeline>,
	/// Dropped after the pool is destroyed, as it was made from it.
	pub(super) device: Rc<Device>,
}

impl Commands {
	/// Commands for the device's queue that raise `timeline`, the device's.
	pub(crate) fn new(timeline: &Rc<Timeline>) -> Result<Self, Error> {
		let device = &timeline.device;
		let info = CommandPoolCreateInfo {
			s_type: STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
			next: ptr::null(),
			flags: TRANSIENT,
			queue_family_index: device.family(),
		};
		let mut pool = 0;
		let create = device.functions.create_pool;
		// SAFETY: the device is live, `info` outlives the call, and `pool` is
		// writable.
		check(unsafe { create(device.handle, &info, ptr::null(), &mut pool) })?;
		// From here on, dropping `commands` destroys the pool.
		let mut commands = Self {
			pool,
			buffer: ptr::null_mut(),
			submitted: 0,
			held: Vec::new(),
			timeline: timeline.clone(),
			device: device.clone(),
		};
		let info = CommandBufferAllocateInfo {
			s_type: STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
			next: ptr::null(),
			command_pool: pool,
			level: 0,
			command_buffer_count: 1,
		};
		// A failed call may leave anything in its output.
		let mut buffer = ptr::null_mut();
		let allocate = device.functions.allocate_commands;
		// SAFETY: the pool is the device's, and `buffer` has room for one.
		check(unsafe { allocate(device.handle, &info, &mut buffer) })?;
		commands.buffer = buffer;
		Ok(commands)
	}

	/// What the last submission raises the timeline to; 0 before any.
	pub(crate) fn submitted(&self) -> u64 {
		self.submitted
	}

	/// Waits for the GPU to finish what was submitted.
	pub(crate) fn finish(&self) -> Result<(), Error> {
		self.timeline.wait(self.submitted, Duration::MAX).map(drop)
	}

	/// Starts recording, once the GPU has finished what was submitted
	/// before, which recording replaces.
	pub(crate) fn record(&mut self) -> Result<Recording<'_>, Error> {
		self.finish()?;
		self.held.clear();
		let (device, functions) = (self.device.handle, &self.device.functions);
		// SAFETY: the pool is the device's, and the GPU has finished its
		// buffer.
		check(unsafe { (functions.reset_pool)(device, self.pool, 0) })?;
		let info = CommandBufferBeginInfo {
			s_type: STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
			next: ptr::null(),
			flags: ONE_TIME_SUBMIT,
			inheritance_info: ptr::null(),
		};
		// SAFETY: the reset left the buffer initial, and `info` outlives the
		// call.
		check(unsafe { (functions.begin)(self.buffer, &info) })?;
		Ok(Recording::new(self))
	}
}

impl Drop for Commands {
	fn drop(&mut self) {
		// A lost device has finished everything.
		let _ = self.finish();
		let destroy = self.device.functions.destroy_pool;
		// SAFETY: the pool is the device's, and the GPU has finished its
		// buffer, which goes with it.
		unsafe { destroy(self.device.handle, self.pool, ptr::null()) };
	}
}
