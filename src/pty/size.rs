//! The window size a session last gave its PTY, so a size is set once and
//! again only when the grid or its cells' pixels change: a zoom that keeps
//! the grid still tells programs, which size images by pixels, the new cell.
use std::{
	io,
	os::fd::AsFd,
	sync::atomic::{AtomicU64, Ordering},
};

use super::device::{self, WindowSize};

/// The size last set, packed as four 16-bit fields.
#[derive(Debug, Default)]
pub(crate) struct SentSize(AtomicU64);

fn pack(size: WindowSize) -> u64 {
	let fields = [size.cols, size.rows, size.pixel_width, size.pixel_height];
	fields
		.iter()
		.fold(0, |packed, &field| packed << 16 | u64::from(field))
}

impl SentSize {
	/// Remembers `size` as already set, as opening a pair at that size does.
	pub(crate) fn new(size: WindowSize) -> Self {
		Self(AtomicU64::new(pack(size)))
	}

	/// Sets `fd`'s size to `cols`×`rows` cells of `cell` pixels (width,
	/// height) unless that is the size set last; whether it was set.
	pub(crate) fn set(
		&self,
		fd: &impl AsFd,
		(cols, rows): (usize, usize),
		cell: (usize, usize),
	) -> io::Result<bool> {
		let size = WindowSize::of(cols, rows, cell);
		if pack(size) == self.0.load(Ordering::Relaxed) {
			return Ok(false);
		}
		device::set_size(fd, size)?;
		self.0.store(pack(size), Ordering::Relaxed);
		Ok(true)
	}
}
