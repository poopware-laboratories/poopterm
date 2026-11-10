//! Tests of timing frames on the GPU against the fake driver of
//! `driver_tests`: the time between a submission's two timestamps, read
//! when the next is recorded, on devices that can write them and only
//! those. The fake's query functions live here and check the
//! specification's valid usage rules.
use super::{
	buffer_tests::{INTEL, device},
	commands::Commands,
	driver_tests::{Driver, finish, with},
	stamps::*,
	submit_types::RawCommandBuffer,
	timeline::Timeline,
	types::*,
};
use std::{
	ffi::{CStr, c_void},
	mem::transmute,
	rc::Rc,
	time::Duration,
};

/// The fake's query pools: each one's queries, and whether each was reset
/// and not yet written.
#[derive(Debug, Default)]
pub(super) struct Pools {
	pub live: Vec<(RawQueryPool, Vec<bool>)>,
	pub made: usize,
}

pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateQueryPool" => transmute::<CreateQueryPool, VoidFunction>(create),
			b"vkDestroyQueryPool" => transmute::<DestroyQueryPool, VoidFunction>(destroy),
			b"vkCmdResetQueryPool" => transmute::<CmdResetQueryPool, VoidFunction>(reset),
			b"vkCmdWriteTimestamp2" => transmute::<CmdWriteTimestamp2, VoidFunction>(write),
			b"vkGetQueryPoolResults" => transmute::<GetQueryPoolResults, VoidFunction>(results),
			_ => return None,
		})
	}
}

unsafe extern "system" fn create(
	device: RawDevice,
	info: *const QueryPoolCreateInfo,
	allocator: *const c_void,
	pool: *mut RawQueryPool,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		driver.check_live_device(device, "vkCreateQueryPool");
		let rules = [
			(
				info.s_type == STRUCTURE_TYPE_QUERY_POOL_CREATE_INFO,
				"sType",
			),
			(
				info.next.is_null() && info.flags == 0 && allocator.is_null(),
				"pNext, flags and allocator",
			),
			(info.query_type == TIMESTAMP, "timestamps"),
			(info.query_count > 0, "queryCount"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateQueryPool: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkCreateQueryPool") {
			return code;
		}
		driver.query_pools.made += 1;
		let handle = 0x9900 + driver.query_pools.made as RawQueryPool;
		let queries = vec![false; info.query_count as usize];
		driver.query_pools.live.push((handle, queries));
		// SAFETY: the bindings pass a writable handle.
		unsafe { pool.write(handle) };
		0
	})
}

unsafe extern "system" fn destroy(device: RawDevice, pool: RawQueryPool, _: *const c_void) {
	with(|driver| {
		driver.check_live_device(device, "vkDestroyQueryPool");
		let before = driver.query_pools.live.len();
		driver.query_pools.live.retain(|(live, _)| *live != pool);
		let gone = driver.query_pools.live.len() < before;
		driver.require(gone, || "vkDestroyQueryPool: a live pool".into());
	})
}

impl Driver {
	/// The queries of live `pool`, recording a broken rule of `call` if
	/// there is no such pool.
	fn queries(&mut self, call: &str, pool: RawQueryPool) -> Option<&mut Vec<bool>> {
		let found = self.query_pools.live.iter().any(|(live, _)| *live == pool);
		self.require(found, || format!("{call}: a live pool"));
		let mut live = self.query_pools.live.iter_mut();
		live
			.find(|(live, _)| *live == pool)
			.map(|(_, queries)| queries)
	}
}

unsafe extern "system" fn reset(_: RawCommandBuffer, pool: RawQueryPool, first: u32, count: u32) {
	with(|driver| {
		let call = "vkCmdResetQueryPool";
		let Some(queries) = driver.queries(call, pool) else {
			return;
		};
		let range = first as usize..(first + count) as usize;
		let inside = range.end <= queries.len();
		if inside {
			queries[range.clone()].fill(true);
		}
		driver.require(inside, || format!("{call}: queries in the pool"));
		driver
			.stamped
			.push(format!("reset {}..{}", range.start, range.end));
	})
}

