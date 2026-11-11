//! Read-only mappings of whole files, so large files we only read (fonts)
//! cost shared, evictable page cache instead of a private heap copy.
//!
//! Package managers replace files by renaming a new one over the old, which
//! leaves a mapping of the old file intact. A file truncated in place while
//! mapped would make reads past its new end fault (`SIGBUS`), as it would in
//! any program that maps fonts; we accept that for files under the system's
//! and user's font directories.
use std::{
	fs::OpenOptions,
	io,
	ops::Deref,
	os::{fd::AsFd, unix::fs::OpenOptionsExt},
	path::Path,
};

use crate::sys::mem::Map;

pub(crate) struct Mapped(Map);

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
		// The mapping holds its own reference to the file; `file` closes here.
		Map::read(file.as_fd(), len, false).map(Self)
	}
}

impl Deref for Mapped {
	type Target = [u8];

	fn deref(&self) -> &[u8] {
		self.0.bytes()
	}
}

#[cfg(test)]
#[path = "mapped_tests.rs"]
mod tests;
