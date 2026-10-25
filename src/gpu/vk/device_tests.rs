//! Tests of creating a device against the fake driver of `driver_tests`:
//! the features and queue asked for, the functions looked up with it, and
//! the order devices and instances are destroyed in. The fake's device
//! functions live here and check the specification's valid usage rules.
use super::{
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind, TRANSFER},
	device::Device,
	driver_tests::{Driver, family, finish, install, with, with_devices},
	features::{Flag, NEEDED},
	types::*,
	version::api_version,
};
use std::{
	ffi::{CStr, c_char, c_void},
	mem::transmute,
	ptr,
	rc::Rc,
};

/// What a device was created with.
#[derive(Debug, PartialEq)]
pub(super) struct Made {
	pub physical: RawPhysicalDevice,
	pub family: u32,
	pub priorities: Vec<f32>,
	/// The features turned on: the structure's type and the flag's index.
	pub enabled: Vec<(i32, usize)>,
}

/// One structure of a `pNext` chain, as every Vulkan structure begins.
#[repr(C)]
struct Link {
	s_type: i32,
	next: *mut c_void,
}

/// The flags of the feature structure `link`, if it is one of the three
/// the bindings use, and how many there are.
///
/// # Safety
///
/// `link` is the start of a structure of the type it says.
unsafe fn flags(link: *mut Link) -> Option<*mut u32> {
	// SAFETY: every structure starts with its type.
	let s_type = unsafe { (*link).s_type };
	matches!(s_type, STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2 | 51 | 53)
		// SAFETY: the flags follow the type and the pointer.
		.then(|| unsafe { link.add(1) }.cast())
}

fn length(s_type: i32) -> usize {
	match s_type {
		STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2 => 55,
		51 => 47,
		_ => 15,
	}
}

/// Each feature structure in the chain from `head`, checking that each
/// is known, appears once and suits a device of version `api`.
///
/// # Safety
///
/// Every structure in the chain is valid.
unsafe fn chain(
	driver: &mut Driver,
	call: &str,
	head: *const c_void,
	api: u32,
) -> Vec<(i32, *mut u32)> {
	let mut found: Vec<(i32, *mut u32)> = Vec::new();
	let mut link = head as *mut Link;
	while !link.is_null() {
		// SAFETY: the caller vouches for the chain.
		let (s_type, next) = unsafe { ((*link).s_type, (*link).next) };
		// SAFETY: as above.
		match unsafe { flags(link) } {
			_ if found.iter().any(|&(seen, _)| seen == s_type) => {
				driver.invalid.push(format!("{call}: {s_type} twice"))
			}
			Some(_) if s_type == 53 && api < api_version(1, 3) => driver
				.invalid
				.push(format!("{call}: 1.3 features on {api:#x}")),
			Some(flags) => found.push((s_type, flags)),
			None => driver.invalid.push(format!("{call}: unknown {s_type}")),
		}
		link = next.cast();
	}
	found
}

pub(super) unsafe extern "system" fn features2(
	physical: RawPhysicalDevice,
	features: *mut PhysicalDeviceFeatures2,
) {
	with(|driver| {
		let Some(device) = driver.device(physical, "vkGetPhysicalDeviceFeatures2") else {
			return;
		};
		// SAFETY: the bindings pass a valid chain.
		let found = unsafe {
			chain(
				driver,
				"vkGetPhysicalDeviceFeatures2",
				features.cast(),
				device.api,
			)
		};
		if found.first().map(|&(s_type, _)| s_type) != Some(STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2) {
			driver
				.invalid
				.push("vkGetPhysicalDeviceFeatures2: sType".into());
		}
		for (s_type, flags) in found {
			for index in 0..length(s_type) {
				let lacked = match s_type {
					51 => driver.lacked.contains(&Flag::V12(index)),
					53 => driver.lacked.contains(&Flag::V13(index)),
					_ => false,
				};
				// SAFETY: within the structure's flags.
				unsafe { flags.add(index).write(u32::from(!lacked)) };
			}
		}
	})
}

