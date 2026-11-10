//! What the driver says of itself: `VkPhysicalDeviceDriverProperties`,
//! asked for through `vkGetPhysicalDeviceProperties2` (core since 1.2), and
//! the line the Diagnostics page shows for it.
use super::{call::fixed_string, types::*};
use std::{ffi::c_char, ffi::c_void, ptr};

pub(crate) const STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2: i32 = 1_000_059_001;
pub(crate) const STRUCTURE_TYPE_PHYSICAL_DEVICE_DRIVER_PROPERTIES: i32 = 1_000_196_000;

/// `VkPhysicalDeviceProperties2`.
#[repr(C)]
pub(crate) struct PhysicalDeviceProperties2 {
	pub s_type: i32,
	pub next: *mut c_void,
	pub properties: PhysicalDeviceProperties,
}

/// `VkPhysicalDeviceDriverProperties`.
#[repr(C)]
pub(crate) struct DriverProperties {
	pub s_type: i32,
	pub next: *mut c_void,
	/// `VkDriverId`.
	pub driver_id: u32,
	pub driver_name: [c_char; 256],
	pub driver_info: [c_char; 256],
	/// `VkConformanceVersion`: major, minor, subminor, patch.
	pub conformance: [u8; 4],
}

pub(crate) type GetPhysicalDeviceProperties2 =
	unsafe extern "system" fn(RawPhysicalDevice, *mut PhysicalDeviceProperties2);

impl DriverProperties {
	/// Empty, for the driver to fill in.
	pub(crate) fn empty() -> Self {
		Self {
			s_type: STRUCTURE_TYPE_PHYSICAL_DEVICE_DRIVER_PROPERTIES,
			next: ptr::null_mut(),
			driver_id: 0,
			driver_name: [0; 256],
			driver_info: [0; 256],
			conformance: [0; 4],
		}
	}

	/// How the driver names itself: its info ("Mesa 26.2.1"), which says
	/// the version, or else its name ("radv"); `None` when it says neither.
	pub(crate) fn text(&self) -> Option<String> {
		[&self.driver_info, &self.driver_name]
			.into_iter()
			.map(|field| fixed_string(field).trim().to_owned())
			.find(|text| !text.is_empty())
	}
}
