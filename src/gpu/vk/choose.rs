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

impl Kind {
	/// The kind a `VkPhysicalDeviceType` value names; unknown ones are
	/// `Other`.
	pub(crate) fn from_type(value: i32) -> Self {
		match value {
			1 => Self::Integrated,
			2 => Self::Discrete,
			3 => Self::Virtual,
			4 => Self::Cpu,
			_ => Self::Other,
		}
	}
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QueueFamily {
	/// `VkQueueFlags`.
	pub flags: u32,
	pub count: u32,
	/// Whether it can show images on the window's surface.
	pub present: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Device {
	pub name: String,
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

/// Why a device cannot draw, in the order [`suits`] checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unsuited {
	/// Its Vulkan version, older than 1.3.
	TooOld(u32),
	/// It draws on the CPU, and software was not asked for.
	Cpu,
	/// It lacks `VK_KHR_swapchain`, and draws for a window.
	NoSwapchain,
	/// No queue family draws (and shows, for a window).
	NoQueue,
}

/// The queue family `device` draws on, for a window or offscreen, allowing
/// a CPU device when `software`; else why it cannot.
pub(crate) fn suits(device: &Device, window: bool, software: bool) -> Result<usize, Unsuited> {
	if major_minor(device.api) < (1, 3) {
		return Err(Unsuited::TooOld(device.api));
	}
	if device.kind == Kind::Cpu && !software {
		return Err(Unsuited::Cpu);
	}
	if window && !device.swapchain {
		return Err(Unsuited::NoSwapchain);
	}
	(device.families.iter())
		.position(|family| {
			family.count > 0 && family.flags & GRAPHICS != 0 && (family.present || !window)
		})
		.ok_or(Unsuited::NoQueue)
}

/// The device to draw on, for a window or offscreen, allowing a CPU device
/// when `software`: the preferred kind, then the newest version, then the
/// first listed; `None` when no device [`suits`].
pub(crate) fn choose(devices: &[Device], window: bool, software: bool) -> Option<Choice> {
	let usable = devices.iter().enumerate().filter_map(|(index, device)| {
		let family = suits(device, window, software).ok()?;
		Some((device.kind, Reverse(major_minor(device.api)), index, family))
	});
	let (.., device, family) = usable.min()?;
	Some(Choice { device, family })
}
