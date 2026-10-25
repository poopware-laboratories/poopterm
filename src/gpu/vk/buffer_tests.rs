//! Tests of buffers against the fake driver of `driver_tests`: the memory
//! each is given, mapping, what is undone when a step fails, and the order
//! buffers, memory and devices go in. The fake's buffer and memory
//! functions live here and check the specification's valid usage rules.
use super::{
	buffer::Buffer,
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind},
	device::Device,
	driver_tests::{Driver, family, finish, install, with, with_devices},
	memory::{HOST_VISIBLE, MemoryType, Usage},
	types::*,
	version::api_version,
};
use std::{
	ffi::{CStr, c_void},
	mem::transmute,
	num::NonZeroUsize,
	ptr,
	rc::Rc,
};

/// What a buffer was created with.
#[derive(Debug, PartialEq)]
pub(super) struct Made {
	pub size: u64,
	pub usage: u32,
}

/// What the fake aligns buffer sizes to.
const ALIGNMENT: u64 = 256;
/// What a failing call leaves in its output handle.
pub(super) const GARBAGE: u64 = 0xdead;
/// `VK_BUFFER_USAGE_TRANSFER_DST_BIT` and `_VERTEX_BUFFER_BIT`.
const TRANSFER_DST: u32 = 2;
const VERTEX: u32 = 0x80;

/// The fake's buffer and memory function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateBuffer" => transmute::<CreateBuffer, VoidFunction>(create_buffer),
			b"vkDestroyBuffer" => transmute::<DestroyBuffer, VoidFunction>(destroy_buffer),
			b"vkGetBufferMemoryRequirements" => {
				transmute::<GetBufferMemoryRequirements, VoidFunction>(requirements)
			}
			b"vkAllocateMemory" => transmute::<AllocateMemory, VoidFunction>(allocate),
			b"vkFreeMemory" => transmute::<FreeMemory, VoidFunction>(free),
			b"vkBindBufferMemory" => transmute::<BindBufferMemory, VoidFunction>(bind),
			b"vkMapMemory" => transmute::<MapMemory, VoidFunction>(map),
			_ => return super::image_tests::function(name),
		})
	}
}

impl Driver {
	/// Breaks the test unless `rule` holds.
	pub(super) fn require(&mut self, rule: bool, broken: impl FnOnce() -> String) {
		if !rule {
			self.invalid.push(broken());
		}
	}

	/// The memory types buffers may use.
	pub(super) fn allowed(&self) -> u32 {
		let every = u32::MAX
			.checked_shr(32 - self.memory.len() as u32)
			.unwrap_or(0);
		self.buffer_bits.unwrap_or(every)
	}
}

unsafe extern "system" fn create_buffer(
	device: RawDevice,
	info: *const BufferCreateInfo,
	allocator: *const c_void,
	buffer: *mut RawBuffer,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		driver.check_live_device(device, "vkCreateBuffer");
		let rules = [
			(info.s_type == STRUCTURE_TYPE_BUFFER_CREATE_INFO, "sType"),
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(allocator.is_null(), "no allocator"),
			(info.size > 0, "size"),
			(info.usage != 0, "usage"),
			(info.sharing_mode == 0, "exclusive"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateBuffer: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkCreateBuffer") {
			// SAFETY: the bindings pass a writable handle.
			unsafe { buffer.write(GARBAGE) };
			return code;
		}
		driver.buffers.push(Made {
			size: info.size,
			usage: info.usage,
		});
		let handle = 0xb0_0000 + driver.buffers.len() as u64;
		driver.live_buffers.push(handle);
		// SAFETY: as above.
		unsafe { buffer.write(handle) };
		0
	})
}

/// The index of `buffer` in `buffers`, as the fake numbers them.
fn index(buffer: RawBuffer) -> usize {
	(buffer - 0xb0_0001) as usize
}

