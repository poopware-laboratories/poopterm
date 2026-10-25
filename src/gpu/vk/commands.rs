//! Recording work for the GPU and submitting it to the device's queue: a
//! command pool with one primary command buffer, recorded anew once the
//! GPU has finished with it, and a timeline each submission raises.
use super::{
	call::{Error, check},
	device::Device,
	submit_types::*,
	timeline::Timeline,
};
use std::{ptr, rc::Rc, time::Duration};

/// `VK_COMMAND_POOL_CREATE_TRANSIENT_BIT`: recorded anew each time.
const TRANSIENT: u32 = 1;
/// `VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT`.
const ONE_TIME_SUBMIT: u32 = 1;
/// `VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT`: the timeline is raised once
/// every command has finished.
const ALL_COMMANDS: u64 = 0x1_0000;

pub(crate) struct Commands {
	pool: RawCommandPool,
	/// Null until allocated; freed with the pool.
	buffer: RawCommandBuffer,
	/// What the last submission raises the timeline to; 0 before any.
	submitted: u64,
	/// Outlives the submissions that raise it.
	timeline: Rc<Timeline>,
	/// Dropped after the pool is destroyed, as it was made from it.
	device: Rc<Device>,
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
		Ok(Recording { commands: self })
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

/// Commands being recorded. Dropped without being submitted, they are
/// discarded when recording starts again.
pub(crate) struct Recording<'a> {
	commands: &'a mut Commands,
}

impl Recording<'_> {
	/// Submits the commands to the device's queue: what they raise the
	/// timeline to once finished.
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
		Ok(value)
	}
}
