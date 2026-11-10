//! The fake driver's functions for showing images, which `driver_tests`
//! hands out: acquiring and presenting swapchain images and waiting for a
//! queue, each checking the specification's valid usage rules, with tests
//! that it reports each broken rule.
use super::{
	driver_tests::{Driver, with},
	present_regions::{PresentRegions, STRUCTURE_TYPE_PRESENT_REGIONS},
	present_tests::{showing, window},
	recording::Layout,
	recording_tests::PRESENT,
	semaphore::Semaphore,
	surface_query_tests::mesa_wayland,
	swapchain_types::*,
	types::*,
};
use std::{ffi::CStr, mem::transmute, ptr, rc::Rc};

/// `VK_NOT_READY`, `VK_TIMEOUT`, `VK_SUBOPTIMAL_KHR`,
/// `VK_ERROR_OUT_OF_DATE_KHR` and `VK_ERROR_SURFACE_LOST_KHR`.
pub(super) const NOT_READY: i32 = 1;
pub(super) const TIMEOUT: i32 = 2;
pub(super) const SUBOPTIMAL: i32 = 1_000_001_003;
pub(super) const OUT_OF_DATE: i32 = -1_000_001_004;
pub(super) const SURFACE_LOST: i32 = -1_000_000_000;

/// The fake's function `name` for showing images, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkAcquireNextImageKHR" => transmute::<AcquireNextImage, VoidFunction>(acquire),
			b"vkQueuePresentKHR" => transmute::<QueuePresent, VoidFunction>(present_image),
			b"vkQueueWaitIdle" => transmute::<QueueWaitIdle, VoidFunction>(wait_idle),
			_ => return None,
		})
	}
}

impl Driver {
	/// Whether `queue` is a live device's.
	fn known_queue(&self, queue: RawQueue) -> bool {
		let queues = self.live_devices.iter().zip(&self.made);
		(queues.into_iter())
			.any(|(&device, made)| queue as usize == device as usize + 1 + made.family as usize)
	}

	/// Sets the binary semaphore `semaphore` signalled or not.
	fn set_binary(&mut self, semaphore: RawSemaphore, signalled: bool) {
		if let Some(counter) = self
			.counters
			.get_mut(semaphore.wrapping_sub(0xc0_0001) as usize)
		{
			*counter = signalled.into();
		}
	}
}

unsafe extern "system" fn acquire(
	device: RawDevice,
	swapchain: RawSwapchain,
	timeout: u64,
	semaphore: RawSemaphore,
	fence: u64,
	index: *mut u32,
) -> i32 {
	with(|driver| {
		const CALL: &str = "vkAcquireNextImageKHR";
		driver.acquire_timeouts.push(timeout);
		driver.check_live_device(device, CALL);
		let live = driver.swapchain(swapchain).cloned();
		let min = driver
			.surface_capabilities
			.clone()
			.unwrap_or_else(mesa_wayland)
			.min_images;
		let held = live.as_ref().map_or(0, |live| live.acquired.len());
		let most = live
			.as_ref()
			.map_or(0, |live| live.images.len() + 1 - min as usize);
		let rules = [
			(
				live
					.as_ref()
					.is_some_and(|live| !live.retired && live.device == device),
				"a live swapchain, not retired",
			),
			(
				driver.binary(semaphore) == Some(false) && fence == 0,
				"an unsignalled binary semaphore, no fence",
			),
			(
				timeout != u64::MAX || held < most,
				"no more images held than the swapchain can spare",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("{CALL}: {rule}"));
		}
		let Some(live) = live else { return -3 };
		let answer = match driver.acquire_answers.first() {
			Some(_) => driver.acquire_answers.remove(0),
			None => 0,
		};
		if answer != 0 && answer != SUBOPTIMAL {
			return answer;
		}
		let free = (0..live.images.len() as u32).find(|image| !live.acquired.contains(image));
		let Some(free) = free.filter(|_| held < most) else {
			return if timeout == 0 { NOT_READY } else { TIMEOUT };
		};
		driver.set_binary(semaphore, true);
		let made = driver
			.live_swapchains
			.iter_mut()
			.find(|made| made.handle == swapchain);
		made.unwrap().acquired.push(free);
		(driver.semaphore_ops).push(format!("acquire {free} signals {semaphore:#x}"));
		// SAFETY: the bindings pass a writable index.
		unsafe { index.write(free) };
		answer
	})
}