pub(super) unsafe extern "system" fn create_device(
	physical: RawPhysicalDevice,
	info: *const DeviceCreateInfo,
	allocator: *const c_void,
	handle: *mut RawDevice,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		let Some(device) = driver.device(physical, "vkCreateDevice") else {
			return -3;
		};
		let one_queue = info.queue_create_info_count == 1 && !info.queue_create_infos.is_null();
		let rules = [
			(info.s_type == STRUCTURE_TYPE_DEVICE_CREATE_INFO, "sType"),
			(
				info.flags == 0 && allocator.is_null(),
				"flags and allocator",
			),
			(one_queue, "one queue create info"),
			(info.enabled_layer_count == 0, "no layers"),
			(info.enabled_extension_count == 0, "no extensions"),
			(
				info.enabled_features.is_null(),
				"pEnabledFeatures with a chain",
			),
		];
		let broken = rules.iter().filter(|(holds, _)| !holds);
		(driver.invalid).extend(broken.map(|(_, rule)| format!("vkCreateDevice: {rule}")));
		if !one_queue {
			return -3;
		}
		// SAFETY: checked non-null; it lives across the call.
		let queue = unsafe { &*info.queue_create_infos };
		let family = device.families.get(queue.queue_family_index as usize);
		let count = queue.queue_count;
		let priorities = if queue.queue_priorities.is_null() || count == 0 {
			Vec::new()
		} else {
			// SAFETY: one priority per queue.
			unsafe { std::slice::from_raw_parts(queue.queue_priorities, count as usize) }.to_vec()
		};
		let rules = [
			(
				queue.s_type == STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
				"sType",
			),
			(queue.next.is_null() && queue.flags == 0, "pNext and flags"),
			(family.is_some(), "a listed family"),
			(
				count > 0 && family.is_none_or(|family| count <= family.count),
				"queueCount",
			),
			(!priorities.is_empty(), "pQueuePriorities"),
			(
				priorities.iter().all(|p| (0.0..=1.0).contains(p)),
				"priorities",
			),
		];
		let broken = rules.iter().filter(|(holds, _)| !holds);
		(driver.invalid).extend(broken.map(|(_, rule)| format!("vkCreateDevice: queue {rule}")));
		// SAFETY: the bindings pass a valid chain.
		let found = unsafe { chain(driver, "vkCreateDevice", info.next, device.api) };
		let mut enabled = Vec::new();
		for (s_type, flags) in found {
			for index in 0..length(s_type) {
				// SAFETY: within the structure's flags.
				if unsafe { flags.add(index).read() } != 0 {
					enabled.push((s_type, index));
				}
			}
		}
		let lacked = enabled.iter().any(|&(s_type, index)| match s_type {
			51 => driver.lacked.contains(&Flag::V12(index)),
			53 => driver.lacked.contains(&Flag::V13(index)),
			_ => false,
		});
		if lacked {
			driver
				.invalid
				.push("vkCreateDevice: a feature the device lacks".into());
			// VK_ERROR_FEATURE_NOT_PRESENT.
			return -8;
		}
		if let Some(code) = driver.failing(c"vkCreateDevice") {
			return code;
		}
		driver.made.push(Made {
			physical,
			family: queue.queue_family_index,
			priorities,
			enabled,
		});
		let device = (0x5000 + 0x100 * driver.made.len()) as RawDevice;
		driver.live_devices.push(device);
		// SAFETY: the bindings pass a writable handle.
		unsafe { handle.write(device) };
		0
	})
}

pub(super) unsafe extern "system" fn get_device_proc_addr(
	device: RawDevice,
	name: *const c_char,
) -> Option<VoidFunction> {
	// SAFETY: the bindings pass C strings.
	let name = unsafe { CStr::from_ptr(name) };
	with(|driver| {
		driver.lookups.push((device, name.to_string_lossy().into()));
		driver.check_live_device(device, "vkGetDeviceProcAddr");
		if driver.lacks.contains(&name) {
			return None;
		}
		// SAFETY: the bindings cast each back to its own type.
		unsafe {
			Some(match name.to_bytes() {
				b"vkDestroyDevice" => transmute::<DestroyDevice, VoidFunction>(destroy_device),
				b"vkGetDeviceQueue" => transmute::<GetDeviceQueue, VoidFunction>(get_device_queue),
				_ => return None,
			})
		}
	})
}

unsafe extern "system" fn destroy_device(device: RawDevice, allocator: *const c_void) {
	with(|driver| {
		driver.check_live_device(device, "vkDestroyDevice");
		if !allocator.is_null() {
			driver.invalid.push("vkDestroyDevice: an allocator".into());
		}
		driver.live_devices.retain(|&live| live != device);
		driver.destroyed_devices.push(device);
	})
}

