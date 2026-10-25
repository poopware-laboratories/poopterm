//! Which GPU to draw on, and which of its queue families: from plain data
//! about each device, so the choice is tested without a driver.
use super::version::major_minor;
use std::cmp::Reverse;

/// `VK_QUEUE_GRAPHICS_BIT` and `VK_QUEUE_TRANSFER_BIT`.
pub(crate) const GRAPHICS: u32 = 1;
pub(crate) const TRANSFER: u32 = 4;

/// `VkPhysicalDeviceType`, in the order they are preferred: a terminal
/// needs little, and an integrated GPU lets a discrete one sleep.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
	Integrated,
	Discrete,
	Virtual,
	Other,
	/// Vulkan drawn by the CPU, slower than our own software renderer.
	Cpu,
}

#[derive(Clone, Debug)]
pub(crate) struct QueueFamily {
	/// `VkQueueFlags`.
	pub flags: u32,
	pub count: u32,
	/// Whether it can show images on the window's surface.
	pub present: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Device {
	pub kind: Kind,
	pub api: u32,
	pub families: Vec<QueueFamily>,
	/// Whether it has `VK_KHR_swapchain`, which showing in a window needs.
	pub swapchain: bool,
}

/// A device's index, and the index of the queue family to use on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
	pub device: usize,
	pub family: usize,
}

/// The device to draw on, for a window or offscreen, allowing a CPU device
/// when `software`: the preferred kind, then the newest version, then the
/// first listed; `None` when no device is Vulkan 1.3 with a family that
/// draws (and shows, for a window).
pub(crate) fn choose(devices: &[Device], window: bool, software: bool) -> Option<Choice> {
	let usable = devices.iter().enumerate().filter_map(|(index, device)| {
		let version = major_minor(device.api);
		let kind_allowed = device.kind != Kind::Cpu || software;
		if version < (1, 3) || !kind_allowed || (window && !device.swapchain) {
			return None;
		}
		let family = (device.families.iter()).position(|family| {
			family.count > 0 && family.flags & GRAPHICS != 0 && (family.present || !window)
		})?;
		Some((device.kind, Reverse(version), index, family))
	});
	let (.., device, family) = usable.min()?;
	Some(Choice { device, family })
}
