//! Tests of what a lost device says through `VK_EXT_device_fault`, against
//! the fake driver of `driver_tests`: the device made with the extension
//! and only its first feature, the bounded two-call read, the text it
//! makes, and a window's lost device saying why. The fake's function lives
//! here and checks the specification's valid usage rules.
use super::{
	call::{Error, INCOMPLETE},
	device::Device,
	driver_tests::{Driver, finish, install, with},
	fault::*,
	optional::{DEVICE_FAULT, DEVICE_FAULT_FEATURE, Optional},
	types::*,
	window_tests::{SIZE, close, draw, open, showing},
};
use std::{ffi::CStr, mem::transmute, rc::Rc};

const FUNCTION: &CStr = c"vkGetDeviceFaultInfoEXT";

/// What the fake's driver says once lost, and the calls asking it.
#[derive(Default)]
pub(super) struct Faults {
	pub description: &'static [u8],
	/// Each address's kind, the address reported and its precision.
	pub addresses: Vec<(i32, u64, u64)>,
	/// Each vendor report's description, code and data.
	pub vendor: Vec<(&'static [u8], u64, u64)>,
	pub asked: Vec<String>,
}

pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast it back to its own type.
	(name == FUNCTION).then(|| unsafe { transmute::<GetDeviceFaultInfo, VoidFunction>(get_info) })
}

/// Copies `text` into a fixed C string field, keeping its last NUL.
fn copy(text: &[u8], field: &mut [std::ffi::c_char]) {
	for (to, &from) in field.iter_mut().zip(&text[..text.len().min(255)]) {
		*to = from as std::ffi::c_char;
	}
}

unsafe extern "system" fn get_info(device: RawDevice, counts: *mut Counts, info: *mut Info) -> i32 {
	with(|driver| {
		let call = "vkGetDeviceFaultInfoEXT";
		driver.check_live_device(device, call);
		let made = driver
			.made
			.iter()
			.any(|made| made.enabled.contains(&(DEVICE_FAULT_FEATURE, 0)));
		driver.require(made, || format!("{call}: deviceFault enabled"));
		// SAFETY: the bindings pass writable counts.
		let counts = unsafe { &mut *counts };
		let rules = [
			(
				counts.s_type == STRUCTURE_TYPE_DEVICE_FAULT_COUNTS && counts.next.is_null(),
				"counts sType and pNext",
			),
			(counts.binary_size == 0, "no vendor binary"),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("{call}: {rule}"));
		}
		// Failing always, or from the call numbered as `fails_from` says.
		let calls = driver.faults.asked.len();
		let mut from = driver.fails_from.iter();
		let later = from.find(|&&(name, after, _)| name == FUNCTION && calls >= after);
		if let Some(code) = driver.failing(FUNCTION).or(later.map(|&(_, _, code)| code)) {
			return code;
		}
		let faults = &driver.faults;
		let have = (faults.addresses.len() as u32, faults.vendor.len() as u32);
		if info.is_null() {
			driver.faults.asked.push("counts".into());
			(counts.addresses, counts.vendors) = have;
			return 0;
		}
		// SAFETY: the bindings pass a writable info.
		let info = unsafe { &mut *info };
		let rules = [
			(
				info.s_type == STRUCTURE_TYPE_DEVICE_FAULT_INFO && info.next.is_null(),
				"info sType and pNext",
			),
			(info.binary.is_null(), "no vendor binary"),
			(
				counts.addresses == 0 || !info.addresses.is_null(),
				"room for addresses",
			),
			(
				counts.vendors == 0 || !info.vendors.is_null(),
				"room for vendor reports",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("{call}: {rule}"));
		}
		let asked = format!(
			"{} addresses, {} vendor reports",
			counts.addresses, counts.vendors
		);
		let faults = &driver.faults;
		copy(faults.description, &mut info.description);
		// Nothing written where there is no room, the rule above broken.
		let room = |list: bool, count: u32| if list { 0 } else { count as usize };
		let addresses = faults
			.addresses
			.iter()
			.take(room(info.addresses.is_null(), counts.addresses));
		for (index, &(kind, reported, precision)) in addresses.enumerate() {
			let address = AddressInfo {
				kind,
				reported,
				precision,
			};
			// SAFETY: room for `counts.addresses`, checked above.
			unsafe { info.addresses.add(index).write(address) };
		}
		for (index, &(text, code, data)) in faults
			.vendor
			.iter()
			.take(room(info.vendors.is_null(), counts.vendors))
			.enumerate()
		{
			let mut vendor = VendorInfo {
				description: [0; 256],
				code,
				data,
			};
			copy(text, &mut vendor.description);
			// SAFETY: as above.
			unsafe { info.vendors.add(index).write(vendor) };
		}
		let written = (counts.addresses.min(have.0), counts.vendors.min(have.1));
		(counts.addresses, counts.vendors) = written;
		driver.faults.asked.push(asked);
		if written == have { 0 } else { INCOMPLETE }
	})
}

