//! Tests of a window buffer's shared memory: its bounds, and that what is
//! written through the mapping is what the compositor reads through the
//! file descriptor.
use super::memory::{MAX_BYTES, Memory};
use std::io::ErrorKind;

/// The size of the file behind `memory`.
fn file_size(memory: &Memory) -> i64 {
	// SAFETY: a zeroed `stat` is valid, and the descriptor is open.
	unsafe {
		let mut stat = std::mem::zeroed::<libc::stat>();
		assert_eq!(libc::fstat(memory.fd(), &mut stat), 0);
		stat.st_size as i64
	}
}

#[test]
fn memory_starts_zeroed_with_a_file_of_exactly_its_size() {
	let mut memory = Memory::new(5).unwrap();
	assert_eq!(memory.pixels(), [0; 5]);
	assert_eq!(file_size(&memory), 20);
}

#[test]
fn pixels_written_are_what_the_file_holds() {
	let mut memory = Memory::new(3).unwrap();
	memory
		.pixels()
		.copy_from_slice(&[0x00ff_0000, 0x0000_ff00, 0x1234_5678]);
	let mut bytes = [0u8; 12];
	// SAFETY: reads into a buffer of that many bytes.
	let read = unsafe { libc::pread(memory.fd(), bytes.as_mut_ptr().cast(), 12, 0) };
	assert_eq!(read, 12);
	let pixels: Vec<u32> = bytes
		.chunks(4)
		.map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
		.collect();
	assert_eq!(pixels, [0x00ff_0000, 0x0000_ff00, 0x1234_5678]);
}

#[test]
fn no_pixels_or_more_than_an_int_of_bytes_is_refused() {
	for pixels in [0, MAX_BYTES / 4 + 1, usize::MAX / 4 + 1, usize::MAX] {
		let refused = Memory::new(pixels).err().map(|error| error.kind());
		assert_eq!(refused, Some(ErrorKind::InvalidInput), "{pixels} pixels");
	}
}

#[test]
fn the_largest_buffer_an_int_can_size_is_allowed() {
	// Nearly 2 GiB, but sparse: nothing is touched beyond the last pixel.
	let mut memory = Memory::new(MAX_BYTES / 4).unwrap();
	assert_eq!(file_size(&memory), (MAX_BYTES / 4 * 4) as i64);
	let pixels = memory.pixels();
	assert_eq!(pixels.len(), MAX_BYTES / 4);
	*pixels.last_mut().unwrap() = 7;
	assert_eq!(pixels.last(), Some(&7));
}

#[test]
fn the_descriptor_is_closed_on_exec() {
	let memory = Memory::new(1).unwrap();
	// SAFETY: reads the descriptor's flags.
	let flags = unsafe { libc::fcntl(memory.fd(), libc::F_GETFD) };
	assert_eq!(flags & libc::FD_CLOEXEC, libc::FD_CLOEXEC);
}
