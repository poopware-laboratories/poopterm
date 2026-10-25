//! Tests of the bindings against a fake driver: what they pass to Vulkan,
//! which instance they look each function up with, and how they fail when
//! the loader is old, lacks a function or a call fails. The driver checks
//! the specification's valid usage rules as it is called.
use super::{
	call::{Error, INCOMPLETE},
	entry::Entry,
	types::*,
	version::api_version,
};
use std::{
	cell::RefCell,
	ffi::{CStr, c_char, c_void},
	mem::transmute,
	ptr,
};

/// What the fake driver offers, and what it saw.
#[derive(Default)]
struct Driver {
	/// The loader's version; `None` for a 1.0 loader, which lacks
	/// vkEnumerateInstanceVersion.
	version: Option<u32>,
	lacks: Vec<&'static CStr>,
	/// Calls that fail, with what they return.
	fails: Vec<(&'static CStr, i32)>,
	devices: Vec<RawPhysicalDevice>,
	/// The devices there are once the first count was given, if more.
	grown: Vec<RawPhysicalDevice>,
	device_calls: u32,
	lookups: Vec<(RawInstance, String)>,
	created: Vec<Created>,
	live: Vec<RawInstance>,
	destroyed: Vec<RawInstance>,
	/// Valid usage rules the bindings broke.
	invalid: Vec<String>,
}

/// What an instance was created with.
#[derive(Debug, PartialEq)]
struct Created {
	application: String,
	engine: String,
	api: u32,
	layers: u32,
	extensions: u32,
}

impl Driver {
	fn offering(version: u32) -> Self {
		Self {
			version: Some(version),
			..Self::default()
		}
	}

	fn failing(&self, name: &CStr) -> Option<i32> {
		self
			.fails
			.iter()
			.find(|(failing, _)| *failing == name)
			.map(|&(_, code)| code)
	}