/// A machine whose device offers `VK_EXT_device_fault`, unless `lacking`,
/// its driver saying `faults` once lost.
fn faulty(lacking: bool, faults: Faults) -> Driver {
	let mut driver = showing();
	if !lacking {
		driver.device_extensions = vec!["VK_EXT_device_fault"];
	}
	driver.faults = faults;
	driver
}

/// A device on `driver`, with the optional extensions it offers.
fn device(driver: Driver) -> Rc<Device> {
	let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
	let physical = instance.physical_devices().unwrap()[0];
	let offered = instance.device_extensions(physical).unwrap();
	let optional = Optional::found(&offered, None);
	let made = Device::with_optional(instance, physical, 0, optional);
	Rc::new(made.expect("a device"))
}

fn sample() -> Faults {
	Faults {
		description: b"GPU hang in queue 0",
		addresses: vec![(1, 0x1234_5678, 0x1000), (0, 0, 0), (6, 0xdead_beef, 1)],
		vendor: vec![(b"ring gfx timeout", 0x2a, 0x7)],
		asked: Vec::new(),
	}
}

#[test]
fn a_lost_device_says_what_its_driver_does() {
	let device = device(faulty(false, sample()));
	let fault = read(&device).expect("a fault");
	let expected = Fault {
		description: "GPU hang in queue 0".into(),
		addresses: vec![
			Address {
				kind: 1,
				first: 0x1234_5000,
				last: 0x1234_5fff,
			},
			Address {
				kind: 0,
				first: 0,
				last: 0,
			},
			Address {
				kind: 6,
				first: 0xdead_beef,
				last: 0xdead_beef,
			},
		],
		vendor: vec![Vendor {
			description: "ring gfx timeout".into(),
			code: 0x2a,
			data: 0x7,
		}],
	};
	assert_eq!(fault, expected);
	let said = "GPU hang in queue 0; a read of invalid address 0x12345000–0x12345fff; \
		a faulting instruction at 0xdeadbeef; ring gfx timeout (code 0x2a, data 0x7)";
	assert_eq!(fault.to_string(), said);
	drop(device);
	let driver = finish();
	assert_eq!(
		driver.faults.asked,
		["counts", "3 addresses, 1 vendor reports"]
	);
	assert!(driver.invalid.is_empty(), "{:?}", driver.invalid);
}

#[test]
fn a_device_is_made_with_the_extension_and_only_its_first_feature() {
	drop(device(faulty(false, Faults::default())));
	let driver = finish();
	let made = &driver.made[0];
	assert!(
		made
			.extensions
			.iter()
			.any(|name| name == "VK_EXT_device_fault")
	);
	assert!(
		made.enabled.contains(&(DEVICE_FAULT_FEATURE, 0)),
		"deviceFault"
	);
	// Never `deviceFaultVendorBinary`: what it holds is not for a log.
	assert!(!made.enabled.contains(&(DEVICE_FAULT_FEATURE, 1)));
	assert!(driver.invalid.is_empty(), "{:?}", driver.invalid);
}

#[test]
fn a_device_without_the_extension_is_never_asked() {
	let device = device(faulty(true, sample()));
	assert_eq!(read(&device), None);
	drop(device);
	let driver = finish();
	assert_eq!(driver.faults.asked, Vec::<String>::new());
	assert!(
		!driver.made[0]
			.enabled
			.iter()
			.any(|&(s_type, _)| s_type == DEVICE_FAULT_FEATURE)
	);
}

#[test]
fn a_device_the_variable_turns_it_off_for_is_never_asked() {
	let driver = faulty(false, sample());
	let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
	let physical = instance.physical_devices().unwrap()[0];
	let offered = instance.device_extensions(physical).unwrap();
	let optional = Optional::found(&offered, Some("memory_budget, device_fault"));
	assert_eq!(optional.device_fault, None);
	let device = Device::with_optional(instance, physical, 0, optional).unwrap();
	assert_eq!(read(&device), None);
	drop(device);
	assert_eq!(finish().faults.asked, Vec::<String>::new());
}

