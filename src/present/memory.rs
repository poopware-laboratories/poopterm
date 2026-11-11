//! Anonymous shared memory for one window buffer, which the window system
//! maps through the file descriptor sent to it (`wl_shm.create_pool`,
//! MIT-SHM `AttachFd`): unmapped and closed when dropped.
use std::{
	io,
	os::fd::{AsFd, BorrowedFd, OwnedFd},
};

use crate::sys::mem::{self, Map};

/// The most bytes one buffer may take: the protocol sends sizes as `int`.
pub(crate) const MAX_BYTES: usize = i32::MAX as usize;

pub(crate) struct Memory {
	map: Map,
	fd: OwnedFd,
}

impl Memory {
	/// Room for `pixels` 32-bit pixels, zeroed. None is refused too, as
	/// `mmap` refuses an empty mapping (`InvalidInput`).
	pub(crate) fn new(pixels: usize) -> io::Result<Self> {
		let bytes = (pixels.checked_mul(4))
			.filter(|&bytes| bytes <= MAX_BYTES)
			.ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
		let file = std::fs::File::from(mem::anonymous(c"poopterm-frame")?);
		file.set_len(bytes as u64)?;
		let fd = OwnedFd::from(file);
		let map = Map::shared(fd.as_fd(), bytes)?;
		Ok(Self { map, fd })
	}

	pub(crate) fn fd(&self) -> BorrowedFd<'_> {
		self.fd.as_fd()
	}

	/// The pixels, which the compositor only reads, between a commit and
	/// the release.
	pub(crate) fn pixels(&mut self) -> &mut [u32] {
		self.map.words_mut().expect("mapped to write")
	}
}
