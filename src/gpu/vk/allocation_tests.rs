//! Tests of allocating memory against the fake driver of `driver_tests`:
//! the most preferred type first, then, when its heap is full, the next
//! type on another heap, as a 256 MB BAR without resizable BAR needs.
use super::{
	buffer::Buffer,
	buffer_tests::device,
	call::Error,
	driver_tests::{finish, with},
	image::{Format, Image},
	memory::{DEVICE_LOCAL, HOST_COHERENT, HOST_VISIBLE, MemoryType, Usage},
};
use std::num::NonZeroUsize;

const HOST: u32 = HOST_VISIBLE | HOST_COHERENT;
const BAR: u32 = DEVICE_LOCAL | HOST;

/// NVIDIA's types without resizable BAR: video memory, system memory, and
/// the 256 MB window of video memory the CPU can write, each its own heap.
fn nvidia(full: &[u32]) -> std::rc::Rc<super::device::Device> {
	let full = full.to_vec();
	device(&[DEVICE_LOCAL, HOST, BAR], |driver| {
		driver.memory = [(DEVICE_LOCAL, 0), (HOST, 1), (BAR, 2)]
			.map(|(flags, heap)| MemoryType { flags, heap })
			.to_vec();
		driver.full_heaps = full;
	})
}

fn upload(device: &std::rc::Rc<super::device::Device>) -> Result<Buffer, Error> {
	Buffer::new(
		device,
		NonZeroUsize::new(4096).unwrap(),
		0x20,
		Usage::Upload,
	)
}

/// The types allocations were asked of, and those made, since the start.
fn attempts() -> (Vec<u32>, Vec<u32>) {
	with(|driver| {
		let made = driver.allocations.iter().map(|&(_, kind)| kind).collect();
		(driver.allocation_attempts.clone(), made)
	})
}

#[test]
fn uploads_go_to_the_bar_while_it_has_room() {
	let device = nvidia(&[]);
	let buffer = upload(&device).unwrap();
	assert_eq!(attempts(), (vec![2], vec![2]));
	drop((buffer, device));
	finish();
}

#[test]
fn uploads_move_to_system_memory_when_the_bar_is_full() {
	let device = nvidia(&[2]);
	let mut buffer = upload(&device).unwrap();
	assert_eq!(attempts(), (vec![2, 1], vec![1]));
	// Mapped all the same: system memory is host-visible too.
	buffer.bytes_mut().unwrap()[0] = 7;
	assert_eq!(buffer.bytes().unwrap()[0], 7);
	drop((buffer, device));
	finish();
}

#[test]
fn images_move_to_the_bar_then_system_memory_as_video_memory_fills() {
	fn image(device: &std::rc::Rc<super::device::Device>) -> Result<Image, Error> {
		Image::new(device, Format::Bgra, [16, 16, 1], 0x4)
	}
	let device = nvidia(&[0]);
	let made = image(&device).unwrap();
	assert_eq!(attempts(), (vec![0, 2], vec![2]));
	drop((made, device));
	finish();
	let device = nvidia(&[0, 2]);
	let made = image(&device).unwrap();
	assert_eq!(attempts(), (vec![0, 2, 1], vec![1]));
	drop((made, device));
	finish();
}

#[test]
fn every_heap_full_is_the_drivers_error_and_nothing_is_left() {
	let device = nvidia(&[0, 1, 2]);
	assert_eq!(upload(&device).err(), Some(Error::Failed(-2)));
	assert_eq!(attempts(), (vec![2, 1], vec![]));
	drop(device);
	let driver = finish();
	assert!(driver.live_memory.is_empty());
}

#[test]
fn a_heap_found_full_is_not_tried_again_through_another_type() {
	// Two types share the full heap: the second is skipped.
	let device = device(
		&[DEVICE_LOCAL, DEVICE_LOCAL | HOST_VISIBLE, HOST],
		|driver| {
			driver.memory = [
				(DEVICE_LOCAL, 0),
				(DEVICE_LOCAL | HOST_VISIBLE, 0),
				(HOST, 1),
			]
			.map(|(flags, heap)| MemoryType { flags, heap })
			.to_vec();
			driver.full_heaps = vec![0];
		},
	);
	let made = Image::new(&device, Format::Bgra, [16, 16, 1], 0x4).unwrap();
	assert_eq!(attempts(), (vec![0, 2], vec![2]));
	drop((made, device));
	finish();
}

#[test]
fn other_failures_are_errors_at_once() {
	// Out of host memory is not a full heap: another type will not help.
	let device = nvidia(&[]);
	with(|driver| driver.fails = vec![(c"vkAllocateMemory", -1)]);
	assert_eq!(upload(&device).err(), Some(Error::Failed(-1)));
	assert_eq!(attempts(), (vec![2], vec![]));
	with(|driver| driver.fails.clear());
	drop(device);
	finish();
}
