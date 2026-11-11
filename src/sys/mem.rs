//! Memory mappings and shared memory objects: `mmap` of an open file or
//! object as an owned [`Map`], unmapped when dropped, and `memfd_create` /
//! `shm_open` objects as owned descriptors.
use std::{
	ffi::CStr,
	io,
	os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
	ptr::{self, NonNull},
	slice,
};

/// A mapping of `len` bytes of a file or shared memory object. Reading past
/// the end of an object that shrank while mapped faults (`SIGBUS`), as in
/// any program that maps files; callers map what they trust not to shrink,
/// or copy out at once.
pub(crate) struct Map {
	ptr: NonNull<u8>,
	len: usize,
	writable: bool,
}

// SAFETY: a `Map` owns its mapping like a `Box<[u8]>` owns its memory: no
// other handle in this process reaches it, so it may move between threads.
unsafe impl Send for Map {}
// SAFETY: shared access only reads, and writing needs `&mut Map`.
unsafe impl Sync for Map {}

impl Map {
	/// Maps the first `len` bytes of `fd` to read: `shared` sees later
	/// writes by others, private does not. An empty mapping is refused
	/// (`InvalidInput`), as `mmap` refuses one.
	pub(crate) fn read(fd: BorrowedFd<'_>, len: usize, shared: bool) -> io::Result<Self> {
		let sharing = if shared {
			libc::MAP_SHARED
		} else {
			libc::MAP_PRIVATE
		};
		Self::new(fd, len, libc::PROT_READ, sharing)
	}

	/// Maps the first `len` bytes of `fd` to read and write, shared, so
	/// writes reach the object and whoever else maps it.
	pub(crate) fn shared(fd: BorrowedFd<'_>, len: usize) -> io::Result<Self> {
		Self::new(
			fd,
			len,
			libc::PROT_READ | libc::PROT_WRITE,
			libc::MAP_SHARED,
		)
	}

	fn new(fd: BorrowedFd<'_>, len: usize, protection: i32, sharing: i32) -> io::Result<Self> {
		// SAFETY: maps `len` bytes of an open descriptor at an address the
		// kernel picks, over no memory of ours; the result is checked.
		let map = unsafe { libc::mmap(ptr::null_mut(), len, protection, sharing, fd.as_raw_fd(), 0) };
		if map == libc::MAP_FAILED {
			return Err(io::Error::last_os_error());
		}
		Ok(Self {
			ptr: NonNull::new(map.cast()).expect("mmap never maps address 0"),
			len,
			writable: protection & libc::PROT_WRITE != 0,
		})
	}

	pub(crate) fn bytes(&self) -> &[u8] {
		// SAFETY: the mapping holds `len` readable bytes until `self` drops.
		unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
	}

	/// The mapping to write, or None when it was mapped to read only.
	#[cfg(test)]
	pub(crate) fn bytes_mut(&mut self) -> Option<&mut [u8]> {
		// SAFETY: a writable mapping holds `len` bytes until `self` drops,
		// and `&mut self` keeps every other borrow out.
		self
			.writable
			.then(|| unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) })
	}

	/// The whole 32-bit words of the mapping to write, or None when it was
	/// mapped to read only. Mappings start on a page, so they are aligned
	/// for any word.
	pub(crate) fn words_mut(&mut self) -> Option<&mut [u32]> {
		// SAFETY: as in `bytes_mut`, over the whole words of `len` bytes,
		// which the page alignment of the start aligns.
		self
			.writable
			.then(|| unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr().cast(), self.len / 4) })
	}
}

impl Drop for Map {
	fn drop(&mut self) {
		// SAFETY: the mapping is this one's, of this length, and no borrow of
		// it outlives `self`.
		unsafe { libc::munmap(self.ptr.as_ptr().cast(), self.len) };
	}
}

/// A new, empty shared memory object without a name, closed on exec; `name`
/// only labels it where the system shows one (`/proc/*/fd` on Linux).
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(crate) fn anonymous(name: &CStr) -> io::Result<OwnedFd> {
	#[cfg(target_os = "linux")]
	// SAFETY: `name` is a C string; the flags ask for nothing unusual.
	let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
	#[cfg(target_os = "freebsd")]
	let _ = name;
	#[cfg(target_os = "freebsd")]
	// SAFETY: `SHM_ANON` asks for an object without a name.
	let fd = unsafe { libc::shm_open(libc::SHM_ANON, libc::O_RDWR | libc::O_CLOEXEC, 0o600) };
	owned(fd)
}

/// Opens the POSIX shared memory object `name` (`/` and a name without
/// another `/`), read only, or read and write after creating it when
/// `create` is set. POSIX closes it on exec.
pub(crate) fn shm_open(name: &CStr, create: bool) -> io::Result<OwnedFd> {
	let flags = if create {
		libc::O_CREAT | libc::O_RDWR
	} else {
		libc::O_RDONLY
	};
	// SAFETY: `name` is a C string, and the object is only opened.
	owned(unsafe { libc::shm_open(name.as_ptr(), flags, 0o600) })
}

/// Removes the name of a shared memory object; open descriptors and
/// mappings of it stay valid.
pub(crate) fn shm_unlink(name: &CStr) -> io::Result<()> {
	// SAFETY: `name` is a C string.
	if unsafe { libc::shm_unlink(name.as_ptr()) } < 0 {
		return Err(io::Error::last_os_error());
	}
	Ok(())
}

/// The descriptor a call returned, or its error.
fn owned(fd: i32) -> io::Result<OwnedFd> {
	if fd < 0 {
		return Err(io::Error::last_os_error());
	}
	// SAFETY: the call just opened `fd`, and nothing else owns it.
	Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(test)]
#[path = "mem_tests.rs"]
mod tests;