unsafe extern "system" fn write(_: RawCommandBuffer, stage: u64, pool: RawQueryPool, query: u32) {
	with(|driver| {
		let call = "vkCmdWriteTimestamp2";
		let Some(queries) = driver.queries(call, pool) else {
			return;
		};
		let reset = queries.get(query as usize).copied() == Some(true);
		if reset {
			queries[query as usize] = false;
		}
		driver.require(reset, || {
			format!("{call}: a query reset and not written since")
		});
		driver.require(stage.is_power_of_two(), || format!("{call}: one stage"));
		driver
			.stamped
			.push(format!("timestamp {query} after {stage:#x}"));
	})
}

#[allow(clippy::too_many_arguments)]
unsafe extern "system" fn results(
	device: RawDevice,
	pool: RawQueryPool,
	first: u32,
	count: u32,
	size: usize,
	data: *mut c_void,
	stride: u64,
	flags: u32,
) -> i32 {
	with(|driver| {
		let call = "vkGetQueryPoolResults";
		driver.check_live_device(device, call);
		let Some(queries) = driver.queries(call, pool) else {
			return -3;
		};
		let inside = (first + count) as usize <= queries.len();
		let rules = [
			(inside, "queries in the pool"),
			(flags == RESULT_64, "64-bit results"),
			(stride == 8 && size >= 8 * count as usize, "room for each"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("{call}: {rule}"));
		}
		driver
			.stamped
			.push(format!("results {first}..{}", first + count));
		let answer = driver.query_answer.unwrap_or(0);
		if answer == 0 {
			let values = driver.timestamps;
			for index in 0..count.min(2) as usize {
				// SAFETY: checked above to have room for `count` 64-bit values.
				unsafe {
					data
						.cast::<u64>()
						.add(index)
						.write(values[first as usize + index])
				};
			}
		}
		answer
	})
}

/// Timed commands on a fake Intel device changed by `change`.
fn timed(change: impl FnOnce(&mut Driver)) -> (Rc<Timeline>, Commands) {
	let device = device(&INTEL, change);
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let commands = Commands::timed(&timeline).expect("commands");
	(timeline, commands)
}

/// Records and submits a frame, the GPU finishing it at once.
fn frame(commands: &mut Commands) {
	let value = commands.record().unwrap().submit().unwrap();
	with(|driver| driver.counters[0] = value);
}

fn stamped() -> Vec<String> {
	with(|driver| std::mem::take(&mut driver.stamped))
}

#[test]
fn a_frames_gpu_time_is_read_when_the_next_is_recorded() {
	// 400 000 ticks of 52.083 ns, Intel's period: 20.8 ms.
	let (timeline, mut commands) = timed(|driver| {
		driver.timestamps = [1_000_000, 1_400_000];
		driver.timestamp_period = Some(52.083);
	});
	assert_eq!(commands.gpu_time(), None);
	frame(&mut commands);
	// Not read until the next frame waits for this one.
	assert_eq!(commands.gpu_time(), None);
	let recording = commands.record().unwrap();
	drop(recording);
	assert_eq!(commands.gpu_time(), Some(Duration::from_nanos(20_833_200)));
	let expected = [
		"reset 0..2",
		"timestamp 0 after 0x1",
		"timestamp 1 after 0x10000",
		"results 0..2",
		"reset 0..2",
		"timestamp 0 after 0x1",
	];
	assert_eq!(stamped(), expected);
	drop((commands, timeline));
	assert!(
		finish().query_pools.live.is_empty(),
		"the pool goes with the commands"
	);
}

#[test]
fn a_recording_dropped_unsubmitted_has_no_time() {
	let (timeline, mut commands) = timed(|_| {});
	frame(&mut commands);
	drop(commands.record().unwrap());
	assert!(commands.gpu_time().is_some());
	// Its timestamps were never written: nothing to read.
	drop(commands.record().unwrap());
	assert_eq!(commands.gpu_time(), None);
	drop((commands, timeline));
	finish();
}

#[test]
fn results_the_driver_does_not_have_are_no_time() {
	// VK_NOT_READY, as when the GPU never ran the commands.
	let (timeline, mut commands) = timed(|driver| driver.query_answer = Some(1));
	frame(&mut commands);
	drop(commands.record().unwrap());
	assert_eq!(commands.gpu_time(), None);
	drop((commands, timeline));
	finish();
}

