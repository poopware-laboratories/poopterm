//! A Metal buffer in memory the CPU and GPU share, which the CPU writes
//! between frames: the quads a frame draws.
use super::device::{Device, Error, owned};
use crate::objc::Object;
use std::num::NonZeroUsize;

/// `MTLResourceStorageModeShared` with the default CPU cache mode.
const SHARED: usize = 0;

pub(crate) struct Buffer {
	pub(super) buffer: Object,
	/// Bytes it holds.
	pub(crate) size: usize,
}

impl Buffer {
	/// A buffer of `size` bytes on `device`, zeroed.
	pub(crate) fn new(device: &Device, size: NonZeroUsize) -> Result<Self, Error> {
		// SAFETY: `newBufferWithLength:options:` takes a length and resource
		// options and returns a zero-filled buffer the caller owns, or nil.
		let buffer = unsafe {
			let made = device
				.device
				.send(c"newBufferWithLength:options:", (size.get(), SHARED));
			owned(made, "buffer")?
		};
		Ok(Self {
			buffer,
			size: size.get(),
		})
	}

	/// What the buffer holds, for the CPU to read once the GPU is done
	/// with it.
	pub(crate) fn bytes(&self) -> &[u8] {
		// SAFETY: a shared buffer's `contents` points to its `size` bytes for
		// as long as it lives, and `&self` keeps out writers in this process;
		// the GPU writes it only in frames the caller waited for.
		unsafe {
			let contents: *const u8 = self.buffer.send(c"contents", ());
			std::slice::from_raw_parts(contents, self.size)
		}
	}

	/// What the buffer holds, for the CPU to write while no frame the GPU
	/// has yet to finish reads it.
	pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
		// SAFETY: as for `bytes`; `&mut self` makes this the only view.
		unsafe {
			let contents: *mut u8 = self.buffer.send(c"contents", ());
			std::slice::from_raw_parts_mut(contents, self.size)
		}
	}
}
