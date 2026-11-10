//! What the driver says went wrong once a device is lost, through
//! `VK_EXT_device_fault`: its description, the addresses that faulted and
//! the vendor's own codes, for the log and the Diagnostics report.
use super::{call::fixed_string, device::Device, types::RawDevice};
use std::{
	ffi::{c_char, c_void},
	fmt, ptr,
};

pub(crate) const STRUCTURE_TYPE_DEVICE_FAULT_COUNTS: i32 = 1_000_341_001;
pub(crate) const STRUCTURE_TYPE_DEVICE_FAULT_INFO: i32 = 1_000_341_002;
/// `VK_ERROR_DEVICE_LOST`.
pub(crate) const DEVICE_LOST: i32 = -4;
/// The most addresses and vendor reports read: a driver's lists are
/// bounded like any other input.
pub(crate) const MOST: u32 = 8;

/// `VkDeviceFaultCountsEXT`.
#[repr(C)]
pub(crate) struct Counts {
	pub s_type: i32,
	pub next: *mut c_void,
	pub addresses: u32,
	pub vendors: u32,
	pub binary_size: u64,
}

/// `VkDeviceFaultAddressInfoEXT`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct AddressInfo {
	pub kind: i32,
	pub reported: u64,
	pub precision: u64,
}

/// `VkDeviceFaultVendorInfoEXT`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VendorInfo {
	pub description: [c_char; 256],
	pub code: u64,
	pub data: u64,
}

/// `VkDeviceFaultInfoEXT`.
#[repr(C)]
pub(crate) struct Info {
	pub s_type: i32,
	pub next: *mut c_void,
	pub description: [c_char; 256],
	pub addresses: *mut AddressInfo,
	pub vendors: *mut VendorInfo,
	pub binary: *mut c_void,
}

pub(crate) type GetDeviceFaultInfo =
	unsafe extern "system" fn(RawDevice, *mut Counts, *mut Info) -> i32;

/// An address that faulted, somewhere from `first` to `last`, and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Address {
	pub kind: i32,
	pub first: u64,
	pub last: u64,
}

/// A fault in the vendor's own terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Vendor {
	pub description: String,
	pub code: u64,
	pub data: u64,
}

/// What the driver said went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fault {
	pub description: String,
	pub addresses: Vec<Address>,
	pub vendor: Vec<Vendor>,
}

impl Address {
	/// The driver's `reported` address, of which the bits below its
	/// `precision`, a power of two, are unknown.
	pub(crate) fn new(info: AddressInfo) -> Self {
		let unknown = info
			.precision
			.max(1)
			.checked_next_power_of_two()
			.map_or(u64::MAX, |p| p - 1);
		Self {
			kind: info.kind,
			first: info.reported & !unknown,
			last: info.reported | unknown,
		}
	}
}

/// A driver's text on one line: up to its NUL, without control characters.
fn text(field: &[c_char]) -> String {
	fixed_string(field)
		.chars()
		.filter(|c| !c.is_control())
		.collect()
}

/// What the driver of `device`, which was lost, says went wrong; `None` on
/// a device made without `VK_EXT_device_fault`, or when the driver fails.
pub(crate) fn read(device: &Device) -> Option<Fault> {
	let get = device.fault?;
	let mut counts = Counts {
		s_type: STRUCTURE_TYPE_DEVICE_FAULT_COUNTS,
		next: ptr::null_mut(),
		addresses: 0,
		vendors: 0,
		binary_size: 0,
	};
	// SAFETY: a lost device may still be asked, `counts` is writable, and
	// without an info only the counts are written.
	if unsafe { get(device.handle, &mut counts, ptr::null_mut()) } < 0 {
		return None;
	}
	counts.addresses = counts.addresses.min(MOST);
	counts.vendors = counts.vendors.min(MOST);
	// The vendor's binary is never turned on, so never asked for.
	counts.binary_size = 0;
	let mut addresses = vec![AddressInfo::default(); counts.addresses as usize];
	let blank = VendorInfo {
		description: [0; 256],
		code: 0,
		data: 0,
	};
	let mut vendors = vec![blank; counts.vendors as usize];
	let room = |items: usize, start: *mut c_void| if items == 0 { ptr::null_mut() } else { start };
	let mut info = Info {
		s_type: STRUCTURE_TYPE_DEVICE_FAULT_INFO,
		next: ptr::null_mut(),
		description: [0; 256],
		addresses: room(addresses.len(), addresses.as_mut_ptr().cast()).cast(),
		vendors: room(vendors.len(), vendors.as_mut_ptr().cast()).cast(),
		binary: ptr::null_mut(),
	};
	// SAFETY: as above, with room for as many of each as `counts` says;
	// `VK_INCOMPLETE` writes fewer.
	if unsafe { get(device.handle, &mut counts, &mut info) } < 0 {
		return None;
	}
	let addresses = addresses.into_iter().take(counts.addresses as usize);
	let vendors = vendors.into_iter().take(counts.vendors as usize);
	Some(Fault {
		description: text(&info.description),
		addresses: addresses.map(Address::new).collect(),
		vendor: vendors
			.map(|vendor| Vendor {
				description: text(&vendor.description),
				code: vendor.code,
				data: vendor.data,
			})
			.collect(),
	})
}

impl fmt::Display for Address {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let what = match self.kind {
			1 => "a read of invalid address",
			2 => "a write to invalid address",
			3 => "a jump to invalid address",
			4 => "an unknown instruction at",
			5 => "an invalid instruction at",
			6 => "a faulting instruction at",
			_ => "a fault at",
		};
		match (self.first, self.last) {
			(first, last) if first == last => write!(f, "{what} {first:#x}"),
			(first, last) => write!(f, "{what} {first:#x}–{last:#x}"),
		}
	}
}

impl fmt::Display for Fault {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self.description.as_str() {
			"" => write!(f, "no description")?,
			description => write!(f, "{description}")?,
		}
		// Kind 0 describes no address.
		for address in self.addresses.iter().filter(|address| address.kind != 0) {
			write!(f, "; {address}")?;
		}
		for vendor in &self.vendor {
			let Vendor {
				description,
				code,
				data,
			} = vendor;
			write!(f, "; {description} (code {code:#x}, data {data:#x})")?;
		}
		Ok(())
	}
}