unsafe extern "system" fn get_device_queue(
	device: RawDevice,
	family: u32,
	index: u32,
	queue: *mut RawQueue,
) {
	with(|driver| {
		driver.check_live_device(device, "vkGetDeviceQueue");
		let made = (driver.live_devices.iter().position(|&live| live == device))
			.and_then(|_| driver.made.get((device as usize - 0x5000) / 0x100 - 1));
		if made.is_none_or(|made| made.family != family || index as usize >= made.priorities.len()) {
			driver.invalid.push(format!(
				"vkGetDeviceQueue: queue {index} of family {family}, never asked for"
			));
		}
		// SAFETY: the bindings pass a writable handle.
		unsafe { queue.write((device as usize + 1 + family as usize) as RawQueue) };
	})
}

/// A 1.3 device that draws on its second family, as the only device.
fn drawing(api: u32) -> Described {
	Described {
		name: "fake".into(),
		kind: Kind::Integrated,
		api,
		families: vec![family(TRANSFER, 1), family(GRAPHICS | TRANSFER, 2)],
		swapchain: true,
	}
}

/// An instance on `driver`, and its first device.
fn open(driver: Driver) -> (Rc<super::instance::Instance>, RawPhysicalDevice) {
	let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
	let physical = instance.physical_devices().unwrap()[0];
	(instance, physical)
}

#[test]
fn a_device_turns_on_exactly_the_needed_features_with_one_queue() {
	let (instance, physical) = open(with_devices(&[drawing(api_version(1, 3))]));
	let device = Device::new(instance.clone(), physical, 1).expect("device");
	assert_eq!(device.family(), 1);
	let handle = with(|driver| driver.live_devices.clone());
	assert_eq!(handle.len(), 1);
	assert_eq!(device.queue(), (handle[0] as usize + 2) as RawQueue);
	drop(device);
	drop(instance);
	let driver = finish();
	let made = Made {
		physical,
		family: 1,
		priorities: vec![1.0],
		enabled: vec![(51, 37), (53, 9), (53, 12)],
	};
	assert_eq!(driver.made, [made]);
	assert_eq!(driver.destroyed_devices, handle);
	assert_eq!(driver.destroyed.len(), 1);
}

#[test]
fn a_1_4_device_is_given_the_same_features() {
	let (instance, physical) = open(with_devices(&[drawing(api_version(1, 4) | 309)]));
	let device = Device::new(instance, physical, 1).expect("device");
	drop(device);
	let driver = finish();
	assert_eq!(driver.made[0].enabled, [(51, 37), (53, 9), (53, 12)]);
}

#[test]
fn a_device_lacking_a_needed_feature_is_refused_by_name() {
	for (flag, name) in NEEDED {
		let mut driver = with_devices(&[drawing(api_version(1, 3))]);
		driver.lacked = vec![flag];
		let (instance, physical) = open(driver);
		let error = Device::new(instance, physical, 1).err();
		assert_eq!(error, Some(Error::Lacks(name)));
		assert_eq!(
			error.unwrap().to_string(),
			format!("the Vulkan device lacks {name}")
		);
		assert!(finish().made.is_empty(), "{name}");
	}
	// Lacking what the renderer does not use is fine.
	let mut driver = with_devices(&[drawing(api_version(1, 3))]);
	driver.lacked = vec![Flag::V12(38), Flag::V13(0), Flag::V13(14), Flag::V12(0)];
	let (instance, physical) = open(driver);
	assert!(Device::new(instance, physical, 1).is_ok());
	assert_eq!(finish().made.len(), 1);
}

#[test]
fn a_device_older_than_1_3_is_too_old_and_never_asked_about_1_3_features() {
	for api in [api_version(1, 2) | 195, api_version(1, 0)] {
		let (instance, physical) = open(with_devices(&[drawing(api)]));
		assert_eq!(
			Device::new(instance, physical, 1).err(),
			Some(Error::TooOld(api))
		);
		// The fake reports any 1.3 structure it is shown.
		assert!(finish().made.is_empty());
	}
}

#[test]
fn failing_device_creation_is_an_error_with_its_code() {
	// VK_ERROR_OUT_OF_HOST_MEMORY, _DEVICE_MEMORY, _INITIALIZATION_FAILED,
	// _TOO_MANY_OBJECTS and _DEVICE_LOST.
	for code in [-1, -2, -3, -10, -4] {
		let mut driver = with_devices(&[drawing(api_version(1, 3))]);
		driver.fails.push((c"vkCreateDevice", code));
		let (instance, physical) = open(driver);
		assert_eq!(
			Device::new(instance, physical, 1).err(),
			Some(Error::Failed(code))
		);
		let driver = finish();
		assert!(driver.live_devices.is_empty() && driver.destroyed_devices.is_empty());
	}
}

