//! The device features the renderer turns on, asked about and enabled as
//! one chain of the Vulkan 1.2 and 1.3 feature structures, and those of the
//! optional extensions after them.
use super::types::*;
use std::{ffi::c_void, ptr};

/// Where a feature's `VkBool32` is: its index in the 1.2 or 1.3 structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flag {
	V12(usize),
	V13(usize),
}

/// The features the renderer needs, named as the specification names them.
pub(crate) const NEEDED: [(Flag, &str); 3] = [
	(Flag::V13(12), "dynamicRendering"),
	(Flag::V13(9), "synchronization2"),
	(Flag::V12(37), "timelineSemaphore"),
];

/// A feature structure of one `VkBool32`, as the optional extensions'
/// are: `VkPhysicalDevicePresentModeFifoLatestReadyFeaturesKHR` and others.
#[repr(C)]
pub(crate) struct OneFeature {
	pub s_type: i32,
	pub next: *mut c_void,
	pub enabled: u32,
}

/// `VkPhysicalDeviceFeatures2` chained to the 1.2 and 1.3 features, then to
/// the optional extensions'; boxed, so the links stay valid when it moves.
pub(crate) struct Features {
	pub head: PhysicalDeviceFeatures2,
	pub v12: Vulkan12Features,
	pub v13: Vulkan13Features,
	/// Never grown once linked, so its items stay put.
	pub extra: Vec<OneFeature>,
}

impl Features {
	/// Every feature off, for the driver to fill in.
	pub(crate) fn none() -> Box<Self> {
		Self::none_with(&[])
	}

	/// Every feature off, with the one-flag structures of types `extra`.
	pub(crate) fn none_with(extra: &[i32]) -> Box<Self> {
		let extra = extra.iter().map(|&s_type| OneFeature {
			s_type,
			next: ptr::null_mut(),
			enabled: 0,
		});
		let mut features = Box::new(Self {
			head: PhysicalDeviceFeatures2 {
				s_type: STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2,
				next: ptr::null_mut(),
				features: [0; 55],
			},
			v12: Vulkan12Features {
				s_type: STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES,
				next: ptr::null_mut(),
				flags: [0; 47],
			},
			v13: Vulkan13Features {
				s_type: STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES,
				next: ptr::null_mut(),
				flags: [0; 15],
			},
			extra: extra.collect(),
		});
		let mut next: *mut c_void = ptr::null_mut();
		for link in features.extra.iter_mut().rev() {
			link.next = next;
			next = (&raw mut *link).cast();
		}
		features.v13.next = next;
		features.v12.next = (&raw mut features.v13).cast();
		features.head.next = (&raw mut features.v12).cast();
		features
	}

	/// Only the features the renderer needs on, and those of types
	/// `extra`.
	pub(crate) fn needed(extra: &[i32]) -> Box<Self> {
		let mut features = Self::none_with(extra);
		for (flag, _) in NEEDED {
			*features.flag(flag) = 1;
		}
		for link in &mut features.extra {
			link.enabled = 1;
		}
		features
	}

	fn flag(&mut self, flag: Flag) -> &mut u32 {
		match flag {
			Flag::V12(index) => &mut self.v12.flags[index],
			Flag::V13(index) => &mut self.v13.flags[index],
		}
	}

	pub(crate) fn get(&self, flag: Flag) -> bool {
		match flag {
			Flag::V12(index) => self.v12.flags[index] != 0,
			Flag::V13(index) => self.v13.flags[index] != 0,
		}
	}

	/// The first feature the renderer needs that is off.
	pub(crate) fn missing(&self) -> Option<&'static str> {
		let mut needed = NEEDED.iter();
		needed
			.find(|&&(flag, _)| !self.get(flag))
			.map(|&(_, name)| name)
	}
}
