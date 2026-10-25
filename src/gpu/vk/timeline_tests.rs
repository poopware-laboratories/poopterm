//! Tests of timeline semaphores against the fake driver of `driver_tests`:
//! what each is made with, reading and waiting for how far the GPU got,
//! timeouts, failures, and the order things go in. The fake's semaphore
//! functions live here and check the specification's valid usage rules.
use super::{
	buffer_tests::{GARBAGE, INTEL, device},
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind},
	commands_tests::Stage,
	device::Device,
	driver_tests::{family, finish, install, with, with_devices},
	timeline::Timeline,
	types::*,
	version::api_version,
};
use std::{
	ffi::{CStr, c_void},
	mem::transmute,
	ptr,
	rc::Rc,
	slice,
	time::Duration,
};

/// `VK_TIMEOUT`.
const TIMEOUT: i32 = 2;
/// `VK_ERROR_DEVICE_LOST`.
const DEVICE_LOST: i32 = -4;

/// The fake's semaphore function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateSemaphore" => transmute::<CreateSemaphore, VoidFunction>(create),
			b"vkDestroySemaphore" => transmute::<DestroySemaphore, VoidFunction>(destroy),
			b"vkGetSemaphoreCounterValue" => transmute::<GetSemaphoreCounterValue, VoidFunction>(counter),
			b"vkWaitSemaphores" => transmute::<WaitSemaphores, VoidFunction>(wait),
			_ => return super::commands_tests::function(name),
		})
	}
}

/// The index of `semaphore` in `semaphores`, as the fake numbers them.
fn index(semaphore: RawSemaphore) -> usize {
	semaphore.wrapping_sub(0xc0_0001) as usize
}

