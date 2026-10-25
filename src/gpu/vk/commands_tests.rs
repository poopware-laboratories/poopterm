//! Tests of recording and submitting commands against the fake driver of
//! `driver_tests`: the pool and buffer made, what each submission signals,
//! waiting for the GPU before a buffer is recorded again or its pool goes,
//! failures, and the order things go in. The fake's command functions live
//! here and check the specification's valid usage rules, among them that
//! nothing pending is reset or destroyed.
use super::{
	buffer_tests::{GARBAGE, INTEL, device},
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind},
	commands::Commands,
	device::Device,
	driver_tests::{Driver, family, finish, install, with, with_devices},
	submit_types::*,
	timeline::Timeline,
	types::*,
	version::api_version,
};
use std::{
	ffi::{CStr, c_void},
	mem::transmute,
	ptr,
	rc::Rc,
};

/// Where a command buffer is in its lifecycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Stage {
	Initial,
	Recording,
	Executable,
	/// Submitted to raise this timeline to this value once done.
	Submitted(RawSemaphore, u64),
}

/// A command buffer the fake allocated.
#[derive(Debug)]
pub(super) struct CommandBuffer {
	pub pool: RawCommandPool,
	pub stage: Stage,
	/// The layouts its barriers leave images in, which take effect once
	/// it is submitted.
	pub layouts: Vec<(RawImage, i32)>,
}

/// `VK_COMMAND_POOL_CREATE_TRANSIENT_BIT`, `_RESET_COMMAND_BUFFER_BIT`.
const TRANSIENT: u32 = 1;
const RESET_BUFFER: u32 = 2;
/// `VK_PIPELINE_STAGE_2_ALL_COMMANDS_BIT`.
const ALL_COMMANDS: u64 = 0x1_0000;
/// `VK_ERROR_DEVICE_LOST`.
const DEVICE_LOST: i32 = -4;
/// `VK_ERROR_VALIDATION_FAILED_EXT`: what the fake answers a call that
/// breaks a rule, changing nothing.
const VALIDATION_FAILED: i32 = -1_000_011_001;

/// The fake's command function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateCommandPool" => transmute::<CreateCommandPool, VoidFunction>(create_pool),
			b"vkDestroyCommandPool" => transmute::<DestroyCommandPool, VoidFunction>(destroy_pool),
			b"vkResetCommandPool" => transmute::<ResetCommandPool, VoidFunction>(reset_pool),
			b"vkAllocateCommandBuffers" => transmute::<AllocateCommandBuffers, VoidFunction>(allocate),
			b"vkBeginCommandBuffer" => transmute::<BeginCommandBuffer, VoidFunction>(begin),
			b"vkEndCommandBuffer" => transmute::<EndCommandBuffer, VoidFunction>(end),
			b"vkQueueSubmit2" => transmute::<QueueSubmit2, VoidFunction>(submit),
			_ => return super::recording_tests::function(name),
		})
	}
}

impl Driver {
	/// How far the GPU got on `semaphore`.
	pub(super) fn reached(&self, semaphore: RawSemaphore) -> u64 {
		let index = semaphore.wrapping_sub(0xc0_0001) as usize;
		self.counters.get(index).copied().unwrap_or(0)
	}

	/// Whether the GPU has yet to finish `buffer`.
	pub(super) fn pending(&self, buffer: &CommandBuffer) -> bool {
		matches!(buffer.stage, Stage::Submitted(semaphore, value) if self.reached(semaphore) < value)
	}

	/// The command buffer behind `handle`, if the fake allocated it from a
	/// live pool.
	pub(super) fn command_buffer(&mut self, handle: RawCommandBuffer) -> Option<&mut CommandBuffer> {
		let index = (handle as usize).wrapping_sub(0x7c_0001);
		let buffer = self.command_buffers.get(index)?;
		self
			.live_pools
			.contains(&buffer.pool)
			.then_some(&mut self.command_buffers[index])
	}

	/// Breaks the test if any buffer of `pool` is pending.
	fn check_idle(&mut self, pool: RawCommandPool, call: &str) {
		let busy =
			(self.command_buffers.iter()).any(|buffer| buffer.pool == pool && self.pending(buffer));
		self.require(!busy, || format!("{call} while its buffers are pending"));
	}
}