unsafe extern "system" fn requirements(
	device: RawDevice,
	buffer: RawBuffer,
	requirements: *mut MemoryRequirements,
) {
	with(|driver| {
		driver.check_live_device(device, "vkGetBufferMemoryRequirements");
		let live = driver.live_buffers.contains(&buffer);
		driver.require(live, || {
			"vkGetBufferMemoryRequirements: a live buffer".into()
		});
		let size = driver
			.buffers
			.get(index(buffer))
			.map_or(0, |made| made.size);
		let written = MemoryRequirements {
			size: size.next_multiple_of(ALIGNMENT),
			alignment: ALIGNMENT,
			memory_type_bits: driver.allowed(),
		};
		// SAFETY: the bindings pass room for the requirements.
		unsafe { requirements.write(written) };
	})
}

unsafe extern "system" fn allocate(
	device: RawDevice,
	info: *const MemoryAllocateInfo,
	allocator: *const c_void,
	memory: *mut RawMemory,
) -> i32 {
	// SAFETY: the bindings pass an allocate info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		driver.check_live_device(device, "vkAllocateMemory");
		let known = (info.memory_type_index as usize) < driver.memory.len();
		let rules = [
			(info.s_type == STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO, "sType"),
			(
				info.next.is_null() && allocator.is_null(),
				"pNext and allocator",
			),
			(info.allocation_size > 0, "allocationSize"),
			(known, "memoryTypeIndex"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkAllocateMemory: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkAllocateMemory") {
			// SAFETY: the bindings pass a writable handle.
			unsafe { memory.write(GARBAGE) };
			return code;
		}
		(driver.allocations).push((info.allocation_size, info.memory_type_index));
		let handle = 0xe0_0000 + driver.allocations.len() as u64;
		driver.live_memory.push(handle);
		// SAFETY: as above.
		unsafe { memory.write(handle) };
		0
	})
}

/// The size and type of `memory`, as the fake numbers allocations.
pub(super) fn allocation(driver: &Driver, memory: RawMemory) -> Option<(u64, u32)> {
	let live = driver.live_memory.contains(&memory);
	live.then(|| driver.allocations[(memory - 0xe0_0001) as usize])
}