#[test]
fn a_driver_listing_more_than_eight_of_each_gives_the_first_eight() {
	let mut faults = sample();
	faults.addresses = (0..20).map(|n| (2, 0x1000 * n, 1)).collect();
	faults.vendor = vec![(b"v".as_slice(), 1, 2); 11];
	let device = device(faulty(false, faults));
	let fault = read(&device).expect("a fault");
	assert_eq!(fault.addresses.len(), MOST as usize);
	assert_eq!(fault.vendor.len(), MOST as usize);
	assert_eq!(
		fault.addresses[7],
		Address {
			kind: 2,
			first: 0x7000,
			last: 0x7000
		}
	);
	drop(device);
	let driver = finish();
	assert_eq!(
		driver.faults.asked,
		["counts", "8 addresses, 8 vendor reports"]
	);
	assert!(driver.invalid.is_empty(), "{:?}", driver.invalid);
}

#[test]
fn a_fault_with_nothing_listed_passes_no_lists() {
	let faults = Faults {
		description: b"",
		..Faults::default()
	};
	let device = device(faulty(false, faults));
	let fault = read(&device).expect("a fault");
	assert_eq!(fault.to_string(), "no description");
	drop(device);
	let driver = finish();
	assert_eq!(
		driver.faults.asked,
		["counts", "0 addresses, 0 vendor reports"]
	);
	assert!(driver.invalid.is_empty(), "{:?}", driver.invalid);
}

#[test]
fn a_driver_failing_either_call_says_nothing() {
	for from in [0, 1] {
		let mut driver = faulty(false, sample());
		driver.fails_from = vec![(FUNCTION, from, -1)];
		let device = device(driver);
		assert_eq!(read(&device), None, "failing from call {from}");
		drop(device);
		finish();
	}
}

#[test]
fn a_drivers_text_is_kept_to_one_line_and_to_its_nul() {
	let mut faults = sample();
	faults.description = b"line one\nline \x1b[31mtwo\x00 hidden";
	faults.vendor = vec![(b"bad \xff byte\r", 0, 0)];
	faults.addresses.clear();
	let device = device(faulty(false, faults));
	let fault = read(&device).expect("a fault");
	assert_eq!(fault.description, "line oneline [31mtwo");
	assert_eq!(fault.vendor[0].description, "bad \u{fffd} byte");
	drop(device);
	finish();
}

#[test]
fn a_text_filling_its_field_stops_at_its_end() {
	let long: &'static [u8] = Box::leak(vec![b'x'; 400].into_boxed_slice());
	let faults = Faults {
		description: long,
		..Faults::default()
	};
	let device = device(faulty(false, faults));
	assert_eq!(read(&device).expect("a fault").description.len(), 255);
	drop(device);
	finish();
}

#[test]
fn an_address_is_known_down_to_its_precision() {
	let at = |reported, precision| {
		let address = Address::new(AddressInfo {
			kind: 1,
			reported,
			precision,
		});
		(address.first, address.last)
	};
	assert_eq!(at(0x1234, 1), (0x1234, 0x1234));
	// None said is as precise as can be.
	assert_eq!(at(0x1234, 0), (0x1234, 0x1234));
	assert_eq!(at(0x1234, 0x100), (0x1200, 0x12ff));
	// Not a power of two, as it must be: rounded up to one.
	assert_eq!(at(0x1234, 0x300), (0x1000, 0x13ff));
	assert_eq!(at(u64::MAX, 1 << 63), (1 << 63, u64::MAX));
	// Past the largest power of two: anywhere.
	assert_eq!(at(0x1234, u64::MAX), (0, u64::MAX));
}

#[test]
fn each_kind_of_address_reads_as_what_happened() {
	let kinds = [
		(1, "a read of invalid address 0x10"),
		(2, "a write to invalid address 0x10"),
		(3, "a jump to invalid address 0x10"),
		(4, "an unknown instruction at 0x10"),
		(5, "an invalid instruction at 0x10"),
		(6, "a faulting instruction at 0x10"),
		(7, "a fault at 0x10"),
	];
	for (kind, said) in kinds {
		let address = Address {
			kind,
			first: 0x10,
			last: 0x10,
		};
		assert_eq!(address.to_string(), said);
	}
}