#[test]
fn device_functions_are_looked_up_with_the_new_device() {
	let (instance, physical) = open(with_devices(&[drawing(api_version(1, 3))]));
	let before = with(|driver| driver.lookups.len());
	let device = Device::new(instance, physical, 1).unwrap();
	let (lookups, live) = with(|driver| (driver.lookups[before..].to_vec(), driver.live_devices[0]));
	let expected = [(live, "vkDestroyDevice"), (live, "vkGetDeviceQueue")];
	let lookups: Vec<_> = lookups
		.iter()
		.map(|(at, name)| (*at, name.as_str()))
		.collect();
	assert_eq!(lookups, expected);
	drop(device);
	finish();
}

#[test]
fn a_device_missing_a_function_is_destroyed_again_unless_it_cannot_be() {
	let mut driver = with_devices(&[drawing(api_version(1, 3))]);
	driver.lacks.push(c"vkGetDeviceQueue");
	let (instance, physical) = open(driver);
	let error = Device::new(instance, physical, 1).err();
	assert_eq!(error, Some(Error::Missing(c"vkGetDeviceQueue")));
	let driver = finish();
	assert_eq!(
		(driver.live_devices.len(), driver.destroyed_devices.len()),
		(0, 1)
	);
	assert_eq!(driver.destroyed.len(), 1);
	// Without vkDestroyDevice the device leaks, and the error says why.
	let mut driver = with_devices(&[drawing(api_version(1, 3))]);
	driver.lacks.push(c"vkDestroyDevice");
	let (instance, physical) = open(driver);
	let error = Device::new(instance.clone(), physical, 1).err();
	assert_eq!(error, Some(Error::Missing(c"vkDestroyDevice")));
	let leaked = with(|driver| std::mem::take(&mut driver.live_devices));
	assert_eq!(leaked.len(), 1);
	drop(instance);
	assert!(finish().destroyed_devices.is_empty());
}

#[test]
fn the_instance_lives_until_its_last_device_is_destroyed() {
	let devices = [drawing(api_version(1, 3)), drawing(api_version(1, 4))];
	let instance = Rc::new(
		install(with_devices(&devices))
			.create_instance(c"poopterm")
			.unwrap(),
	);
	let physical = instance.physical_devices().unwrap();
	let first = Device::new(instance.clone(), physical[0], 1).unwrap();
	let second = Device::new(instance.clone(), physical[1], 0).unwrap();
	assert_eq!((first.family(), second.family()), (1, 0));
	assert_ne!(first.queue(), second.queue());
	drop(instance);
	assert!(with(|driver| driver.destroyed.is_empty()));
	drop(first);
	assert!(with(|driver| driver.destroyed.is_empty()));
	let live = with(|driver| driver.live_devices.clone());
	drop(second);
	let driver = finish();
	assert_eq!(driver.destroyed_devices.last(), live.last());
	assert_eq!(
		(driver.destroyed.len(), driver.destroyed_devices.len()),
		(1, 2)
	);
	let physicals: Vec<_> = driver.made.iter().map(|made| made.physical).collect();
	assert_eq!(physicals, physical);
}

/// A create info for one queue of `queue`, chaining `next`.
fn device_info(queue: &DeviceQueueCreateInfo, next: *const c_void) -> DeviceCreateInfo {
	DeviceCreateInfo {
		s_type: STRUCTURE_TYPE_DEVICE_CREATE_INFO,
		next,
		flags: 0,
		queue_create_info_count: 1,
		queue_create_infos: queue,
		enabled_layer_count: 0,
		enabled_layer_names: ptr::null(),
		enabled_extension_count: 0,
		enabled_extension_names: ptr::null(),
		enabled_features: ptr::null(),
	}
}

/// What the fake reported, emptied.
fn reported() -> Vec<String> {
	with(|driver| std::mem::take(&mut driver.invalid))
}