unsafe extern "system" fn present_image(queue: RawQueue, info: *const PresentInfo) -> i32 {
	// SAFETY: the bindings pass a present info whose arrays live across the
	// call, each as long as its count.
	let (info, waits, swapchains, indices) = unsafe {
		let info = &*info;
		let read = |count| count as usize;
		(
			info,
			std::slice::from_raw_parts(info.wait_semaphores, read(info.wait_semaphore_count)),
			std::slice::from_raw_parts(info.swapchains, read(info.swapchain_count)),
			std::slice::from_raw_parts(info.image_indices, read(info.swapchain_count)),
		)
	};
	// SAFETY: every chained structure starts with its sType; one of the
	// regions' type is the regions the bindings name, whose arrays live
	// across the call.
	let regions = (!info.next.is_null()).then(|| unsafe {
		let s_type = *info.next.cast::<i32>();
		if s_type != STRUCTURE_TYPE_PRESENT_REGIONS {
			return (s_type, None);
		}
		let regions = &*info.next.cast::<PresentRegions>();
		let region = (regions.swapchain_count == 1).then(|| &*regions.regions);
		let rects = region.map(|region| {
			let count = region.rectangle_count as usize;
			std::slice::from_raw_parts(region.rectangles, count).to_vec()
		});
		(s_type, rects)
	});
	with(|driver| {
		const CALL: &str = "vkQueuePresentKHR";
		let enabled = (driver.made.last()).is_some_and(|made| {
			made
				.extensions
				.iter()
				.any(|name| name == "VK_KHR_incremental_present")
		});
		let chain_holds = match &regions {
			None => true,
			Some((s_type, rects)) => {
				*s_type == STRUCTURE_TYPE_PRESENT_REGIONS && rects.is_some() && enabled
			}
		};
		let rects = regions.and_then(|(_, rects)| rects).unwrap_or_default();
		let extent = driver.swapchains.last().map_or([0; 2], |made| made.extent);
		let inside = rects.iter().all(|rect| {
			let [x, y] = rect.offset.map(|n| n.max(0) as u32);
			rect.offset.iter().all(|&n| n >= 0)
				&& x + rect.extent[0] <= extent[0]
				&& y + rect.extent[1] <= extent[1]
				&& rect.extent.iter().all(|&n| n > 0)
				&& rect.layer == 0
		});
		driver.require(chain_holds, || {
			format!("{CALL}: present regions on an enabled extension")
		});
		driver.require(inside, || format!("{CALL}: regions inside the image"));
		driver.present_regions.push(rects);
		let (&[swapchain], &[index]) = (swapchains, indices) else {
			driver.invalid.push(format!("{CALL}: one swapchain"));
			return -3;
		};
		let live = driver.swapchain(swapchain).cloned();
		let image = live
			.as_ref()
			.and_then(|live| live.images.get(index as usize).copied());
		let committed = (driver.image_layouts.iter().rev())
			.find(|&&(made, _)| Some(made) == image)
			.map(|&(_, layout)| layout);
		let rules = [
			(driver.known_queue(queue), "a live device's queue"),
			(info.s_type == STRUCTURE_TYPE_PRESENT_INFO, "sType"),
			(info.next.is_null() || chain_holds, "pNext"),
			(
				waits.iter().all(|&wait| driver.binary(wait) == Some(true)),
				"waits for signalled binary semaphores",
			),
			(
				live
					.as_ref()
					.is_some_and(|live| live.acquired.contains(&index)),
				"an image acquired",
			),
			(committed == Some(PRESENT), "an image in the present layout"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("{CALL}: {rule}"));
		}
		for &wait in waits {
			driver.set_binary(wait, false);
		}
		if let Some(made) = driver
			.live_swapchains
			.iter_mut()
			.find(|made| made.handle == swapchain)
		{
			made.acquired.retain(|&acquired| acquired != index);
		}
		driver.presenting = true;
		driver.presents.push((swapchain, index));
		let waits: Vec<_> = waits.iter().map(|wait| format!("{wait:#x}")).collect();
		(driver.semaphore_ops).push(format!("present {index} waits {waits:?}"));
		match driver.present_answers.first() {
			Some(_) => driver.present_answers.remove(0),
			None => 0,
		}
	})
}

