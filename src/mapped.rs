//! Read-only mappings of whole files, so large files we only read (fonts)
//! cost shared, evictable page cache instead of a private heap copy.
//!
//! Package managers replace files by renaming a new one over the old, which
//! leaves a mapping of the old file intact. A file truncated in place while
//! mapped would make reads past its new end fault (`SIGBUS`), as it would in
//! any program that maps fonts; we accept that for files under the system's
//! and user's font directories.
// Calls into C or the kernel; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
use std::{
	fs::OpenOptions,
	io,
	ops::Deref,
	os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
	path::Path,
	ptr::{self, NonNull},
	slice,
};

pub(crate) struct Mapped {
	map: NonNull<u8>,
	len: usize,
}

// SAFETY: the mapping is read only and never written through, like a
// `&'static [u8]`, so any thread may read it and drop it.
unsafe impl Send for Mapped {}
// SAFETY: as above; shared access only reads.
unsafe impl Sync for Mapped {}

impl Mapped {
	/// Maps the regular file at `path`. Refuses other kinds of file and
	/// empty ones (`InvalidInput`), as `mmap` cannot map nothing; procfs
	/// files report a size of 0 and are refused too.
	pub(crate) fn open(path: &Path) -> io::Result<Self> {
		// Non-blocking, so opening a FIFO does not wait for a writer.
		let file = OpenOptions::new()
			.read(true)
			.custom_flags(libc::O_NONBLOCK)
			.open(path)?;
		let metadata = file.metadata()?;
		let refused = || io::Error::from(io::ErrorKind::InvalidInput);
		if !metadata.is_file() {
			return Err(refused());
		}
		// `mmap` refuses an empty file (`EINVAL`) as `InvalidInput` too.
		let len = usize::try_from(metadata.len()).map_err(|_| refused())?;
		// SAFETY: maps `len` bytes of an open file, read only and private, at
		// an address the kernel picks; the result is checked before use.
		let map = unsafe {
			libc::mmap(
				ptr::null_mut(),
				len,
				libc::PROT_READ,
				libc::MAP_PRIVATE,
				file.as_raw_fd(),
				0,
			)
		};
		if map == libc::MAP_FAILED {
			return Err(io::Error::last_os_error());
		}
		// The mapping holds its own reference to the file; `file` closes here.
		Ok(Self {
			map: NonNull::new(map.cast()).expect("mmap never maps address 0"),
			len,
		})
	}
}

impl Deref for Mapped {
	type Target = [u8];

	fn deref(&self) -> &[u8] {
		// SAFETY: the mapping holds `len` readable bytes and lives until
		// `self` drops; nothing writes to it.
		unsafe { slice::from_raw_parts(self.map.as_ptr(), self.len) }
	}
}

impl Drop for Mapped {
	fn drop(&mut self) {
		// SAFETY: the mapping is this one's, of this length, and no borrow of
		// it outlives `self`.
		unsafe { libc::munmap(self.map.as_ptr().cast(), self.len) };
	}
}

#[cfg(test)]
#[path = "mapped_tests.rs"]
mod tests;