unsafe extern "system" fn create_pool(
	device: RawDevice,
	info: *const CommandPoolCreateInfo,
	allocator: *const c_void,
	pool: *mut RawCommandPool,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		driver.check_live_device(device, "vkCreateCommandPool");
		let made = driver
			.made
			.get((device as usize).wrapping_sub(0x5100) / 0x100);
		let rules = [
			(
				info.s_type == STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
				"sType",
			),
			(
				info.next.is_null() && allocator.is_null(),
				"pNext and allocator",
			),
			(info.flags & !(TRANSIENT | RESET_BUFFER) == 0, "flags"),
			(
				made.is_some_and(|made| made.family == info.queue_family_index),
				"the device's queue family",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateCommandPool: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkCreateCommandPool") {
			// SAFETY: the bindings pass a writable handle.
			unsafe { pool.write(GARBAGE) };
			return code;
		}
		driver.pools.push((info.flags, info.queue_family_index));
		let handle = 0xd0_0000 + driver.pools.len() as u64;
		driver.live_pools.push(handle);
		// SAFETY: as above.
		unsafe { pool.write(handle) };
		0
	})
}

unsafe extern "system" fn allocate(
	device: RawDevice,
	info: *const CommandBufferAllocateInfo,
	buffers: *mut RawCommandBuffer,
) -> i32 {
	// SAFETY: the bindings pass an allocate info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		driver.check_live_device(device, "vkAllocateCommandBuffers");
		let rules = [
			(
				info.s_type == STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
				"sType",
			),
			(info.next.is_null(), "pNext"),
			(
				driver.live_pools.contains(&info.command_pool),
				"a live pool",
			),
			(info.level == 0, "primary"),
			(info.command_buffer_count == 1, "one buffer"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkAllocateCommandBuffers: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkAllocateCommandBuffers") {
			// SAFETY: the bindings pass room for one handle.
			unsafe { buffers.write(GARBAGE as RawCommandBuffer) };
			return code;
		}
		driver.command_buffers.push(CommandBuffer {
			pool: info.command_pool,
			stage: Stage::Initial,
			layouts: Vec::new(),
		});
		let handle = 0x7c_0000 + driver.command_buffers.len();
		// SAFETY: as above.
		unsafe { buffers.write(handle as RawCommandBuffer) };
		0
	})
}

unsafe extern "system" fn reset_pool(device: RawDevice, pool: RawCommandPool, flags: u32) -> i32 {
	with(|driver| {
		let before = driver.invalid.len();
		driver.check_live_device(device, "vkResetCommandPool");
		let live = driver.live_pools.contains(&pool);
		driver.require(live && flags == 0, || {
			"vkResetCommandPool: a live pool, no flags".into()
		});
		driver.check_idle(pool, "vkResetCommandPool");
		if driver.invalid.len() > before {
			return VALIDATION_FAILED;
		}
		if let Some(code) = driver.failing(c"vkResetCommandPool") {
			return code;
		}
		for buffer in driver
			.command_buffers
			.iter_mut()
			.filter(|buffer| buffer.pool == pool)
		{
			buffer.stage = Stage::Initial;
			buffer.layouts.clear();
		}
		driver.recorded.push(format!("reset {pool:#x}"));
		0
	})
}

unsafe extern "system" fn begin(
	buffer: RawCommandBuffer,
	info: *const CommandBufferBeginInfo,
) -> i32 {
	// SAFETY: the bindings pass a begin info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		let before = driver.invalid.len();
		let rules = [
			(
				info.s_type == STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
				"sType",
			),
			(info.next.is_null(), "pNext"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkBeginCommandBuffer: {rule}"));
		}
		// Without RESET_BUFFER, only a reset pool makes a buffer initial.
		let stage = driver.command_buffer(buffer).map(|buffer| buffer.stage);
		let initial = stage == Some(Stage::Initial);
		driver.require(initial, || {
			format!("vkBeginCommandBuffer: {stage:?}, not initial")
		});
		if driver.invalid.len() > before {
			return VALIDATION_FAILED;
		}
		if let Some(code) = driver.failing(c"vkBeginCommandBuffer") {
			return code;
		}
		if let Some(buffer) = driver.command_buffer(buffer) {
			buffer.stage = Stage::Recording;
		}
		driver
			.recorded
			.push(format!("begin {buffer:?} flags {}", info.flags));
		0
	})
}