#[test]
fn a_window_whose_device_is_lost_says_why() {
	let mut window = open(faulty(false, sample()), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	with(|driver| driver.fails = vec![(c"vkQueueSubmit2", DEVICE_LOST)]);
	let error = draw(&mut window, SIZE).unwrap_err();
	let Error::Lost(fault) = &error else {
		panic!("not lost: {error:?}");
	};
	assert_eq!(fault.description, "GPU hang in queue 0");
	let said = error.to_string();
	assert!(
		said.starts_with("the Vulkan device was lost: GPU hang in queue 0; "),
		"{said}"
	);
	with(|driver| driver.fails.clear());
	let driver = close(window);
	assert_eq!(driver.faults.asked.len(), 2);
}

#[test]
fn a_window_without_the_extension_or_failing_otherwise_says_as_before() {
	let mut window = open(faulty(true, sample()), false).unwrap();
	with(|driver| driver.fails = vec![(c"vkQueueSubmit2", DEVICE_LOST)]);
	assert_eq!(draw(&mut window, SIZE), Err(Error::Failed(DEVICE_LOST)));
	with(|driver| driver.fails.clear());
	close(window);
	// Out of device memory is no loss: nothing to ask.
	let mut window = open(faulty(false, sample()), false).unwrap();
	with(|driver| driver.fails = vec![(c"vkQueueSubmit2", -2)]);
	assert_eq!(draw(&mut window, SIZE), Err(Error::Failed(-2)));
	with(|driver| driver.fails.clear());
	assert_eq!(close(window).faults.asked, Vec::<String>::new());
}

#[test]
fn the_fake_driver_checks_the_fault_rules() {
	// Were it blind to them, the tests above would prove nothing.
	let device = device(faulty(true, sample()));
	let mut counts = Counts {
		s_type: 0,
		next: std::ptr::null_mut(),
		addresses: 1,
		vendors: 0,
		binary_size: 4,
	};
	let mut info = Info {
		s_type: 0,
		next: std::ptr::null_mut(),
		description: [0; 256],
		addresses: std::ptr::null_mut(),
		vendors: std::ptr::null_mut(),
		binary: std::ptr::NonNull::dangling().as_ptr(),
	};
	// SAFETY: the fake writes nothing it has no room for.
	unsafe { get_info(device.handle, &mut counts, &mut info) };
	let expected = [
		"vkGetDeviceFaultInfoEXT: deviceFault enabled",
		"vkGetDeviceFaultInfoEXT: counts sType and pNext",
		"vkGetDeviceFaultInfoEXT: no vendor binary",
		"vkGetDeviceFaultInfoEXT: info sType and pNext",
		"vkGetDeviceFaultInfoEXT: no vendor binary",
		"vkGetDeviceFaultInfoEXT: room for addresses",
	];
	assert_eq!(with(|driver| std::mem::take(&mut driver.invalid)), expected);
	drop(device);
	finish();
}

#[test]
fn the_fault_structures_and_constants_are_as_vulkan_declares_them() {
	use std::mem::{offset_of, size_of};
	assert_eq!(size_of::<Counts>(), 32);
	assert_eq!(
		[
			offset_of!(Counts, addresses),
			offset_of!(Counts, vendors),
			offset_of!(Counts, binary_size)
		],
		[16, 20, 24]
	);
	assert_eq!(size_of::<AddressInfo>(), 24);
	assert_eq!(
		[
			offset_of!(AddressInfo, reported),
			offset_of!(AddressInfo, precision)
		],
		[8, 16]
	);
	assert_eq!(size_of::<VendorInfo>(), 272);
	assert_eq!(
		[offset_of!(VendorInfo, code), offset_of!(VendorInfo, data)],
		[256, 264]
	);
	assert_eq!(size_of::<Info>(), 296);
	assert_eq!(
		[
			offset_of!(Info, description),
			offset_of!(Info, addresses),
			offset_of!(Info, vendors),
			offset_of!(Info, binary)
		],
		[16, 272, 280, 288]
	);
	assert_eq!(DEVICE_FAULT.to_bytes(), b"VK_EXT_device_fault");
	assert_eq!(
		[
			DEVICE_FAULT_FEATURE,
			STRUCTURE_TYPE_DEVICE_FAULT_COUNTS,
			STRUCTURE_TYPE_DEVICE_FAULT_INFO
		],
		[1_000_341_000, 1_000_341_001, 1_000_341_002]
	);
	assert_eq!(DEVICE_LOST, -4);
}