unsafe extern "system" fn bind(
	device: RawDevice,
	buffer: RawBuffer,
	memory: RawMemory,
	offset: u64,
) -> i32 {
	with(|driver| {
		driver.check_live_device(device, "vkBindBufferMemory");
		let live = driver.live_buffers.contains(&buffer);
		let unbound = driver.bound.iter().all(|&(bound, _)| bound != buffer);
		let needed = driver
			.buffers
			.get(index(buffer))
			.map_or(0, |made| made.size);
		let (size, kind) = allocation(driver, memory).unwrap_or((0, 32));
		let rules = [
			(live && unbound, "a live buffer bound once"),
			(
				kind < 32 && driver.allowed() >> kind & 1 != 0,
				"a memory type it allows",
			),
			(
				offset.is_multiple_of(ALIGNMENT) && offset + needed <= size,
				"room at the offset",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkBindBufferMemory: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkBindBufferMemory") {
			return code;
		}
		driver.bound.push((buffer, memory));
		0
	})
}

unsafe extern "system" fn map(
	device: RawDevice,
	memory: RawMemory,
	offset: u64,
	size: u64,
	flags: u32,
	data: *mut *mut c_void,
) -> i32 {
	with(|driver| {
		driver.check_live_device(device, "vkMapMemory");
		let (bytes, kind) = allocation(driver, memory).unwrap_or((0, 32));
		let visible =
			(driver.memory.get(kind as usize)).is_some_and(|kind| kind.flags & HOST_VISIBLE != 0);
		let unmapped = driver.mapped.iter().all(|&(mapped, _)| mapped != memory);
		let rules = [
			(visible, "host-visible memory"),
			(unmapped, "memory not mapped yet"),
			(offset == 0 && flags == 0, "offset and flags"),
			(size == WHOLE_SIZE || size <= bytes, "size"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkMapMemory: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkMapMemory") {
			return code;
		}
		let mut backing = vec![0xa5; bytes as usize].into_boxed_slice();
		// SAFETY: the bindings pass a writable pointer.
		unsafe { data.write(backing.as_mut_ptr().cast()) };
		driver.mapped.push((memory, backing));
		0
	})
}

unsafe extern "system" fn destroy_buffer(
	device: RawDevice,
	buffer: RawBuffer,
	allocator: *const c_void,
) {
	with(|driver| {
		driver.check_live_device(device, "vkDestroyBuffer");
		let live = driver.live_buffers.contains(&buffer) && allocator.is_null();
		driver.require(live, || {
			format!("vkDestroyBuffer: {buffer:#x}, which is not live")
		});
		driver.live_buffers.retain(|&other| other != buffer);
		driver.released.push(format!("buffer {buffer:#x}"));
	})
}

unsafe extern "system" fn free(device: RawDevice, memory: RawMemory, allocator: *const c_void) {
	with(|driver| {
		driver.check_live_device(device, "vkFreeMemory");
		let live = driver.live_memory.contains(&memory) && allocator.is_null();
		driver.require(live, || {
			format!("vkFreeMemory: {memory:#x}, which is not live")
		});
		driver.live_memory.retain(|&other| other != memory);
		driver.mapped.retain(|&(mapped, _)| mapped != memory);
		driver.released.push(format!("memory {memory:#x}"));
	})
}

/// A device on a driver whose GPU has `memory`, with `change` made to it.
pub(super) fn device(memory: &[u32], change: impl FnOnce(&mut Driver)) -> Rc<Device> {
	let described = Described {
		name: "buffers".into(),
		kind: Kind::Integrated,
		api: api_version(1, 3),
		families: vec![family(GRAPHICS, 1)],
		swapchain: true,
	};
	let mut driver = with_devices(&[described]);
	driver.memory = (memory.iter())
		.map(|&flags| MemoryType { flags, heap: 0 })
		.collect();
	change(&mut driver);
	let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
	let physical = instance.physical_devices().unwrap()[0];
	Rc::new(Device::new(instance, physical, 0).unwrap())
}

/// Intel ADL-N's first memory types: device-local, + host-visible
/// coherent, + cached.
pub(super) const INTEL: [u32; 3] = [0x1, 0x7, 0xf];

fn size(bytes: usize) -> NonZeroUsize {
	NonZeroUsize::new(bytes).unwrap()
}

#[test]
fn an_upload_buffer_is_bound_to_coherent_memory_and_written_through_its_mapping() {
	let device = device(&INTEL, |_| {});
	let mut buffer = Buffer::new(&device, size(1000), VERTEX, Usage::Upload).expect("buffer");
	assert_eq!(buffer.handle(), 0xb0_0001);
	let bytes = buffer.bytes_mut().expect("mapped");
	assert_eq!(bytes.len(), 1000);
	bytes.fill(7);
	bytes[999] = 9;
	let backing = with(|driver| driver.mapped[0].1.clone());
	// What the CPU wrote is in the memory; past the buffer is untouched.
	assert_eq!((backing[0], backing[999], backing[1000]), (7, 9, 0xa5));
	assert_eq!(backing.len(), 1024);
	drop(buffer);
	drop(device);
	let driver = finish();
	assert_eq!(
		driver.buffers,
		[Made {
			size: 1000,
			usage: VERTEX
		}]
	);
	assert_eq!(driver.allocations, [(1024, 1)]);
	assert_eq!(driver.bound, [(0xb0_0001, 0xe0_0001)]);
	assert_eq!(driver.released, ["buffer 0xb00001", "memory 0xe00001"]);
	assert_eq!(driver.destroyed_devices.len(), 1);
}

#[test]
fn a_buffer_for_the_gpu_alone_is_never_mapped() {
	for memory in [&INTEL[..], &[0xf]] {
		let device = device(memory, |_| {});
		let mut buffer = Buffer::new(&device, size(64), TRANSFER_DST, Usage::Device).unwrap();
		assert!(buffer.bytes().is_none() && buffer.bytes_mut().is_none());
		drop(buffer);
		drop(device);
		let driver = finish();
		assert!(driver.mapped.is_empty());
		assert_eq!(driver.allocations, [(256, 0)]);
	}
}

#[test]
fn readback_reads_what_was_written_to_its_memory() {
	let device = device(&INTEL, |_| {});
	let buffer = Buffer::new(&device, size(4), TRANSFER_DST, Usage::Readback).unwrap();
	// As the GPU would write it.
	with(|driver| driver.mapped[0].1[..4].copy_from_slice(&[1, 2, 3, 4]));
	assert_eq!(buffer.bytes(), Some(&[1, 2, 3, 4][..]));
	drop(buffer);
	assert_eq!(with(|driver| driver.allocations.clone()), [(256, 2)]);
	drop(device);
	finish();
}

#[test]
fn only_the_memory_types_a_buffer_allows_are_used() {
	let device = device(&INTEL, |driver| driver.buffer_bits = Some(0b101));
	let buffer = Buffer::new(&device, size(300), VERTEX, Usage::Upload).unwrap();
	assert!(buffer.bytes().is_some());
	drop(buffer);
	drop(device);
	assert_eq!(finish().allocations, [(512, 2)]);
}

#[test]
fn a_buffer_no_memory_suits_is_an_error_and_is_destroyed() {
	// Only device-local memory, the CPU cannot write it.
	let device = device(&INTEL, |driver| driver.buffer_bits = Some(0b1));
	let error = Buffer::new(&device, size(16), VERTEX, Usage::Upload).err();
	assert_eq!(error, Some(Error::NoMemory));
	assert_eq!(
		error.unwrap().to_string(),
		"no Vulkan memory type suits the resource"
	);
	drop(device);
	let driver = finish();
	assert!(driver.allocations.is_empty());
	assert_eq!(driver.released, ["buffer 0xb00001"]);
}

#[test]
fn a_failing_step_undoes_the_steps_before_it() {
	let cases: [(&CStr, &[&str]); 4] = [
		(c"vkCreateBuffer", &[]),
		(c"vkAllocateMemory", &["buffer 0xb00001"]),
		(
			c"vkBindBufferMemory",
			&["buffer 0xb00001", "memory 0xe00001"],
		),
		(c"vkMapMemory", &["buffer 0xb00001", "memory 0xe00001"]),
	];
	// VK_ERROR_OUT_OF_HOST_MEMORY, _DEVICE_MEMORY and _MEMORY_MAP_FAILED.
	for (name, released) in cases {
		for code in [-1, -2, -5] {
			let device = device(&INTEL, |driver| driver.fails.push((name, code)));
			let result = Buffer::new(&device, size(8), VERTEX, Usage::Upload);
			assert_eq!(result.err(), Some(Error::Failed(code)), "{name:?}");
			drop(device);
			// Never the garbage a failed call left behind.
			let driver = finish();
			assert_eq!(driver.released, released, "{name:?}");
			assert!(driver.live_buffers.is_empty() && driver.live_memory.is_empty());
		}
	}
}

#[test]
fn buffers_keep_their_device_alive_and_go_in_any_order() {
	let device = device(&INTEL, |_| {});
	let first = Buffer::new(&device, size(1), VERTEX, Usage::Upload).unwrap();
	let second = Buffer::new(&device, size(1 << 20), TRANSFER_DST, Usage::Device).unwrap();
	let third = Buffer::new(&device, size(3), TRANSFER_DST, Usage::Readback).unwrap();
	drop(device);
	drop(second);
	assert!(with(|driver| driver.destroyed_devices.is_empty()));
	drop(first);
	drop(third);
	let driver = finish();
	assert_eq!(driver.allocations, [(256, 1), (1 << 20, 0), (256, 2)]);
	let released = [
		"buffer 0xb00002",
		"memory 0xe00002",
		"buffer 0xb00001",
		"memory 0xe00001",
		"buffer 0xb00003",
		"memory 0xe00003",
	];
	assert_eq!(driver.released, released);
	assert_eq!(driver.destroyed_devices.len(), 1);
}

#[test]
fn a_device_missing_a_buffer_function_is_not_made() {
	let names = [
		c"vkCreateBuffer",
		c"vkDestroyBuffer",
		c"vkGetBufferMemoryRequirements",
		c"vkAllocateMemory",
		c"vkFreeMemory",
		c"vkBindBufferMemory",
		c"vkMapMemory",
	];
	for name in names {
		let mut driver = with_devices(&[Described {
			name: "lacking".into(),
			kind: Kind::Discrete,
			api: api_version(1, 3),
			families: vec![family(GRAPHICS, 1)],
			swapchain: false,
		}]);
		driver.lacks.push(name);
		let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
		let physical = instance.physical_devices().unwrap()[0];
		let error = Device::new(instance, physical, 0).err();
		assert_eq!(error, Some(Error::Missing(name)));
		let driver = finish();
		assert_eq!(driver.destroyed_devices.len(), 1, "{name:?}");
	}
}

#[test]
fn the_fake_driver_reports_broken_buffer_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing.
	let device = device(&INTEL, |driver| driver.buffer_bits = Some(0b11));
	let handle = with(|driver| driver.live_devices[0]);
	let reported = || with(|driver| std::mem::take(&mut driver.invalid));
	let info = |size, sharing_mode| BufferCreateInfo {
		s_type: STRUCTURE_TYPE_BUFFER_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		size,
		usage: VERTEX,
		sharing_mode,
		queue_family_index_count: 0,
		queue_family_indices: ptr::null(),
	};
	let mut buffer = 0;
	// SAFETY: the fake reads only what the pointers point to.
	unsafe {
		create_buffer(handle, &info(0, 0), ptr::null(), &mut buffer);
		create_buffer(handle, &info(8, 1), ptr::null(), &mut buffer);
	}
	assert_eq!(
		reported(),
		["vkCreateBuffer: size", "vkCreateBuffer: exclusive"]
	);
	let allocate_info = |memory_type_index| MemoryAllocateInfo {
		s_type: STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		next: ptr::null(),
		allocation_size: 256,
		memory_type_index,
	};
	let (mut device_local, mut cached, mut unknown) = (0, 0, 0);
	// SAFETY: as above.
	unsafe {
		allocate(handle, &allocate_info(0), ptr::null(), &mut device_local);
		allocate(handle, &allocate_info(2), ptr::null(), &mut cached);
		allocate(handle, &allocate_info(3), ptr::null(), &mut unknown);
	}
	assert_eq!(reported(), ["vkAllocateMemory: memoryTypeIndex"]);
	let mut data = ptr::null_mut();
	// SAFETY: as above.
	unsafe {
		// Type 2 is not one the buffer allows; then bound twice; then a
		// fresh buffer with too little room past the offset.
		bind(handle, buffer, cached, 0);
		bind(handle, buffer, device_local, 0);
		create_buffer(handle, &info(256, 0), ptr::null(), &mut buffer);
		bind(handle, buffer, device_local, 256);
		// Device-local memory is not host-visible; cached memory twice.
		map(handle, device_local, 0, WHOLE_SIZE, 0, &mut data);
		map(handle, cached, 0, WHOLE_SIZE, 0, &mut data);
		map(handle, cached, 0, WHOLE_SIZE, 0, &mut data);
	}
	let expected = [
		"vkBindBufferMemory: a memory type it allows",
		"vkBindBufferMemory: a live buffer bound once",
		"vkBindBufferMemory: room at the offset",
		"vkMapMemory: host-visible memory",
		"vkMapMemory: memory not mapped yet",
	];
	assert_eq!(reported(), expected);
	// Destroying the device while they live, then each twice.
	let live = with(|driver| (driver.live_buffers.clone(), driver.live_memory.clone()));
	with(|driver| driver.live_devices.push(0x7777 as RawDevice));
	// SAFETY: the fake only records the handles.
	unsafe { (device.functions.free)(0x7777 as RawDevice, 0x1, ptr::null()) };
	assert_eq!(reported().len(), 1);
	with(|driver| driver.live_devices.retain(|&live| live == handle));
	drop(device);
	assert_eq!(
		reported(),
		["vkDestroyDevice while buffers or memory are live"]
	);
	with(|driver| driver.live_devices.push(handle));
	// SAFETY: as above.
	unsafe {
		for &buffer in &live.0 {
			destroy_buffer(handle, buffer, ptr::null());
			destroy_buffer(handle, buffer, ptr::null());
		}
		for &memory in &live.1 {
			free(handle, memory, ptr::null());
		}
	}
	assert_eq!(reported().len(), live.0.len());
	with(|driver| driver.live_devices.clear());
	finish();
}
