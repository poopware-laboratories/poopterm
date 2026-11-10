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

/// Which optional extensions a device is made with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Optional {
	pub incremental_present: bool,
}

/// Where an extension's flag is.
type Flag = fn(&mut Optional) -> &mut bool;

/// Each extension: its name, its short name, and its flag.
const ALL: [(&CStr, &str, Flag); 1] = [(INCREMENTAL_PRESENT, "incremental_present", |optional| {
	&mut optional.incremental_present
})];

impl Optional {
	/// Those among the device's `offered` extensions, less the short names
	/// in `disable`, the variable's value.
	pub(crate) fn found(offered: &[String], disable: Option<&str>) -> Self {
		let disabled: Vec<&str> = disable.unwrap_or("").split(',').map(str::trim).collect();
		let mut optional = Self::default();
		for (name, short, flag) in ALL {
			let name = name.to_str().unwrap_or_default();
			let off = disabled.contains(&short) || disabled.contains(&"all");
			*flag(&mut optional) = !off && offered.iter().any(|offer| offer == name);
		}
		optional
	}

	/// The extensions to make the device with.
	pub(crate) fn extensions(&self) -> Vec<&'static CStr> {
		let mut on = *self;
		let enabled = ALL.into_iter().filter(|(_, _, flag)| *flag(&mut on));
		enabled.map(|(name, _, _)| name).collect()
	}

	/// The short names of those enabled.
	pub(crate) fn names(&self) -> Vec<&'static str> {
		let mut on = *self;
		let enabled = ALL.into_iter().filter(|(_, _, flag)| *flag(&mut on));
		enabled.map(|(_, short, _)| short).collect()
	}
}
