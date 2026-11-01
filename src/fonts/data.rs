//! The bytes of a loaded font: built into the binary, read onto the heap,
//! or mapped from its file.
use std::{fs, io, ops::Deref, path::Path};

use crate::mapped::Mapped;

pub(crate) enum Data {
	Static(&'static [u8]),
	Owned(Vec<u8>),
	Mapped(Mapped),
}

impl Deref for Data {
	type Target = [u8];

	fn deref(&self) -> &[u8] {
		match self {
			Self::Static(bytes) => bytes,
			Self::Owned(bytes) => bytes,
			Self::Mapped(map) => map,
		}
	}
}

impl From<&'static [u8]> for Data {
	fn from(bytes: &'static [u8]) -> Self {
		Self::Static(bytes)
	}
}

impl From<Vec<u8>> for Data {
	fn from(bytes: Vec<u8>) -> Self {
		Self::Owned(bytes)
	}
}

/// A font file's bytes, mapped when possible. Regular files that cannot be
/// mapped (procfs, file systems without `mmap`) are read onto the heap;
/// other kinds of file (directories, FIFOs) are errors.
pub(crate) fn read_file(path: &Path) -> io::Result<Data> {
	match Mapped::open(path) {
		Ok(map) => Ok(Data::Mapped(map)),
		Err(error) if !fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) => Err(error),
		Err(_) => fs::read(path).map(Data::Owned),
	}
}
