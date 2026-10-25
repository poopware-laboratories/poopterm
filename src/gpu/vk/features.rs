//! The device features the renderer turns on, asked about and enabled as
//! one chain of the Vulkan 1.2 and 1.3 feature structures.
use super::types::*;
use std::ptr;

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

/// `VkPhysicalDeviceFeatures2` chained to the 1.2 and 1.3 features; boxed,
/// so the links stay valid when it moves.
pub(crate) struct Features {
	pub head: PhysicalDeviceFeatures2,
	pub v12: Vulkan12Features,
	pub v13: Vulkan13Features,
}

impl Features {
	/// Every feature off, for the driver to fill in.
	pub(crate) fn none() -> Box<Self> {
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
		});
		features.v12.next = (&raw mut features.v13).cast();
		features.head.next = (&raw mut features.v12).cast();
		features
	}

	/// Only the features the renderer needs on.
	pub(crate) fn needed() -> Box<Self> {
		let mut features = Self::none();
		for (flag, _) in NEEDED {
			*features.flag(flag) = 1;
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