unsafe extern "system" fn wait_idle(queue: RawQueue) -> i32 {
	with(|driver| {
		let known = driver.known_queue(queue);
		driver.require(known, || "vkQueueWaitIdle: a live device's queue".into());
		// Everything submitted is done.
		let done: Vec<_> = (driver.command_buffers.iter())
			.filter_map(|buffer| match buffer.stage {
				super::commands_tests::Stage::Submitted(semaphore, value) => Some((semaphore, value)),
				_ => None,
			})
			.collect();
		for (semaphore, value) in done {
			let counter = &mut driver.counters[semaphore.wrapping_sub(0xc0_0001) as usize];
			*counter = value.max(*counter);
		}
		driver.presenting = false;
		driver.idle_waits += 1;
		0
	})
}

/// What the fake found broken since last asked, emptied.
fn broken() -> Vec<String> {
	with(|driver| std::mem::take(&mut driver.invalid))
}

#[test]
fn the_fake_driver_reports_images_acquired_against_the_rules() {
	// Were it blind to them, the present tests would prove nothing.
	let mut driver = showing();
	// One image of the four to spare.
	driver.surface_capabilities = Some(super::swapchain_plan::Capabilities {
		min_images: 4,
		..mesa_wayland()
	});
	let window = window(driver);
	let (device, swapchain) = (window.device.handle, window.swapchain.chain.handle);
	let acquire = window.swapchain.functions.acquire;
	let call = |swapchain, timeout, semaphore, fence| {
		let mut index = 9;
		// SAFETY: the fake only checks and records the handles.
		let code = unsafe { acquire(device, swapchain, timeout, semaphore, fence, &mut index) };
		(code, index, broken())
	};
	let made = || Semaphore::new(&window.device).unwrap();
	let (first, second) = (made(), made());
	let rule = |rule: &str| vec![format!("vkAcquireNextImageKHR: {rule}")];
	assert_eq!(call(swapchain, u64::MAX, first.handle(), 0), (0, 0, vec![]));
	let signalled = rule("an unsignalled binary semaphore, no fence");
	assert_eq!(
		call(swapchain, 0, first.handle(), 0),
		(NOT_READY, 9, signalled.clone())
	);
	assert_eq!(
		call(swapchain, 0, second.handle(), 7),
		(NOT_READY, 9, signalled.clone())
	);
	let timeline = super::timeline::Timeline::new(&window.device).unwrap();
	assert_eq!(call(swapchain, 0, timeline.handle(), 0).2, signalled);
	// Waiting forever for an image that cannot come free is broken; asking
	// whether one is free is not.
	let held = rule("no more images held than the swapchain can spare");
	assert_eq!(
		call(swapchain, u64::MAX, second.handle(), 0),
		(TIMEOUT, 9, held)
	);
	assert_eq!(
		call(swapchain, 0, second.handle(), 0),
		(NOT_READY, 9, vec![])
	);
	let unknown = rule("a live swapchain, not retired");
	assert_eq!(call(0x6_0099, 0, second.handle(), 0), (-3, 9, unknown));
	drop((first, second, timeline));
	window.close();
}

