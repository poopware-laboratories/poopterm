//! Anonymous shared memory for one window buffer, which the compositor
//! maps through the file descriptor sent with `wl_shm.create_pool`:
//! unmapped and closed when dropped.
use std::{
	io,
	os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
	ptr::{self, NonNull},
	slice,
};

/// The most bytes one buffer may take: the protocol sends sizes as `int`.
pub(crate) const MAX_BYTES: usize = i32::MAX as usize;

pub(crate) struct Memory {
	map: NonNull<u32>,
	pixels: usize,
	fd: OwnedFd,
}

impl Memory {
	/// Room for `pixels` 32-bit pixels, zeroed. None is refused too, as
	/// `mmap` refuses an empty mapping (`InvalidInput`).
	pub(crate) fn new(pixels: usize) -> io::Result<Self> {
		let bytes = (pixels.checked_mul(4))
			.filter(|&bytes| bytes <= MAX_BYTES)
			.ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
		let fd = anonymous()?;
		// SAFETY: `fd` is a fresh shared memory object this owns, and `bytes`
		// fits an `off_t`.
		if unsafe { libc::ftruncate(fd.as_raw_fd(), bytes as libc::off_t) } != 0 {
			return Err(io::Error::last_os_error());
		}
		let protection = libc::PROT_READ | libc::PROT_WRITE;
		// SAFETY: maps the whole object, which is `bytes` long, shared, at an
		// address the kernel picks; the result is checked before use.
		let map = unsafe {
			libc::mmap(
				ptr::null_mut(),
				bytes,
				protection,
				libc::MAP_SHARED,
				fd.as_raw_fd(),
				0,
			)
		};
		if map == libc::MAP_FAILED {
			return Err(io::Error::last_os_error());
		}
		Ok(Self {
			map: NonNull::new(map.cast()).expect("mmap never maps address 0"),
			pixels,
			fd,
		})
	}

	pub(crate) fn fd(&self) -> RawFd {
		self.fd.as_raw_fd()
	}

	pub(crate) fn pixels(&mut self) -> &mut [u32] {
		// SAFETY: the mapping holds `pixels` pixels, is page aligned, lives
		// until `self` drops, and `&mut self` keeps other borrows out. The
		// compositor only reads it, between a commit and the release.
		unsafe { slice::from_raw_parts_mut(self.map.as_ptr(), self.pixels) }
	}
}

impl Drop for Memory {
	fn drop(&mut self) {
		// SAFETY: the mapping is this one's, of this length, and no borrow of
		// it outlives `self`.
		unsafe { libc::munmap(self.map.as_ptr().cast(), self.pixels * 4) };
	}
}

/// A new anonymous shared memory object, closed on exec.
fn anonymous() -> io::Result<OwnedFd> {
	#[cfg(target_os = "linux")]
	// SAFETY: the name is a C string; the flags ask for nothing unusual.
	let fd = unsafe { libc::memfd_create(c"poopterm-frame".as_ptr(), libc::MFD_CLOEXEC) };
	#[cfg(target_os = "freebsd")]
	// SAFETY: `SHM_ANON` asks for an object without a name.
	let fd = unsafe { libc::shm_open(libc::SHM_ANON, libc::O_RDWR | libc::O_CLOEXEC, 0o600) };
	if fd < 0 {
		return Err(io::Error::last_os_error());
	}
	// SAFETY: `fd` was just opened and nothing else owns it.
	Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