#[test]
fn a_queue_without_timestamps_or_a_device_without_a_period_times_nothing() {
	for change in [
		(|driver: &mut Driver| driver.timestamp_bits = Some(0)) as fn(&mut Driver),
		|driver| driver.timestamp_period = Some(0.0),
		|driver| driver.timestamp_period = Some(f32::NAN),
		|driver| driver.lacks = vec![c"vkCmdWriteTimestamp2"],
	] {
		let (timeline, mut commands) = timed(change);
		frame(&mut commands);
		drop(commands.record().unwrap());
		assert_eq!(commands.gpu_time(), None);
		assert_eq!(stamped(), Vec::<String>::new());
		drop((commands, timeline));
		assert_eq!(finish().query_pools.made, 0, "no pool made");
	}
}

#[test]
fn untimed_commands_write_no_timestamps() {
	let device = device(&INTEL, |_| {});
	let timeline = Rc::new(Timeline::new(&device).unwrap());
	let mut commands = Commands::new(&timeline).unwrap();
	frame(&mut commands);
	drop(commands.record().unwrap());
	assert_eq!(commands.gpu_time(), None);
	assert_eq!(stamped(), Vec::<String>::new());
	drop((commands, timeline, device));
	finish();
}

#[test]
fn the_time_between_timestamps_counts_only_their_valid_bits() {
	let ms = Duration::from_millis;
	assert_eq!(elapsed([0, 1_000_000], 64, 1.0), ms(1));
	assert_eq!(elapsed([5, 5], 64, 1.0), Duration::ZERO);
	// A 36-bit counter that wrapped between the two.
	let top = (1u64 << 36) - 1_000;
	assert_eq!(elapsed([top, 999_000], 36, 1.0), ms(1));
	// Bits past the valid ones are noise.
	assert_eq!(
		elapsed([0xff << 40, (0xee << 40) + 2_000_000], 40, 0.5),
		ms(1)
	);
	// AMD's 10 ns and Intel's 52.083 ns periods.
	assert_eq!(elapsed([0, 100_000], 64, 10.0), ms(1));
	assert_eq!(
		elapsed([0, 19_200], 64, 52.083),
		Duration::from_nanos(999_993)
	);
}

#[test]
fn the_fake_driver_checks_the_query_rules() {
	// Were it blind to them, the tests above would prove nothing.
	let (timeline, mut commands) = timed(|_| {});
	let pool = with(|driver| driver.query_pools.live[0].0);
	let buffer = std::ptr::null_mut();
	// SAFETY: the fake only records what it is given.
	unsafe {
		write(buffer, 0x1, pool, 0);
		reset(buffer, pool, 1, 2);
		write(buffer, 0x3, pool, 1);
		write(buffer, 0x1, 0xbad, 0);
	}
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	let expected = [
		"vkCmdWriteTimestamp2: a query reset and not written since",
		"vkCmdResetQueryPool: queries in the pool",
		"vkCmdWriteTimestamp2: a query reset and not written since",
		"vkCmdWriteTimestamp2: one stage",
		"vkCmdWriteTimestamp2: a live pool",
	];
	assert_eq!(invalid, expected);
	stamped();
	frame(&mut commands);
	drop((commands, timeline));
	finish();
}

#[test]
fn the_query_pool_info_and_constants_are_as_vulkan_declares_them() {
	use std::mem::{offset_of, size_of};
	assert_eq!(size_of::<QueryPoolCreateInfo>(), 32);
	assert_eq!(
		[
			offset_of!(QueryPoolCreateInfo, flags),
			offset_of!(QueryPoolCreateInfo, query_type),
			offset_of!(QueryPoolCreateInfo, query_count),
			offset_of!(QueryPoolCreateInfo, pipeline_statistics),
		],
		[16, 20, 24, 28]
	);
	assert_eq!(STRUCTURE_TYPE_QUERY_POOL_CREATE_INFO, 11);
	assert_eq!((TIMESTAMP, RESULT_64), (2, 1));
	assert_eq!((TOP_OF_PIPE, ALL_COMMANDS), (0x1, 0x1_0000));
	// `timestampPeriod`'s place among the limits, as a 32-bit word.
	assert_eq!(super::types::TIMESTAMP_PERIOD, 106);
}