	/// Breaks the test unless `instance` is one the driver made and has not
	/// destroyed.
	fn check_live(&mut self, instance: RawInstance, call: &str) {
		if !self.live.contains(&instance) {
			self
				.invalid
				.push(format!("{call} on {instance:?}, which is not live"));
		}
	}
}

thread_local! {
	static DRIVER: RefCell<Driver> = RefCell::default();
}

fn with<T>(f: impl FnOnce(&mut Driver) -> T) -> T {
	DRIVER.with(|driver| f(&mut driver.borrow_mut()))
}

/// An entry whose loader is `driver`, for this thread.
fn install(driver: Driver) -> Entry {
	with(|installed| *installed = driver);
	Entry {
		get_instance_proc_addr,
		_library: None,
	}
}

/// What the driver saw, checking no valid usage rule was broken.
fn finish() -> Driver {
	let driver = with(std::mem::take);
	assert_eq!(driver.invalid, Vec::<String>::new());
	driver
}

unsafe extern "system" fn get_instance_proc_addr(
	instance: RawInstance,
	name: *const c_char,
) -> Option<VoidFunction> {
	// SAFETY: the bindings pass C strings.
	let name = unsafe { CStr::from_ptr(name) };
	with(|driver| {
		driver
			.lookups
			.push((instance, name.to_string_lossy().into()));
		let global = matches!(
			name.to_bytes(),
			b"vkEnumerateInstanceVersion" | b"vkCreateInstance"
		);
		if global && !instance.is_null() {
			driver
				.invalid
				.push(format!("{name:?} looked up with an instance"));
		} else if !global {
			driver.check_live(instance, "vkGetInstanceProcAddr");
		}
		if driver.lacks.contains(&name) {
			return None;
		}
		// SAFETY: the bindings cast each back to its own type.
		unsafe {
			Some(match name.to_bytes() {
				b"vkEnumerateInstanceVersion" if driver.version.is_some() => {
					transmute::<EnumerateInstanceVersion, VoidFunction>(enumerate_instance_version)
				}
				b"vkCreateInstance" => transmute::<CreateInstance, VoidFunction>(create_instance),
				b"vkDestroyInstance" => transmute::<DestroyInstance, VoidFunction>(destroy_instance),
				b"vkEnumeratePhysicalDevices" => {
					transmute::<EnumeratePhysicalDevices, VoidFunction>(enumerate_physical_devices)
				}
				_ => return None,
			})
		}
	})
}

unsafe extern "system" fn enumerate_instance_version(version: *mut u32) -> i32 {
	with(|driver| {
		if let Some(code) = driver.failing(c"vkEnumerateInstanceVersion") {
			return code;
		}
		// SAFETY: the bindings pass a writable u32.
		unsafe { version.write(driver.version.unwrap()) };
		0
	})
}

/// A C string the bindings passed, or "(null)".
fn string(text: *const c_char) -> String {
	if text.is_null() {
		return "(null)".into();
	}
	// SAFETY: the bindings pass C strings.
	unsafe { CStr::from_ptr(text) }.to_string_lossy().into()
}

unsafe extern "system" fn create_instance(
	info: *const InstanceCreateInfo,
	allocator: *const c_void,
	instance: *mut RawInstance,
) -> i32 {
	// SAFETY: the bindings pass a create info that lives across the call.
	let info = unsafe { &*info };
	with(|driver| {
		let (layers, extensions) = (info.enabled_layer_count, info.enabled_extension_count);
		let mut rules = vec![
			(info.s_type == STRUCTURE_TYPE_INSTANCE_CREATE_INFO, "sType"),
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(allocator.is_null(), "no allocator"),
			(!info.application_info.is_null(), "pApplicationInfo"),
			(
				layers == 0 || !info.enabled_layer_names.is_null(),
				"layer names",
			),
			(
				extensions == 0 || !info.enabled_extension_names.is_null(),
				"extension names",
			),
		];
		if !info.application_info.is_null() {
			// SAFETY: checked non-null; it lives across the call.
			let application = unsafe { &*info.application_info };
			rules.push((
				application.s_type == STRUCTURE_TYPE_APPLICATION_INFO,
				"app sType",
			));
			rules.push((application.next.is_null(), "app pNext"));
			// apiVersion must be 0 or at least 1.0.
			let api = application.api_version;
			rules.push((api == 0 || api >= api_version(1, 0), "apiVersion"));
			driver.created.push(Created {
				application: string(application.application_name),
				engine: string(application.engine_name),
				api,
				layers,
				extensions,
			});
		}
		let broken = rules.iter().filter(|(holds, _)| !holds);
		(driver.invalid).extend(broken.map(|(_, rule)| format!("vkCreateInstance: {rule}")));
		if let Some(code) = driver.failing(c"vkCreateInstance") {
			return code;
		}
		// Distinct, never null, and never reused.
		let handle = (0x1000 + 0x10 * driver.created.len()) as RawInstance;
		driver.live.push(handle);
		// SAFETY: the bindings pass a writable handle.
		unsafe { instance.write(handle) };
		0
	})
}

unsafe extern "system" fn destroy_instance(instance: RawInstance, allocator: *const c_void) {
	with(|driver| {
		driver.check_live(instance, "vkDestroyInstance");
		if !allocator.is_null() {
			driver
				.invalid
				.push("vkDestroyInstance: an allocator".into());
		}
		driver.live.retain(|&live| live != instance);
		driver.destroyed.push(instance);
	})
}

unsafe extern "system" fn enumerate_physical_devices(
	instance: RawInstance,
	count: *mut u32,
	devices: *mut RawPhysicalDevice,
) -> i32 {
	with(|driver| {
		driver.check_live(instance, "vkEnumeratePhysicalDevices");
		if let Some(code) = driver.failing(c"vkEnumeratePhysicalDevices") {
			return code;
		}
		driver.device_calls += 1;
		if devices.is_null() {
			// SAFETY: the bindings pass a writable count.
			unsafe { count.write(driver.devices.len() as u32) };
			if !driver.grown.is_empty() {
				driver.devices = driver.grown.clone();
			}
			return 0;
		}
		let all = &driver.devices;
		// SAFETY: the bindings pass a count, and room for that many devices.
		let room = unsafe { count.read() } as usize;
		let written = room.min(all.len());
		for (i, &device) in all[..written].iter().enumerate() {
			// SAFETY: within the room given.
			unsafe { devices.add(i).write(device) };
		}
		// SAFETY: as above.
		unsafe { count.write(written as u32) };
		if written < all.len() { INCOMPLETE } else { 0 }
	})
}

#[test]
fn instances_ask_for_the_newest_version_used_and_enable_nothing_yet() {
	let cases = [
		(api_version(1, 3) | 280, api_version(1, 3)),
		(api_version(1, 4) | 363, api_version(1, 4)),
		(api_version(1, 5), api_version(1, 4)),
	];
	for (loader, asked) in cases {
		let entry = install(Driver::offering(loader));
		let instance = entry.create_instance(c"poopterm").expect("instance");
		assert_eq!(instance.api_version(), asked);
		drop(instance);
		let driver = finish();
		let created = Created {
			application: "poopterm".into(),
			engine: "poopterm".into(),
			api: asked,
			layers: 0,
			extensions: 0,
		};
		assert_eq!(driver.created, [created]);
		assert_eq!((driver.live.len(), driver.destroyed.len()), (0, 1));
	}
}

#[test]
fn a_loader_older_than_1_3_creates_nothing() {
	let old = [
		(None, api_version(1, 0)),
		(Some(api_version(1, 2) | 198), api_version(1, 2) | 198),
	];
	for (version, reported) in old {
		let entry = install(Driver {
			version,
			..Driver::default()
		});
		assert_eq!(
			entry.create_instance(c"poopterm").err(),
			Some(Error::TooOld(reported))
		);
		let driver = finish();
		assert!(driver.created.is_empty());
		let looked_up: Vec<_> = driver
			.lookups
			.iter()
			.map(|(_, name)| name.as_str())
			.collect();
		assert_eq!(looked_up, ["vkEnumerateInstanceVersion"]);
	}
}

#[test]
fn failing_global_calls_are_errors_with_their_codes() {
	let cases = [
		(c"vkEnumerateInstanceVersion", -1),
		(c"vkCreateInstance", -9),
		(c"vkCreateInstance", -3),
	];
	for (name, code) in cases {
		let mut driver = Driver::offering(api_version(1, 3));
		driver.fails.push((name, code));
		let entry = install(driver);
		assert_eq!(
			entry.create_instance(c"poopterm").err(),
			Some(Error::Failed(code))
		);
		let driver = finish();
		assert!(
			driver.live.is_empty() && driver.destroyed.is_empty(),
			"{name:?}"
		);
		// Nothing is looked up from an instance that was never made.
		assert!(
			driver
				.lookups
				.iter()
				.all(|(instance, _)| instance.is_null())
		);
	}
}

#[test]
fn global_functions_are_looked_up_without_an_instance_and_the_rest_with_it() {
	let entry = install(Driver::offering(api_version(1, 4)));
	let instance = entry.create_instance(c"poopterm").expect("instance");
	let driver = with(std::mem::take);
	let handle = driver.live[0];
	let expected = [
		(ptr::null_mut(), "vkEnumerateInstanceVersion"),
		(ptr::null_mut(), "vkCreateInstance"),
		(handle, "vkDestroyInstance"),
		(handle, "vkEnumeratePhysicalDevices"),
	];
	let lookups: Vec<_> = driver
		.lookups
		.iter()
		.map(|(at, name)| (*at, name.as_str()))
		.collect();
	assert_eq!(lookups, expected);
	with(|installed| *installed = driver);
	drop(instance);
	assert_eq!(finish().destroyed, [handle]);
}

#[test]
fn a_function_the_loader_lacks_is_missing_and_the_instance_is_not_kept() {
	// Without vkCreateInstance there is nothing to destroy.
	let mut driver = Driver::offering(api_version(1, 3));
	driver.lacks.push(c"vkCreateInstance");
	let error = install(driver).create_instance(c"poopterm").err();
	assert_eq!(error, Some(Error::Missing(c"vkCreateInstance")));
	assert!(finish().created.is_empty());
	// Without vkEnumeratePhysicalDevices the new instance is destroyed again.
	let mut driver = Driver::offering(api_version(1, 3));
	driver.lacks.push(c"vkEnumeratePhysicalDevices");
	let error = install(driver).create_instance(c"poopterm").err();
	assert_eq!(error, Some(Error::Missing(c"vkEnumeratePhysicalDevices")));
	let driver = finish();
	assert_eq!((driver.live.len(), driver.destroyed.len()), (0, 1));
	// Without vkDestroyInstance nothing could destroy it: it leaks, and the
	// error says why.
	let mut driver = Driver::offering(api_version(1, 3));
	driver.lacks.push(c"vkDestroyInstance");
	let error = install(driver).create_instance(c"poopterm").err();
	assert_eq!(error, Some(Error::Missing(c"vkDestroyInstance")));
	let driver = finish();
	assert_eq!((driver.live.len(), driver.destroyed.len()), (1, 0));
}

#[test]
fn each_instance_is_destroyed_once_when_dropped() {
	install(Driver::offering(api_version(1, 3)));
	let entry = || Entry {
		get_instance_proc_addr,
		_library: None,
	};
	let first = entry().create_instance(c"first").unwrap();
	let second = entry().create_instance(c"second").unwrap();
	let live = with(|driver| driver.live.clone());
	assert_eq!(live.len(), 2);
	assert_ne!(live[0], live[1]);
	drop(second);
	assert_eq!(with(|driver| driver.live.clone()), [live[0]]);
	drop(first);
	let driver = finish();
	assert_eq!(driver.destroyed, [live[1], live[0]]);
	let names: Vec<_> = driver
		.created
		.iter()
		.map(|created| created.application.as_str())
		.collect();
	assert_eq!(names, ["first", "second"]);
}

#[test]
fn physical_devices_are_listed_in_the_drivers_order() {
	let devices: Vec<RawPhysicalDevice> = [0x20, 0x10, 0x30].map(|at| at as RawPhysicalDevice).into();
	for listed in [vec![], devices[..1].to_vec(), devices.clone()] {
		let mut driver = Driver::offering(api_version(1, 3));
		driver.devices = listed.clone();
		let instance = install(driver).create_instance(c"poopterm").unwrap();
		assert_eq!(instance.physical_devices(), Ok(listed));
		drop(instance);
		finish();
	}
	let mut driver = Driver::offering(api_version(1, 3));
	driver.devices = devices;
	driver.fails.push((c"vkEnumeratePhysicalDevices", -3));
	let instance = install(driver).create_instance(c"poopterm").unwrap();
	assert_eq!(instance.physical_devices(), Err(Error::Failed(-3)));
	drop(instance);
	finish();
}

#[test]
fn a_device_list_that_grows_while_it_is_read_is_read_again() {
	// The count says one device; by the time devices are asked for there
	// are two, so the driver answers VK_INCOMPLETE and is asked again.
	let mut driver = Driver::offering(api_version(1, 3));
	driver.devices = vec![0x40 as RawPhysicalDevice];
	driver.grown = vec![0x40 as RawPhysicalDevice, 0x50 as RawPhysicalDevice];
	let instance = install(driver).create_instance(c"poopterm").unwrap();
	let expected = with(|driver| driver.grown.clone());
	assert_eq!(instance.physical_devices(), Ok(expected));
	assert_eq!(
		with(|driver| driver.device_calls),
		4,
		"count, short read, count, read"
	);
	drop(instance);
	finish();
}

#[test]
fn functions_are_looked_up_once_per_instance_not_per_call() {
	let mut driver = Driver::offering(api_version(1, 3));
	driver.devices = vec![0x60 as RawPhysicalDevice];
	let instance = install(driver).create_instance(c"poopterm").unwrap();
	let lookups = with(|driver| driver.lookups.len());
	for _ in 0..3 {
		assert_eq!(instance.physical_devices().unwrap().len(), 1);
	}
	assert_eq!(with(|driver| driver.lookups.len()), lookups);
	drop(instance);
	assert_eq!(finish().lookups.len(), lookups);
}

#[test]
fn the_fake_driver_reports_broken_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing.
	with(|driver| *driver = Driver::offering(api_version(1, 3)));
	let name = c"poopterm";
	let application = |api_version| ApplicationInfo {
		s_type: STRUCTURE_TYPE_APPLICATION_INFO,
		next: ptr::null(),
		application_name: name.as_ptr(),
		application_version: 0,
		engine_name: name.as_ptr(),
		engine_version: 0,
		api_version,
	};
	let good = application(api_version(1, 3));
	let info = |application: &ApplicationInfo| InstanceCreateInfo {
		s_type: STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
		next: ptr::null(),
		flags: 0,
		application_info: application,
		enabled_layer_count: 0,
		enabled_layer_names: ptr::null(),
		enabled_extension_count: 0,
		enabled_extension_names: ptr::null(),
	};
	let old = application(1);
	let cases = [
		(
			InstanceCreateInfo {
				s_type: 3,
				..info(&good)
			},
			"sType",
		),
		(
			InstanceCreateInfo {
				flags: 1,
				..info(&good)
			},
			"pNext and flags",
		),
		(
			InstanceCreateInfo {
				application_info: ptr::null(),
				..info(&good)
			},
			"pApplicationInfo",
		),
		(
			InstanceCreateInfo {
				enabled_layer_count: 1,
				..info(&good)
			},
			"layer names",
		),
		(
			InstanceCreateInfo {
				enabled_extension_count: 2,
				..info(&good)
			},
			"extension names",
		),
		(info(&old), "apiVersion"),
	];
	for (info, rule) in cases {
		let mut handle = ptr::null_mut();
		// SAFETY: the fake reads only what the create info points to.
		unsafe { create_instance(&info, ptr::null(), &mut handle) };
		let invalid = with(|driver| std::mem::take(&mut driver.invalid));
		assert_eq!(invalid, [format!("vkCreateInstance: {rule}")]);
		// SAFETY: as above.
		unsafe { destroy_instance(handle, ptr::null()) };
	}
	// Destroying twice, or an instance it never made.
	// SAFETY: the fake only records the handle.
	unsafe { destroy_instance(0x1000 as RawInstance, ptr::null()) };
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(invalid, ["vkDestroyInstance on 0x1000, which is not live"]);
	// Looking a global function up with an instance, or an instance's
	// function without one.
	// SAFETY: C strings.
	unsafe {
		get_instance_proc_addr(0x1000 as RawInstance, c"vkCreateInstance".as_ptr());
		get_instance_proc_addr(ptr::null_mut(), c"vkDestroyInstance".as_ptr());
	}
	let invalid = with(std::mem::take).invalid;
	assert_eq!(invalid.len(), 2, "{invalid:?}");
}