unsafe extern "system" fn create(
	device: RawDevice,
	info: *const SemaphoreCreateInfo,
	allocator: *const c_void,
	semaphore: *mut RawSemaphore,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call,
	// and a chain of structures of the types they name.
	let (info, kind) = unsafe {
		let info = &*info;
		let chained = info.next.cast::<SemaphoreTypeCreateInfo>();
		let typed =
			!chained.is_null() && (*chained).s_type == STRUCTURE_TYPE_SEMAPHORE_TYPE_CREATE_INFO;
		(info, typed.then(|| &*chained))
	};
	with(|driver| {
		driver.check_live_device(device, "vkCreateSemaphore");
		let rules = [
			(info.s_type == STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO, "sType"),
			(
				info.flags == 0 && allocator.is_null(),
				"no flags or allocator",
			),
			(
				kind.is_some_and(|kind| kind.next.is_null()),
				"a type chained last",
			),
			(
				kind.is_some_and(|kind| matches!(kind.semaphore_type, 0 | 1)),
				"semaphoreType",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateSemaphore: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkCreateSemaphore") {
			// SAFETY: the bindings pass a writable handle.
			unsafe { semaphore.write(GARBAGE) };
			return code;
		}
		let (kind, initial) = kind.map_or((-1, 0), |kind| (kind.semaphore_type, kind.initial_value));
		driver.semaphores.push((kind, initial));
		driver.counters.push(initial);
		let handle = 0xc0_0000 + driver.semaphores.len() as u64;
		driver.live_semaphores.push(handle);
		// SAFETY: as above.
		unsafe { semaphore.write(handle) };
		0
	})
}

/// Breaks the test unless `semaphore` is a live timeline.
fn check_timeline(driver: &mut super::driver_tests::Driver, semaphore: RawSemaphore, call: &str) {
	let live = driver.live_semaphores.contains(&semaphore);
	let timeline = driver
		.semaphores
		.get(index(semaphore))
		.is_some_and(|made| made.0 == 1);
	driver.require(live && timeline, || {
		format!("{call}: {semaphore:#x}, which is not a live timeline")
	});
}

unsafe extern "system" fn counter(
	device: RawDevice,
	semaphore: RawSemaphore,
	value: *mut u64,
) -> i32 {
	with(|driver| {
		driver.check_live_device(device, "vkGetSemaphoreCounterValue");
		check_timeline(driver, semaphore, "vkGetSemaphoreCounterValue");
		if let Some(code) = driver.failing(c"vkGetSemaphoreCounterValue") {
			// SAFETY: the bindings pass room for the value.
			unsafe { value.write(GARBAGE) };
			return code;
		}
		let reached = driver.counters.get(index(semaphore)).copied().unwrap_or(0);
		// SAFETY: as above.
		unsafe { value.write(reached) };
		0
	})
}

unsafe extern "system" fn wait(
	device: RawDevice,
	info: *const SemaphoreWaitInfo,
	timeout: u64,
) -> i32 {
	// SAFETY: the bindings pass a wait info that lives across the call, and
	// `semaphore_count` of each.
	let (info, semaphores, values) = unsafe {
		let info = &*info;
		let count = info.semaphore_count as usize;
		let semaphores = slice::from_raw_parts(info.semaphores, count).to_vec();
		(
			info,
			semaphores,
			slice::from_raw_parts(info.values, count).to_vec(),
		)
	};
	with(|driver| {
		driver.check_live_device(device, "vkWaitSemaphores");
		let rules = [
			(info.s_type == STRUCTURE_TYPE_SEMAPHORE_WAIT_INFO, "sType"),
			(info.next.is_null(), "pNext"),
			(info.flags == 0, "waits for every semaphore"),
			(info.semaphore_count > 0, "semaphoreCount"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkWaitSemaphores: {rule}"));
		}
		for &semaphore in &semaphores {
			check_timeline(driver, semaphore, "vkWaitSemaphores");
		}
		let reached = semaphores.iter().zip(&values).all(|(&semaphore, &value)| {
			driver
				.counters
				.get(index(semaphore))
				.is_some_and(|&reached| reached >= value)
		});
		driver
			.waits
			.push((semaphores.clone(), values.clone(), timeout));
		if let Some(code) = driver.failing(c"vkWaitSemaphores") {
			return code;
		}
		// The fake's GPU finishes only when waited for without a timeout.
		if timeout == u64::MAX {
			for (&semaphore, &value) in semaphores.iter().zip(&values) {
				if let Some(counter) = driver.counters.get_mut(index(semaphore)) {
					*counter = value.max(*counter);
				}
			}
			return 0;
		}
		if reached { 0 } else { TIMEOUT }
	})
}

unsafe extern "system" fn destroy(
	device: RawDevice,
	semaphore: RawSemaphore,
	allocator: *const c_void,
) {
	with(|driver| {
		driver.check_live_device(device, "vkDestroySemaphore");
		let live = driver.live_semaphores.contains(&semaphore) && allocator.is_null();
		driver.require(live, || {
			format!("vkDestroySemaphore: {semaphore:#x}, which is not live")
		});
		let signalled = (driver.command_buffers.iter()).any(|buffer| {
			matches!(buffer.stage, Stage::Submitted(signalled, _) if signalled == semaphore)
				&& driver.pending(buffer)
		});
		driver.require(!signalled, || {
			"vkDestroySemaphore while work that signals it is pending".into()
		});
		driver.live_semaphores.retain(|&other| other != semaphore);
		driver.released.push(format!("semaphore {semaphore:#x}"));
	})
}

/// Sets how far the fake's GPU got on the `n`th semaphore made.
fn reach(n: usize, value: u64) {
	with(|driver| driver.counters[n] = value);
}

#[test]
fn a_timeline_starts_at_zero_and_reads_how_far_the_gpu_got() {
	let device = device(&INTEL, |_| {});
	let timeline = Timeline::new(&device).expect("timeline");
	assert_eq!(timeline.handle(), 0xc0_0001);
	assert_eq!(timeline.reached(), Ok(0));
	reach(0, 7);
	assert_eq!(timeline.reached(), Ok(7));
	reach(0, u64::MAX);
	assert_eq!(timeline.reached(), Ok(u64::MAX));
	drop(timeline);
	drop(device);
	let driver = finish();
	// VK_SEMAPHORE_TYPE_TIMELINE from zero, not a binary semaphore.
	assert_eq!(driver.semaphores, [(1, 0)]);
	assert_eq!(driver.released, ["semaphore 0xc00001"]);
}

#[test]
fn waiting_says_whether_the_gpu_reached_the_value_in_time() {
	let device = device(&INTEL, |_| {});
	let timeline = Timeline::new(&device).unwrap();
	reach(0, 3);
	assert_eq!(timeline.wait(3, Duration::ZERO), Ok(true));
	assert_eq!(timeline.wait(0, Duration::from_millis(1)), Ok(true));
	assert_eq!(timeline.wait(4, Duration::from_nanos(1)), Ok(false));
	// 2^64 ns, just past what Vulkan's timeout holds: as good as forever,
	// which the fake's GPU answers by finishing.
	let past = Duration::new(18_446_744_073, 709_551_616);
	assert_eq!(timeline.wait(4, past), Ok(true));
	assert_eq!(timeline.reached(), Ok(4));
	assert_eq!(timeline.wait(u64::MAX, Duration::MAX), Ok(true));
	assert_eq!(timeline.reached(), Ok(u64::MAX));
	drop(timeline);
	drop(device);
	let waits: Vec<_> = (finish().waits.into_iter())
		.map(|(semaphores, values, timeout)| (semaphores, values[0], timeout))
		.collect();
	let one = || vec![0xc0_0001];
	let expected = [
		(one(), 3, 0),
		(one(), 0, 1_000_000),
		(one(), 4, 1),
		(one(), 4, u64::MAX),
		(one(), u64::MAX, u64::MAX),
	];
	assert_eq!(waits, expected);
}

#[test]
fn failures_are_errors_and_a_failed_timeline_is_never_destroyed() {
	let failing =
		|name: &'static CStr, code| device(&INTEL, move |driver| driver.fails.push((name, code)));
	for code in [-1, -2] {
		let device = failing(c"vkCreateSemaphore", code);
		assert_eq!(Timeline::new(&device).err(), Some(Error::Failed(code)));
		drop(device);
		assert!(finish().released.is_empty());
	}
	// A lost device is an error, never the garbage the call left behind.
	let device = failing(c"vkGetSemaphoreCounterValue", DEVICE_LOST);
	let timeline = Timeline::new(&device).unwrap();
	assert_eq!(timeline.reached(), Err(Error::Failed(DEVICE_LOST)));
	drop(timeline);
	drop(device);
	finish();
	for code in [-1, -2, DEVICE_LOST] {
		let device = failing(c"vkWaitSemaphores", code);
		let timeline = Timeline::new(&device).unwrap();
		assert_eq!(timeline.wait(0, Duration::ZERO), Err(Error::Failed(code)));
		drop(timeline);
		drop(device);
		assert_eq!(finish().released, ["semaphore 0xc00001"]);
	}
}

#[test]
fn timelines_keep_their_device_alive() {
	let device = device(&INTEL, |_| {});
	let first = Timeline::new(&device).unwrap();
	let second = Timeline::new(&device).unwrap();
	assert_ne!(first.handle(), second.handle());
	drop(device);
	drop(second);
	assert!(with(|driver| driver.destroyed_devices.is_empty()));
	drop(first);
	let driver = finish();
	assert_eq!(
		driver.released,
		["semaphore 0xc00002", "semaphore 0xc00001"]
	);
	assert_eq!(driver.destroyed_devices.len(), 1);
}

#[test]
fn a_device_missing_a_semaphore_function_is_not_made() {
	let names = [
		c"vkCreateSemaphore",
		c"vkDestroySemaphore",
		c"vkGetSemaphoreCounterValue",
		c"vkWaitSemaphores",
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
fn the_fake_driver_reports_broken_semaphore_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing.
	let device = device(&INTEL, |_| {});
	let handle = with(|driver| driver.live_devices[0]);
	let reported = || with(|driver| std::mem::take(&mut driver.invalid));
	let create_with = |kind: Option<&SemaphoreTypeCreateInfo>| {
		let info = SemaphoreCreateInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_CREATE_INFO,
			next: kind.map_or(ptr::null(), |kind| ptr::from_ref(kind).cast()),
			flags: 0,
		};
		let mut semaphore = 0;
		// SAFETY: the fake reads only what the pointers point to.
		unsafe { create(handle, &info, ptr::null(), &mut semaphore) };
		semaphore
	};
	let kind = |semaphore_type, next| SemaphoreTypeCreateInfo {
		s_type: STRUCTURE_TYPE_SEMAPHORE_TYPE_CREATE_INFO,
		next,
		semaphore_type,
		initial_value: 0,
	};
	// No type, a type with more chained after it, an unknown type, and a
	// binary semaphore, which is valid but no timeline.
	create_with(None);
	let after = kind(1, ptr::null());
	create_with(Some(&kind(1, ptr::from_ref(&after).cast())));
	create_with(Some(&kind(2, ptr::null())));
	let binary = create_with(Some(&kind(0, ptr::null())));
	let expected = [
		"vkCreateSemaphore: a type chained last",
		"vkCreateSemaphore: semaphoreType",
		"vkCreateSemaphore: a type chained last",
		"vkCreateSemaphore: semaphoreType",
	];
	assert_eq!(reported(), expected);
	let timeline = create_with(Some(&kind(1, ptr::null())));
	let waiting = |flags, semaphores: &[RawSemaphore]| {
		let values = vec![0; semaphores.len()];
		let info = SemaphoreWaitInfo {
			s_type: STRUCTURE_TYPE_SEMAPHORE_WAIT_INFO,
			next: ptr::null(),
			flags,
			semaphore_count: semaphores.len() as u32,
			semaphores: semaphores.as_ptr(),
			values: values.as_ptr(),
		};
		// SAFETY: as above.
		unsafe { wait(handle, &info, 0) }
	};
	let mut value = 0;
	// SAFETY: as above.
	unsafe { counter(handle, binary, &mut value) };
	assert_eq!(waiting(1, &[timeline]), 0);
	waiting(0, &[]);
	waiting(0, &[binary]);
	let expected = [
		format!("vkGetSemaphoreCounterValue: {binary:#x}, which is not a live timeline"),
		"vkWaitSemaphores: waits for every semaphore".into(),
		"vkWaitSemaphores: semaphoreCount".into(),
		format!("vkWaitSemaphores: {binary:#x}, which is not a live timeline"),
	];
	assert_eq!(reported(), expected);
	// Destroying the device while they live, then each twice.
	drop(device);
	assert_eq!(reported(), ["vkDestroyDevice while semaphores are live"]);
	let live = with(|driver| {
		driver.live_devices.push(handle);
		driver.live_semaphores.clone()
	});
	// SAFETY: the fake only records the handles.
	unsafe {
		for &semaphore in &live {
			destroy(handle, semaphore, ptr::null());
			destroy(handle, semaphore, ptr::null());
		}
	}
	assert_eq!(reported().len(), live.len());
	// A destroyed timeline is no longer one to wait for.
	waiting(0, &[timeline]);
	assert_eq!(reported().len(), 1);
	with(|driver| driver.live_devices.clear());
	finish();
}
