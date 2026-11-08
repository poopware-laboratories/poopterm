//! Tests of shared buffers on the real device: made zeroed at the size
//! asked for, and what the CPU writes reads back.
use super::{buffer::Buffer, device::tests::device};
use std::num::NonZeroUsize;

#[test]
fn a_buffer_holds_the_bytes_asked_for_zeroed() {
	let Some(device) = device() else { return };
	for bytes in [1, 48, 4096, 1 << 20] {
		let buffer = Buffer::new(&device, NonZeroUsize::new(bytes).unwrap()).unwrap();
		assert_eq!(buffer.size, bytes);
		assert_eq!(buffer.bytes().len(), bytes);
		assert!(buffer.bytes().iter().all(|&byte| byte == 0), "{bytes}");
	}
}

#[test]
fn what_the_cpu_writes_to_a_buffer_it_reads_back() {
	let Some(device) = device() else { return };
	let mut buffer = Buffer::new(&device, NonZeroUsize::new(64).unwrap()).unwrap();
	for (index, byte) in buffer.bytes_mut().iter_mut().enumerate() {
		*byte = index as u8 ^ 0xa5;
	}
	let read: Vec<u8> = buffer.bytes().to_vec();
	assert_eq!(read, (0..64).map(|i| i as u8 ^ 0xa5).collect::<Vec<_>>());
	// Two buffers never share their bytes.
	let other = Buffer::new(&device, NonZeroUsize::new(64).unwrap()).unwrap();
	assert!(other.bytes().iter().all(|&byte| byte == 0));
}

#[test]
fn a_buffer_is_asked_for_in_memory_both_share_at_its_length() {
	use super::fake_tests::{Made, fake, made};
	let device = fake(0);
	for bytes in [1, 48, usize::MAX >> 1] {
		assert!(Buffer::new(&device, NonZeroUsize::new(bytes).unwrap()).is_ok());
		// `MTLResourceStorageModeShared`, the default cache mode.
		assert_eq!(
			made(),
			[Made::Buffer {
				length: bytes,
				options: 0
			}]
		);
	}
}

#[test]
fn a_buffer_metal_refuses_is_refused() {
	use super::{
		device::Error,
		fake_tests::{BUFFER, fake},
	};
	let device = fake(BUFFER);
	let refused = Buffer::new(&device, NonZeroUsize::new(64).unwrap());
	assert_eq!(
		refused.err().map(|error| error.to_string()),
		Some(Error::Refused("buffer").to_string())
	);
}

#[test]
fn a_buffer_dropped_lets_go_of_what_metal_made_once() {
	use crate::{gpu::mtl::fake_tests::fake, objc::probe_tests::Weak};
	let device = fake(0);
	let buffer = Buffer::new(&device, NonZeroUsize::new(48).unwrap()).unwrap();
	let mut made = Weak::new(&buffer.buffer);
	let other = Buffer::new(&device, NonZeroUsize::new(48).unwrap()).unwrap();
	let mut kept = Weak::new(&other.buffer);
	drop(buffer);
	assert!(!made.alive());
	// Only its own.
	assert!(kept.alive());
	drop(other);
	assert!(!kept.alive());
}