#[test]
fn the_fake_driver_reports_images_presented_against_the_rules() {
	let mut window = window(showing());
	window.frame().unwrap();
	let (device, swapchain) = (window.device.handle, window.swapchain.chain.handle);
	let (acquire, present) = (
		window.swapchain.functions.acquire,
		window.swapchain.functions.present,
	);
	let queue = window.device.queue();
	let waited = Semaphore::new(&window.device).unwrap();
	let waits = [waited.handle()];
	// Image 0 acquired again, still in the present layout from the frame.
	let acquired = || {
		let mut index = 9;
		// SAFETY: the fake only checks and records the handles.
		unsafe { acquire(device, swapchain, 0, waited.handle(), 0, &mut index) };
		index
	};
	let good = |index: &u32, swapchains: &[RawSwapchain; 1]| PresentInfo {
		s_type: STRUCTURE_TYPE_PRESENT_INFO,
		next: ptr::null(),
		wait_semaphore_count: 1,
		wait_semaphores: waits.as_ptr(),
		swapchain_count: 1,
		swapchains: swapchains.as_ptr(),
		image_indices: index,
		results: ptr::null_mut(),
	};
	let call = |queue, info: &PresentInfo| {
		// SAFETY: as above; `info` and what it points to outlive the call.
		let code = unsafe { present(queue, info) };
		(code, broken())
	};
	let rule = |rule: &str| vec![format!("vkQueuePresentKHR: {rule}")];
	let (zero, swapchains) = (0, [swapchain]);
	assert_eq!(acquired(), 0);
	assert_eq!(call(queue, &good(&zero, &swapchains)), (0, vec![]));
	let unsignalled = Semaphore::new(&window.device).unwrap();
	let unsignalled_waits = [unsignalled.handle()];
	let cases: [(_, PresentInfo, &[&str]); 5] = [
		(
			0x99 as RawQueue,
			good(&zero, &swapchains),
			&["a live device's queue"],
		),
		(
			queue,
			PresentInfo {
				s_type: 3,
				..good(&zero, &swapchains)
			},
			&["sType"],
		),
		(
			queue,
			PresentInfo {
				next: &zero as *const u32 as _,
				..good(&zero, &swapchains)
			},
			&["present regions on an enabled extension", "pNext"],
		),
		(
			queue,
			PresentInfo {
				wait_semaphores: unsignalled_waits.as_ptr(),
				..good(&zero, &swapchains)
			},
			&["waits for signalled binary semaphores"],
		),
		(
			queue,
			PresentInfo {
				image_indices: &1,
				..good(&zero, &swapchains)
			},
			// Image 1 has never been drawn, so is in no layout either.
			&["an image acquired", "an image in the present layout"],
		),
	];
	for (queue, info, broken) in cases {
		// Presenting released image 0 and unsignalled what it waited for,
		// unless it waited for another semaphore.
		if with(|driver| driver.binary(waited.handle())) == Some(false) {
			assert_eq!(acquired(), 0);
		}
		let expected: Vec<_> = broken.iter().flat_map(|broken| rule(broken)).collect();
		assert_eq!(call(queue, &info), (0, expected), "{broken:?}");
	}
	let two = PresentInfo {
		swapchain_count: 2,
		..good(&zero, &swapchains)
	};
	assert_eq!(call(queue, &two), (-3, rule("one swapchain")));
	// A queue no device made is not waited for.
	// SAFETY: the fake's own function, cast back to its type.
	let wait: QueueWaitIdle = unsafe { transmute(function(c"vkQueueWaitIdle").unwrap()) };
	// SAFETY: the fake only checks the handle.
	assert_eq!(unsafe { wait(0x99 as RawQueue) }, 0);
	assert_eq!(broken(), ["vkQueueWaitIdle: a live device's queue"]);
	drop((waited, unsignalled));
	window.close();
}

#[test]
fn the_fake_driver_reports_presenting_an_image_no_swapchain_made() {
	let mut driver = showing();
	let memory = super::buffer_tests::INTEL.map(|flags| super::memory::MemoryType { flags, heap: 0 });
	driver.memory = memory.to_vec();
	let mut window = window(driver);
	let usage = super::swapchain_plan::USAGE;
	let image =
		super::image::Image::new(&window.device, super::image::Format::Bgra, [4, 4, 1], usage);
	let image = Rc::new(image.unwrap());
	let mut recording = window.commands.record().unwrap();
	recording.transition(&image, Layout::Present);
	recording.submit().unwrap();
	let rule = "vkCmdPipelineBarrier2: image presenting a swapchain's image";
	assert_eq!(broken(), [rule]);
	window.commands.record().unwrap().submit().unwrap();
	drop(image);
	window.close();
}