unsafe extern "system" fn end(buffer: RawCommandBuffer) -> i32 {
	with(|driver| {
		let stage = driver.command_buffer(buffer).map(|buffer| buffer.stage);
		let recording = stage == Some(Stage::Recording);
		driver.require(recording, || {
			format!("vkEndCommandBuffer: {stage:?}, not recording")
		});
		if !recording {
			return VALIDATION_FAILED;
		}
		if let Some(code) = driver.failing(c"vkEndCommandBuffer") {
			return code;
		}
		if let Some(buffer) = driver.command_buffer(buffer) {
			buffer.stage = Stage::Executable;
		}
		driver.recorded.push(format!("end {buffer:?}"));
		0
	})
}

/// The `count` items at `items`, which may be null when there are none.
///
/// # Safety
///
/// A nonzero `count` of items must live at `items` for `'a`.
unsafe fn read<'a, T>(items: *const T, count: u32) -> &'a [T] {
	match count {
		0 => &[],
		// SAFETY: as the caller promised.
		count => unsafe { std::slice::from_raw_parts(items, count as usize) },
	}
}

unsafe extern "system" fn submit(
	queue: RawQueue,
	count: u32,
	submits: *const SubmitInfo2,
	fence: u64,
) -> i32 {
	// SAFETY: the bindings pass one submission whose arrays live across the
	// call, each as long as its count, or null and empty.
	let (info, buffers, signals) = unsafe {
		let info = &*submits;
		let buffers = read(info.command_buffer_infos, info.command_buffer_info_count);
		let signals = read(
			info.signal_semaphore_infos,
			info.signal_semaphore_info_count,
		);
		(info, buffers, signals)
	};
	with(|driver| {
		let before = driver.invalid.len();
		let queues = driver.live_devices.iter().zip(&driver.made);
		let known = (queues.into_iter())
			.any(|(&device, made)| queue as usize == device as usize + 1 + made.family as usize);
		let rules = [
			(known, "a live device's queue"),
			(count == 1 && fence == 0, "one submission, no fence"),
			(info.s_type == STRUCTURE_TYPE_SUBMIT_INFO_2, "sType"),
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(info.wait_semaphore_info_count == 0, "no waits"),
			(
				buffers.len() == 1 && signals.len() == 1,
				"one buffer, one signal",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkQueueSubmit2: {rule}"));
		}
		let (Some(buffer), Some(signal)) = (buffers.first(), signals.first()) else {
			return -1;
		};
		let stage = driver
			.command_buffer(buffer.command_buffer)
			.map(|buffer| buffer.stage);
		let timeline = driver.live_semaphores.contains(&signal.semaphore)
			&& driver
				.semaphores
				.get((signal.semaphore - 0xc0_0001) as usize)
				.is_some_and(|made| made.0 == 1);
		let pending = (driver.command_buffers.iter())
			.filter_map(|buffer| match buffer.stage {
				Stage::Submitted(semaphore, value) if semaphore == signal.semaphore => Some(value),
				_ => None,
			})
			.max();
		let above = signal.value > driver.reached(signal.semaphore)
			&& pending.is_none_or(|pending| signal.value > pending);
		let rules = [
			(
				buffer.s_type == STRUCTURE_TYPE_COMMAND_BUFFER_SUBMIT_INFO
					&& signal.s_type == STRUCTURE_TYPE_SEMAPHORE_SUBMIT_INFO,
				"sTypes",
			),
			(buffer.next.is_null() && signal.next.is_null(), "pNexts"),
			(
				buffer.device_mask == 0 && signal.device_index == 0,
				"one device",
			),
			(stage == Some(Stage::Executable), "an executable buffer"),
			(timeline, "a live timeline"),
			(above, "a value above the timeline's and every pending one"),
			(signal.stage_mask != 0, "a stage"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkQueueSubmit2: {rule}"));
		}
		if driver.invalid.len() > before {
			return VALIDATION_FAILED;
		}
		if let Some(code) = driver.failing(c"vkQueueSubmit2") {
			return code;
		}
		if let Some(submitted) = driver.command_buffer(buffer.command_buffer) {
			submitted.stage = Stage::Submitted(signal.semaphore, signal.value);
			let layouts = std::mem::take(&mut submitted.layouts);
			driver.image_layouts.extend(layouts);
		}
		driver.recorded.push(format!(
			"submit {:?} to {queue:?}: {:#x} to {} after {:#x}",
			buffer.command_buffer, signal.semaphore, signal.value, signal.stage_mask
		));
		0
	})
}

unsafe extern "system" fn destroy_pool(
	device: RawDevice,
	pool: RawCommandPool,
	allocator: *const c_void,
) {
	with(|driver| {
		driver.check_live_device(device, "vkDestroyCommandPool");
		let live = driver.live_pools.contains(&pool) && allocator.is_null();
		driver.require(live, || {
			format!("vkDestroyCommandPool: {pool:#x}, which is not live")
		});
		driver.check_idle(pool, "vkDestroyCommandPool");
		driver.live_pools.retain(|&other| other != pool);
		driver.released.push(format!("pool {pool:#x}"));
	})
}

/// A timeline on a fake device, and commands that raise it.
fn make(change: impl FnOnce(&mut Driver)) -> (Rc<Timeline>, Commands) {
	let device = device(&INTEL, change);
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let commands = Commands::new(&timeline);
	(timeline, commands.expect("commands"))
}

/// The fake's command buffer handles, as the bindings see them.
fn buffer(n: usize) -> RawCommandBuffer {
	(0x7c_0000 + n) as RawCommandBuffer
}

/// Sets how far the fake's GPU got on the first timeline.
fn reach(value: u64) {
	with(|driver| driver.counters[0] = value);
}

#[test]
fn commands_are_recorded_and_submitted_to_raise_the_timeline() {
	let (timeline, mut commands) = make(|_| {});
	assert_eq!(commands.submitted(), 0);
	assert_eq!(commands.record().unwrap().submit(), Ok(1));
	assert_eq!(commands.submitted(), 1);
	reach(1);
	assert_eq!(commands.record().unwrap().submit(), Ok(2));
	drop(commands);
	drop(timeline);
	let driver = finish();
	// A transient pool on the device's family, one primary buffer.
	assert_eq!(driver.pools, [(TRANSIENT, 0)]);
	assert_eq!(driver.command_buffers.len(), 1);
	let queue = with_queue(&driver);
	let recorded = [
		"reset 0xd00001".to_string(),
		format!("begin {:?} flags 1", buffer(1)),
		format!("end {:?}", buffer(1)),
		format!(
			"submit {:?} to {queue}: 0xc00001 to 1 after 0x10000",
			buffer(1)
		),
		"reset 0xd00001".to_string(),
		format!("begin {:?} flags 1", buffer(1)),
		format!("end {:?}", buffer(1)),
		format!(
			"submit {:?} to {queue}: 0xc00001 to 2 after 0x10000",
			buffer(1)
		),
	];
	assert_eq!(driver.recorded, recorded);
	assert_eq!(driver.released, ["pool 0xd00001", "semaphore 0xc00001"]);
}

/// How the fake's queue of its one device prints.
fn with_queue(driver: &Driver) -> String {
	format!(
		"{:?}",
		(driver.destroyed_devices[0] as usize + 1) as RawQueue
	)
}

#[test]
fn recording_again_waits_for_the_gpu_to_finish_the_last_submission() {
	let (timeline, mut commands) = make(|_| {});
	// Nothing to wait for before the first submission.
	commands.record().unwrap().submit().unwrap();
	assert_eq!(timeline.reached(), Ok(0));
	// The fake's GPU finishes once waited for without a timeout.
	commands.record().unwrap();
	assert_eq!(timeline.reached(), Ok(1));
	assert_eq!(commands.finish(), Ok(()));
	drop(commands);
	let waits: Vec<_> = (with(|driver| driver.waits.clone()).into_iter())
		.map(|(_, values, timeout)| (values[0], timeout))
		.collect();
	assert_eq!(
		waits,
		[(0, u64::MAX), (1, u64::MAX), (1, u64::MAX), (1, u64::MAX)]
	);
	drop(timeline);
	finish();
}

#[test]
fn a_recording_dropped_unsubmitted_is_discarded() {
	let (timeline, mut commands) = make(|_| {});
	commands.record().unwrap();
	assert_eq!(commands.submitted(), 0);
	// The pool's reset discards the half-recorded buffer.
	assert_eq!(commands.record().unwrap().submit(), Ok(1));
	drop(commands);
	drop(timeline);
	let recorded = finish().recorded;
	assert_eq!(
		recorded
			.iter()
			.filter(|call| call.starts_with("begin"))
			.count(),
		2
	);
	assert_eq!(
		recorded
			.iter()
			.filter(|call| call.starts_with("end"))
			.count(),
		1
	);
}

#[test]
fn commands_sharing_a_timeline_raise_it_in_turn_and_wait_only_for_their_own() {
	let (timeline, mut first) = make(|_| {});
	let mut second = Commands::new(&timeline).unwrap();
	assert_eq!(first.record().unwrap().submit(), Ok(1));
	assert_eq!(second.record().unwrap().submit(), Ok(2));
	reach(1);
	assert_eq!(first.record().unwrap().submit(), Ok(3));
	// The first's buffer is done with 1; the second waits for 2 alone.
	assert_eq!(timeline.reached(), Ok(1));
	reach(2);
	drop(second);
	reach(3);
	drop(first);
	drop(timeline);
	let driver = finish();
	let values: Vec<_> = driver
		.waits
		.iter()
		.map(|(_, values, _)| values[0])
		.collect();
	assert_eq!(values, [0, 0, 1, 2, 3]);
	assert_eq!(driver.pools.len(), 2);
	assert_eq!(
		driver.released,
		["pool 0xd00002", "pool 0xd00001", "semaphore 0xc00001"]
	);
}

#[test]
fn a_failing_call_is_an_error_and_undoes_what_was_made() {
	for code in [-1, -2] {
		let cases: [(&CStr, &[&str]); 2] = [
			(c"vkCreateCommandPool", &["semaphore 0xc00001"]),
			(
				c"vkAllocateCommandBuffers",
				&["pool 0xd00001", "semaphore 0xc00001"],
			),
		];
		for (name, released) in cases {
			let made = device(&INTEL, |driver| driver.fails.push((name, code)));
			let timeline = Rc::new(Timeline::new(&made).unwrap());
			assert_eq!(Commands::new(&timeline).err(), Some(Error::Failed(code)));
			drop((made, timeline));
			assert_eq!(finish().released, released, "{name:?}");
		}
	}
	for name in [c"vkResetCommandPool", c"vkBeginCommandBuffer"] {
		let (timeline, mut commands) = make(|driver| driver.fails.push((name, -1)));
		assert_eq!(commands.record().err(), Some(Error::Failed(-1)), "{name:?}");
		drop((commands, timeline));
		finish();
	}
}

#[test]
fn a_failed_submission_raises_nothing_and_the_next_skips_its_value() {
	for (name, code) in [
		(c"vkEndCommandBuffer", -1),
		(c"vkQueueSubmit2", -2),
		(c"vkQueueSubmit2", DEVICE_LOST),
	] {
		let (timeline, mut commands) = make(|driver| driver.fails.push((name, code)));
		assert_eq!(
			commands.record().unwrap().submit(),
			Err(Error::Failed(code))
		);
		assert_eq!(commands.submitted(), 0);
		// Nothing is pending, so recording again waits for nothing new.
		with(|driver| driver.fails.clear());
		let value = commands.record().unwrap().submit().unwrap();
		assert!(value >= 1, "{name:?}");
		assert_eq!(commands.submitted(), value);
		drop((commands, timeline));
		finish();
	}
}

#[test]
fn dropping_commands_waits_for_the_gpu_even_when_the_device_is_lost() {
	let (timeline, mut commands) = make(|_| {});
	commands.record().unwrap().submit().unwrap();
	drop(commands);
	// Waited for forever, the fake's GPU finished before the pool went.
	assert_eq!(timeline.reached(), Ok(1));
	drop(timeline);
	finish();
	let (timeline, mut commands) = make(|_| {});
	commands.record().unwrap().submit().unwrap();
	with(|driver| driver.fails.push((c"vkWaitSemaphores", DEVICE_LOST)));
	assert_eq!(commands.finish(), Err(Error::Failed(DEVICE_LOST)));
	// A lost device has finished everything, so the pool still goes.
	reach(1);
	drop(commands);
	drop(timeline);
	assert_eq!(finish().released, ["pool 0xd00001", "semaphore 0xc00001"]);
}

#[test]
fn commands_keep_their_timeline_and_device_alive() {
	let device = device(&INTEL, |_| {});
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let mut commands = Commands::new(&timeline).unwrap();
	drop(device);
	drop(timeline);
	assert_eq!(commands.record().unwrap().submit(), Ok(1));
	assert!(with(|driver| driver.destroyed_devices.is_empty()));
	drop(commands);
	let driver = finish();
	assert_eq!(driver.released, ["pool 0xd00001", "semaphore 0xc00001"]);
	assert_eq!(driver.destroyed_devices.len(), 1);
}

#[test]
fn a_device_missing_a_command_function_is_not_made() {
	let names = [
		c"vkCreateCommandPool",
		c"vkDestroyCommandPool",
		c"vkResetCommandPool",
		c"vkAllocateCommandBuffers",
		c"vkBeginCommandBuffer",
		c"vkEndCommandBuffer",
		c"vkQueueSubmit2",
	];
	for name in names {
		let mut driver = with_devices(&[Described {
			name: "lacking".into(),
			kind: Kind::Integrated,
			api: api_version(1, 3),
			families: vec![family(GRAPHICS, 1)],
			swapchain: true,
		}]);
		driver.lacks.push(name);
		let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
		let physical = instance.physical_devices().unwrap()[0];
		assert_eq!(
			Device::new(instance, physical, 0).err(),
			Some(Error::Missing(name))
		);
		assert_eq!(finish().destroyed_devices.len(), 1, "{name:?}");
	}
}

#[test]
fn the_fake_driver_reports_broken_command_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing. Everything is made by hand, so
	// nothing is released on drop.
	let device = device(&INTEL, |_| {});
	let handle = with(|driver| driver.live_devices[0]);
	let queue = (handle as usize + 1) as RawQueue;
	let reported = || with(|driver| std::mem::take(&mut driver.invalid));
	let functions = &device.functions;
	let kind = SemaphoreTypeCreateInfo {
		s_type: STRUCTURE_TYPE_SEMAPHORE_TYPE_CREATE_INFO,
		next: ptr::null(),
		semaphore_type: 1,
		initial_value: 0,
	};
	let semaphore_info = SemaphoreCreateInfo {
		s_type: STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO,
		next: ptr::from_ref(&kind).cast(),
		flags: 0,
	};
	let pool_info = |flags, queue_family_index| CommandPoolCreateInfo {
		s_type: STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
		next: ptr::null(),
		flags,
		queue_family_index,
	};
	let allocate_info = |command_pool, level| CommandBufferAllocateInfo {
		s_type: STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
		next: ptr::null(),
		command_pool,
		level,
		command_buffer_count: 1,
	};
	let begin_info = CommandBufferBeginInfo {
		s_type: STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
		next: ptr::null(),
		flags: 0,
		inheritance_info: ptr::null(),
	};
	let (mut semaphore, mut pool, mut other) = (0, 0, 0);
	let (mut first, mut second) = (ptr::null_mut(), ptr::null_mut());
	// SAFETY: the fake reads only what the pointers point to.
	unsafe {
		(functions.create_semaphore)(handle, &semaphore_info, ptr::null(), &mut semaphore);
		// Unknown flags, another family, and a secondary buffer.
		(functions.create_pool)(handle, &pool_info(4, 0), ptr::null(), &mut other);
		(functions.create_pool)(handle, &pool_info(0, 1), ptr::null(), &mut other);
		(functions.create_pool)(handle, &pool_info(TRANSIENT, 0), ptr::null(), &mut pool);
		(functions.allocate_commands)(handle, &allocate_info(pool, 1), &mut first);
		(functions.allocate_commands)(handle, &allocate_info(pool, 0), &mut second);
	}
	let expected = [
		"vkCreateCommandPool: flags",
		"vkCreateCommandPool: the device's queue family",
		"vkAllocateCommandBuffers: primary",
	];
	assert_eq!(reported(), expected);
	let submit_to = |command_buffer, queue, value, stage_mask| {
		let buffers = CommandBufferSubmitInfo {
			s_type: STRUCTURE_TYPE_COMMAND_BUFFER_SUBMIT_INFO,
			next: ptr::null(),
			command_buffer,
			device_mask: 0,
		};
		let signal = SemaphoreSubmitInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_SUBMIT_INFO,
			next: ptr::null(),
			semaphore,
			value,
			stage_mask,
			device_index: 0,
		};
		let info = SubmitInfo2 {
			s_type: STRUCTURE_TYPE_SUBMIT_INFO_2,
			next: ptr::null(),
			flags: 0,
			wait_semaphore_info_count: 0,
			wait_semaphore_infos: ptr::null(),
			command_buffer_info_count: 1,
			command_buffer_infos: &buffers,
			signal_semaphore_info_count: 1,
			signal_semaphore_infos: &signal,
		};
		// SAFETY: as above.
		unsafe { (functions.submit)(queue, 1, &info, 0) }
	};
	// A call that breaks a rule changes nothing and fails.
	let invalid = VALIDATION_FAILED;
	// SAFETY: as above.
	unsafe {
		// Ending and submitting a buffer never begun, then beginning one twice.
		assert_eq!((functions.end)(first), invalid);
		assert_eq!(submit_to(first, queue, 1, ALL_COMMANDS), invalid);
		(functions.begin)(first, &begin_info);
		assert_eq!((functions.begin)(first, &begin_info), invalid);
		(functions.end)(first);
		// Another queue, no stage, and a value the timeline is not above.
		submit_to(first, (handle as usize + 2) as RawQueue, 1, ALL_COMMANDS);
		submit_to(first, queue, 1, 0);
		reach(3);
		submit_to(first, queue, 3, ALL_COMMANDS);
		// Raising it to 5, then to 5 again before the GPU gets there.
		assert_eq!(submit_to(first, queue, 5, ALL_COMMANDS), 0);
		(functions.begin)(second, &begin_info);
		(functions.end)(second);
		submit_to(second, queue, 5, ALL_COMMANDS);
		// Resetting or destroying the pool or the timeline meanwhile.
		(functions.reset_pool)(handle, pool, 0);
		(functions.destroy_pool)(handle, pool, ptr::null());
		(functions.destroy_semaphore)(handle, semaphore, ptr::null());
	}
	let expected = [
		"vkEndCommandBuffer: Some(Initial), not recording",
		"vkQueueSubmit2: an executable buffer",
		"vkBeginCommandBuffer: Some(Recording), not initial",
		"vkQueueSubmit2: a live device's queue",
		"vkQueueSubmit2: a stage",
		"vkQueueSubmit2: a value above the timeline's and every pending one",
		"vkQueueSubmit2: a value above the timeline's and every pending one",
		"vkResetCommandPool while its buffers are pending",
		"vkDestroyCommandPool while its buffers are pending",
		"vkDestroySemaphore while work that signals it is pending",
	];
	assert_eq!(reported(), expected);
	// With the pool and timeline gone, its buffers are no longer there to
	// record or submit, nor the timeline to raise.
	// SAFETY: as above.
	unsafe { (functions.begin)(first, &begin_info) };
	submit_to(second, queue, 6, ALL_COMMANDS);
	let expected = [
		"vkBeginCommandBuffer: None, not initial",
		"vkQueueSubmit2: an executable buffer",
		"vkQueueSubmit2: a live timeline",
	];
	assert_eq!(reported(), expected);
	// The other pool outlives the device.
	drop(device);
	assert_eq!(reported(), ["vkDestroyDevice while command pools are live"]);
	with(|driver| driver.live_pools.clear());
	finish();
}
