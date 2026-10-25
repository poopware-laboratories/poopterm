//! Tests of reading a device's memory types from the fake driver of
//! `driver_tests`, whose answer here is the memory it was given.
use super::{
	choose::{Device, GRAPHICS, Kind},
	driver_tests::{Driver, family, finish, install, with, with_devices},
	memory::MemoryType,
	types::*,
	version::api_version,
};

pub(super) unsafe extern "system" fn memory_properties(
	device: RawPhysicalDevice,
	memory: *mut MemoryProperties,
) {
	with(|driver| {
		if driver
			.device(device, "vkGetPhysicalDeviceMemoryProperties")
			.is_none()
		{
			return;
		}
		// Noise the bindings must ignore past the count, and heaps.
		let mut written = MemoryProperties {
			type_count: driver.memory.len() as u32,
			types: [MemoryType {
				flags: 0xf,
				heap: 9,
			}; 32],
			heap_count: 1,
			heaps: [MemoryHeap {
				size: 1 << 33,
				flags: 1,
			}; 16],
		};
		for (out, &kind) in written.types.iter_mut().zip(&driver.memory) {
			*out = kind;
		}
		// SAFETY: the bindings pass room for the properties.
		unsafe { memory.write(written) };
	})
}

/// A driver with one device with `memory`.
fn with_memory(memory: &[MemoryType]) -> Driver {
	let device = Device {
		name: "memory".into(),
		kind: Kind::Discrete,
		api: api_version(1, 3),
		families: vec![family(GRAPHICS, 1)],
		swapchain: true,
	};
	let mut driver = with_devices(&[device]);
	driver.memory = memory.to_vec();
	driver
}

fn memory(pairs: &[(u32, u32)]) -> Vec<MemoryType> {
	(pairs.iter())
		.map(|&(flags, heap)| MemoryType { flags, heap })
		.collect()
}

/// The memory types the bindings read from a driver with `given`.
fn read(given: &[MemoryType]) -> Vec<MemoryType> {
	let instance = install(with_memory(given))
		.create_instance(c"poopterm")
		.unwrap();
	let device = instance.physical_devices().unwrap()[0];
	let types = instance.memory_types(device);
	drop(instance);
	finish();
	types
}

#[test]
fn memory_types_come_in_the_drivers_order_with_their_heaps() {
	let radv = memory(&[(0x1, 0), (0x6, 1), (0x7, 2), (0xe, 1), (0xc1, 0)]);
	assert_eq!(read(&radv), radv);
	let one = memory(&[(0xf, 0)]);
	assert_eq!(read(&one), one);
	assert_eq!(read(&[]), []);
}

#[test]
fn at_most_32_memory_types_are_read_whatever_the_driver_claims() {
	let full: Vec<_> = (0..32)
		.map(|i| MemoryType {
			flags: i,
			heap: i % 3,
		})
		.collect();
	assert_eq!(read(&full), full);
	// The 33rd cannot be stored; the count alone says it exists.
	let mut over = full.clone();
	over.push(MemoryType {
		flags: 0x7,
		heap: 0,
	});
	assert_eq!(read(&over), full);
}

#[test]
fn memory_of_a_device_the_driver_never_listed_is_reported() {
	let instance = install(with_memory(&memory(&[(0x1, 0)])))
		.create_instance(c"poopterm")
		.unwrap();
	assert_eq!(instance.memory_types(0x900 as RawPhysicalDevice), []);
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(
		invalid,
		["vkGetPhysicalDeviceMemoryProperties on 0x900, which was not listed"]
	);
	drop(instance);
	finish();
}