#[test]
fn the_fake_driver_finishes_everything_submitted_when_the_queue_goes_idle() {
	let mut window = window(showing());
	window.frame().unwrap();
	let timeline = window.commands.timeline.clone();
	assert_eq!(
		timeline.reached(),
		Ok(0),
		"the fake finishes nothing by itself"
	);
	assert!(with(|driver| driver.presenting));
	// SAFETY: as above.
	let wait: QueueWaitIdle = unsafe { transmute(function(c"vkQueueWaitIdle").unwrap()) };
	// SAFETY: the queue is the device's.
	assert_eq!(unsafe { wait(window.device.queue()) }, 0);
	assert_eq!(timeline.reached(), Ok(1));
	assert!(!with(|driver| driver.presenting));
	drop(timeline);
	window.close();
}

#[test]
fn the_fake_driver_checks_present_regions_against_the_rules() {
	// Were it blind to them, the region tests would prove nothing.
	use super::present_regions::{PresentRegion, RectLayer};
	let mut window = window(showing());
	window.frame().unwrap();
	let (device, swapchain) = (window.device.handle, window.swapchain.chain.handle);
	let functions = &window.swapchain.functions;
	let (acquire, present) = (functions.acquire, functions.present);
	let queue = window.device.queue();
	let waited = Semaphore::new(&window.device).unwrap();
	let (waits, swapchains, zero) = ([waited.handle()], [swapchain], 0u32);
	let extent = with(|driver| driver.swapchains.last().unwrap().extent);
	// Presents image 0, acquired again, naming `rects` for `count` swapchains.
	let call = |rects: &[RectLayer], count: u32, s_type: i32| {
		let mut index = 9;
		// SAFETY: the fake only checks and records the handles.
		unsafe { acquire(device, swapchain, 0, waited.handle(), 0, &mut index) };
		let region = PresentRegion {
			rectangle_count: rects.len() as u32,
			rectangles: rects.as_ptr(),
		};
		let regions = PresentRegions {
			s_type,
			next: ptr::null(),
			swapchain_count: count,
			regions: &region,
		};
		let info = PresentInfo {
			s_type: STRUCTURE_TYPE_PRESENT_INFO,
			next: (&raw const regions).cast(),
			wait_semaphore_count: 1,
			wait_semaphores: waits.as_ptr(),
			swapchain_count: 1,
			swapchains: swapchains.as_ptr(),
			image_indices: &zero,
			results: ptr::null_mut(),
		};
		// SAFETY: as above; `info` and what it chains outlive the call.
		unsafe { present(queue, &info) };
		broken()
	};
	let rect = |offset: [i32; 2], extent: [u32; 2], layer| RectLayer {
		offset,
		extent,
		layer,
	};
	let rule = |rule: &str| format!("vkQueuePresentKHR: {rule}");
	let good = [rect([0, 0], [extent[0], 4], 0)];
	// A device made without the extension takes no regions.
	let off = [
		rule("present regions on an enabled extension"),
		rule("pNext"),
	];
	assert_eq!(call(&good, 1, STRUCTURE_TYPE_PRESENT_REGIONS), off);
	with(|driver| {
		let made = driver.made.last_mut().unwrap();
		made.extensions.push("VK_KHR_incremental_present".into());
	});
	let none = Vec::<String>::new();
	assert_eq!(call(&good, 1, STRUCTURE_TYPE_PRESENT_REGIONS), none);
	assert_eq!(call(&good, 2, STRUCTURE_TYPE_PRESENT_REGIONS), off);
	assert_eq!(call(&good, 1, 7), off);
	let outside = [rule("regions inside the image")];
	for bad in [
		rect([0, 0], [extent[0] + 1, 4], 0),
		rect([0, extent[1] as i32 - 2], [4, 4], 0),
		rect([-1, 0], [4, 4], 0),
		rect([0, 0], [0, 4], 0),
		rect([0, 0], [4, 4], 1),
	] {
		let got = call(&[bad], 1, STRUCTURE_TYPE_PRESENT_REGIONS);
		assert_eq!(got, outside, "{bad:?}");
	}
	// Each present's regions are kept in order, empty when none were named.
	let named = with(|driver| driver.present_regions.clone());
	assert_eq!(named.len(), 1 + 9);
	assert_eq!(named[2], good);
	drop(waited);
	window.close();
}