#[test]
fn the_fake_driver_reports_broken_device_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing.
	let mut driver = with_devices(&[drawing(api_version(1, 3)), drawing(api_version(1, 2))]);
	driver.lacked = vec![Flag::V13(12)];
	let (instance, _) = open(driver);
	let physical = instance.physical_devices().unwrap();
	// Room for the three queues one case asks for.
	let priority = [1.0f32; 3];
	let good = DeviceQueueCreateInfo {
		s_type: STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		queue_family_index: 1,
		queue_count: 1,
		queue_priorities: priority.as_ptr(),
	};
	let create = |info: &DeviceCreateInfo, at: RawPhysicalDevice| {
		let mut handle = ptr::null_mut();
		// SAFETY: the fake reads only what the create info points to.
		let code = unsafe { create_device(at, info, ptr::null(), &mut handle) };
		(code, reported())
	};
	let one = |rule: &str| vec![format!("vkCreateDevice: {rule}")];
	assert_eq!(
		create(&device_info(&good, ptr::null()), physical[0]),
		(0, vec![])
	);
	let wrong = DeviceCreateInfo {
		s_type: 2,
		..device_info(&good, ptr::null())
	};
	assert_eq!(create(&wrong, physical[0]).1, one("sType"));
	let features = [0u32; 55];
	let both = DeviceCreateInfo {
		enabled_features: features.as_ptr().cast(),
		..device_info(&good, ptr::null())
	};
	assert_eq!(
		create(&both, physical[0]).1,
		one("pEnabledFeatures with a chain")
	);
	let layers = DeviceCreateInfo {
		enabled_layer_count: 1,
		..device_info(&good, ptr::null())
	};
	assert_eq!(create(&layers, physical[0]).1, one("no layers"));
	let queues = [
		(
			DeviceQueueCreateInfo {
				queue_family_index: 2,
				..good
			},
			"a listed family",
		),
		(
			DeviceQueueCreateInfo {
				queue_count: 3,
				..good
			},
			"queueCount",
		),
		(
			DeviceQueueCreateInfo {
				queue_priorities: ptr::null(),
				..good
			},
			"pQueuePriorities",
		),
		(DeviceQueueCreateInfo { s_type: 3, ..good }, "sType"),
	];
	for (queue, rule) in queues {
		assert_eq!(
			create(&device_info(&queue, ptr::null()), physical[0]).1,
			one(&format!("queue {rule}"))
		);
	}
	let loud = [1.5f32];
	let loud = DeviceQueueCreateInfo {
		queue_priorities: loud.as_ptr(),
		..good
	};
	assert_eq!(
		create(&device_info(&loud, ptr::null()), physical[0]).1,
		one("queue priorities")
	);
	// A feature the device lacks fails, and a 1.3 structure on a 1.2 device
	// or an unknown structure is invalid.
	let mut enabled = super::features::Features::none();
	enabled.v13.flags[12] = 1;
	let head = (&enabled.head as *const PhysicalDeviceFeatures2).cast();
	let (code, invalid) = create(&device_info(&good, head), physical[0]);
	assert_eq!((code, invalid), (-8, one("a feature the device lacks")));
	enabled.v13.flags[12] = 0;
	let queue = DeviceQueueCreateInfo {
		queue_family_index: 0,
		..good
	};
	assert_eq!(
		create(&device_info(&queue, head), physical[1]).1,
		one("1.3 features on 0x402000")
	);
	enabled.v12.s_type = 7;
	assert_eq!(
		create(&device_info(&good, head), physical[0]).1,
		one("unknown 7")
	);
	// Destroying a device twice, and a queue never asked for.
	let live = with(|driver| driver.live_devices.clone());
	let mut queue = ptr::null_mut();
	// SAFETY: the fake only records the handles.
	unsafe {
		get_device_queue(live[0], 0, 0, &mut queue);
		get_device_queue(live[0], 1, 1, &mut queue);
	}
	assert_eq!(reported().len(), 2);
	for &device in &live {
		// SAFETY: as above.
		unsafe { destroy_device(device, ptr::null()) };
	}
	assert_eq!(reported(), Vec::<String>::new());
	// SAFETY: as above.
	unsafe { destroy_device(live[0], ptr::null()) };
	assert_eq!(
		reported(),
		[format!(
			"vkDestroyDevice on {:?}, which is not live",
			live[0]
		)]
	);
	// Destroying the instance while a device lives.
	let mut handle = ptr::null_mut();
	// SAFETY: as above.
	unsafe {
		create_device(
			physical[0],
			&device_info(&good, ptr::null()),
			ptr::null(),
			&mut handle,
		)
	};
	drop(instance);
	assert_eq!(reported(), ["vkDestroyInstance while a device is live"]);
	// SAFETY: as above.
	unsafe { destroy_device(handle, ptr::null()) };
	finish();
}
