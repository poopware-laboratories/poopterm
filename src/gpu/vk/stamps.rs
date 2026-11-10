//! The GPU time of each frame, from two timestamps its commands write: one
//! as they start, one once all of them are done. Read when the next frame
//! is recorded, which waits until the GPU has finished the last.
use super::{
	call::{Error, check},
	device::Device,
	submit_types::RawCommandBuffer,
	types::RawDevice,
};
use std::{ffi::c_void, ptr, rc::Rc, time::Duration};

pub(crate) type RawQueryPool = u64;

pub(crate) const STRUCTURE_TYPE_QUERY_POOL_CREATE_INFO: i32 = 11;
/// `VK_QUERY_TYPE_TIMESTAMP`.
pub(crate) const TIMESTAMP: i32 = 2;
/// `VK_QUERY_RESULT_64_BIT`.
pub(crate) const RESULT_64: u32 = 1;
/// `VK_PIPELINE_STAGE_2_TOP_OF_PIPE_BIT` and `_ALL_COMMANDS_BIT`.
pub(crate) const TOP_OF_PIPE: u64 = 0x1;
pub(crate) const ALL_COMMANDS: u64 = 0x1_0000;

/// `VkQueryPoolCreateInfo`.
#[repr(C)]
pub(crate) struct QueryPoolCreateInfo {
	pub s_type: i32,
	pub next: *const c_void,
	pub flags: u32,
	pub query_type: i32,
	pub query_count: u32,
	pub pipeline_statistics: u32,
}

pub(crate) type CreateQueryPool = unsafe extern "system" fn(
	RawDevice,
	*const QueryPoolCreateInfo,
	*const c_void,
	*mut RawQueryPool,
) -> i32;
pub(crate) type DestroyQueryPool =
	unsafe extern "system" fn(RawDevice, RawQueryPool, *const c_void);
pub(crate) type CmdResetQueryPool =
	unsafe extern "system" fn(RawCommandBuffer, RawQueryPool, u32, u32);
pub(crate) type CmdWriteTimestamp2 =
	unsafe extern "system" fn(RawCommandBuffer, u64, RawQueryPool, u32);
pub(crate) type GetQueryPoolResults =
	unsafe extern "system" fn(RawDevice, RawQueryPool, u32, u32, usize, *mut c_void, u64, u32) -> i32;

/// The device's query functions, all or none.
#[derive(Clone, Copy)]
pub(crate) struct Functions {
	pub create: CreateQueryPool,
	pub destroy: DestroyQueryPool,
	pub reset: CmdResetQueryPool,
	pub write: CmdWriteTimestamp2,
	pub results: GetQueryPoolResults,
}

/// A pool of the two timestamps of a frame, destroyed when dropped.
pub(crate) struct Stamps {
	pool: RawQueryPool,
	functions: Functions,
	/// Whether the last submission wrote both.
	pub(super) written: bool,
	device: Rc<Device>,
}

impl Stamps {
	/// Timestamps on `device`, or `None` when it cannot write them: its
	/// queue counts no bits of time, it has no period, or its driver lacks
	/// a function.
	pub(crate) fn new(device: &Rc<Device>) -> Result<Option<Self>, Error> {
		let Some(functions) = device.stamps else {
			return Ok(None);
		};
		if device.timestamp_bits == 0
			|| device.timestamp_period.is_nan()
			|| device.timestamp_period <= 0.0
		{
			return Ok(None);
		}
		let info = QueryPoolCreateInfo {
			s_type: STRUCTURE_TYPE_QUERY_POOL_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			query_type: TIMESTAMP,
			query_count: 2,
			pipeline_statistics: 0,
		};
		let mut pool = 0;
		// SAFETY: the device is live, `info` outlives the call, and `pool`
		// is writable.
		check(unsafe { (functions.create)(device.handle, &info, ptr::null(), &mut pool) })?;
		Ok(Some(Self {
			pool,
			functions,
			written: false,
			device: device.clone(),
		}))
	}

	/// Records, first in `buffer`, the reset of both and the first.
	pub(super) fn start(&self, buffer: RawCommandBuffer) {
		// SAFETY: the buffer is the device's and being recorded, and the
		// pool holds two queries, which no submission in flight uses.
		unsafe {
			(self.functions.reset)(buffer, self.pool, 0, 2);
			(self.functions.write)(buffer, TOP_OF_PIPE, self.pool, 0);
		}
	}

	/// Records, last in `buffer`, the second, once all before it is done.
	pub(super) fn end(&self, buffer: RawCommandBuffer) {
		// SAFETY: as above; the first was reset and written before.
		unsafe { (self.functions.write)(buffer, ALL_COMMANDS, self.pool, 1) };
	}

	/// The GPU time of the last submission that wrote both, once the GPU
	/// has finished it; `None` when there is none or the driver has no
	/// result.
	pub(super) fn read(&mut self) -> Option<Duration> {
		if !std::mem::take(&mut self.written) {
			return None;
		}
		let mut stamps = [0u64; 2];
		// SAFETY: the pool is the device's, `stamps` has room for two
		// 64-bit results eight bytes apart, and the GPU has finished.
		let code = unsafe {
			(self.functions.results)(
				self.device.handle,
				self.pool,
				0,
				2,
				16,
				stamps.as_mut_ptr().cast(),
				8,
				RESULT_64,
			)
		};
		let device = &self.device;
		(code == 0).then(|| elapsed(stamps, device.timestamp_bits, device.timestamp_period))
	}
}

impl Drop for Stamps {
	fn drop(&mut self) {
		// SAFETY: the pool is the device's, and the commands that use it,
		// which own it, have finished.
		unsafe { (self.functions.destroy)(self.device.handle, self.pool, ptr::null()) };
	}
}

/// The time between timestamps `start` and `end`, of which the low `bits`
/// count, at `period` nanoseconds a tick; a counter that wrapped between
/// them still gives the time.
pub(crate) fn elapsed([start, end]: [u64; 2], bits: u32, period: f32) -> Duration {
	let mask = if bits >= 64 {
		u64::MAX
	} else {
		(1 << bits) - 1
	};
	let ticks = end.wrapping_sub(start) & mask;
	Duration::from_nanos((ticks as f64 * f64::from(period)) as u64)
}
