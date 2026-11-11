//! Device extensions the renderer uses when a device has them and does
//! without otherwise. `POOPTERM_VULKAN_DISABLE` turns any of them off by
//! its short name (comma-separated, or `all`), for drivers that offer one
//! but get it wrong.
use std::ffi::CStr;

/// The environment variable naming extensions not to use.
pub(crate) const VARIABLE: &str = "POOPTERM_VULKAN_DISABLE";

/// `VK_KHR_incremental_present`: the present names the regions that
/// changed, so the compositor redraws only those.
pub(crate) const INCREMENTAL_PRESENT: &CStr = c"VK_KHR_incremental_present";

/// `VK_KHR_present_mode_fifo_latest_ready`, and the EXT it was promoted
/// from, which older drivers offer alone: a FIFO mode showing the newest
/// frame ready, for surfaces without mailbox.
pub(crate) const FIFO_LATEST_READY: [&CStr; 2] = [
	c"VK_KHR_present_mode_fifo_latest_ready",
	c"VK_EXT_present_mode_fifo_latest_ready",
];

/// `VkPhysicalDevicePresentModeFifoLatestReadyFeaturesKHR`'s type.
pub(crate) const FIFO_LATEST_READY_FEATURE: i32 = 1_000_361_000;

/// `VK_EXT_memory_budget`: how much of each heap is free, so the glyph
/// atlas leaves room for others.
pub(crate) const MEMORY_BUDGET: &CStr = c"VK_EXT_memory_budget";

/// `VK_EXT_device_fault`: why a device was lost, for the log.
pub(crate) const DEVICE_FAULT: &CStr = c"VK_EXT_device_fault";

/// `VkPhysicalDeviceFaultFeaturesEXT`'s type.
pub(crate) const DEVICE_FAULT_FEATURE: i32 = 1_000_341_000;

/// Which optional extensions a device is made with: each by the name the
/// device offered it under, or `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Optional {
	pub incremental_present: Option<&'static CStr>,
	pub fifo_latest_ready: Option<&'static CStr>,
	pub memory_budget: Option<&'static CStr>,
	pub device_fault: Option<&'static CStr>,
}

/// Where an extension's name is.
type Field = fn(&mut Optional) -> &mut Option<&'static CStr>;

/// An optional extension.
struct Extension {
	/// Its names, the preferred first.
	names: &'static [&'static CStr],
	/// What `POOPTERM_VULKAN_DISABLE` calls it.
	short: &'static str,
	field: Field,
	/// The type of the one-flag structure turning on the feature it is
	/// used for, when it has one.
	feature: Option<i32>,
}

/// Every optional extension.
const ALL: [Extension; 4] = [
	Extension {
		names: &[INCREMENTAL_PRESENT],
		short: "incremental_present",
		field: |optional| &mut optional.incremental_present,
		feature: None,
	},
	Extension {
		names: &FIFO_LATEST_READY,
		short: "fifo_latest_ready",
		field: |optional| &mut optional.fifo_latest_ready,
		feature: Some(FIFO_LATEST_READY_FEATURE),
	},
	Extension {
		names: &[MEMORY_BUDGET],
		short: "memory_budget",
		field: |optional| &mut optional.memory_budget,
		feature: None,
	},
	Extension {
		names: &[DEVICE_FAULT],
		short: "device_fault",
		field: |optional| &mut optional.device_fault,
		feature: Some(DEVICE_FAULT_FEATURE),
	},
];

impl Optional {
	/// Those among the device's `offered` extensions, less the short names
	/// in `disable`, the variable's value.
	pub(crate) fn found(offered: &[String], disable: Option<&str>) -> Self {
		let disabled: Vec<&str> = disable.unwrap_or("").split(',').map(str::trim).collect();
		let mut optional = Self::default();
		for extension in ALL {
			let off = disabled.contains(&extension.short) || disabled.contains(&"all");
			let mut names = extension.names.iter().copied();
			let name = names.find(|name| {
				offered
					.iter()
					.any(|offer| offer.as_bytes() == name.to_bytes())
			});
			*(extension.field)(&mut optional) = name.filter(|_| !off);
		}
		optional
	}

	/// The extensions on, with their entries in [`ALL`].
	fn on(&self) -> impl Iterator<Item = (&'static CStr, Extension)> {
		let mut optional = *self;
		ALL.into_iter().filter_map(move |extension| {
			Some((
				(extension.field)(&mut optional).as_ref().copied()?,
				extension,
			))
		})
	}

	/// The extensions to make the device with.
	pub(crate) fn extensions(&self) -> Vec<&'static CStr> {
		self.on().map(|(name, _)| name).collect()
	}

	/// The short names of those enabled.
	pub(crate) fn names(&self) -> Vec<&'static str> {
		self.on().map(|(_, extension)| extension.short).collect()
	}

	/// The types of the feature structures to turn on with them.
	pub(crate) fn features(&self) -> Vec<i32> {
		self
			.on()
			.filter_map(|(_, extension)| extension.feature)
			.collect()
	}

	/// Turns off those whose feature the device lacks, as `supported` says.
	pub(crate) fn keep_supported(&mut self, mut supported: impl FnMut(i32) -> bool) {
		for extension in ALL {
			let on = (extension.field)(self).is_some();
			if on && extension.feature.is_some_and(|feature| !supported(feature)) {
				*(extension.field)(self) = None;
			}
		}
	}
}
