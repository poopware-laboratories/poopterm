//! What the GPU drawing a window is and how it draws, as plain data, and
//! the rows the Diagnostics page and its report list for it.
use super::atlas::Usage;

/// The swapchain as made: its format's name, present mode and images.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Swapchain {
	pub format: &'static str,
	/// `VkPresentModeKHR`.
	pub mode: i32,
	pub images: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuFacts {
	pub device: String,
	/// The GPU API: "Vulkan" or "Metal".
	pub api: &'static str,
	/// The API version the device offers, packed as Vulkan packs it.
	pub version: Option<u32>,
	/// The driver's own name and version, as it reports them.
	pub driver: Option<String>,
	/// `None` before the window first drew.
	pub swapchain: Option<Swapchain>,
	pub atlas: Option<Usage>,
	pub presenter: String,
}

/// A packed Vulkan version as major.minor.patch.
pub(crate) fn version(packed: u32) -> String {
	let (major, minor, patch) = (packed >> 22 & 0x7f, packed >> 12 & 0x3ff, packed & 0xfff);
	format!("{major}.{minor}.{patch}")
}

/// A present mode's name in the specification.
pub(crate) fn mode_name(mode: i32) -> String {
	const NAMES: [&str; 4] = ["IMMEDIATE", "MAILBOX", "FIFO", "FIFO_RELAXED"];
	match usize::try_from(mode).ok().and_then(|mode| NAMES.get(mode)) {
		Some(name) => (*name).to_owned(),
		// `VK_PRESENT_MODE_FIFO_LATEST_READY_KHR`.
		None if mode == 1_000_361_000 => "FIFO_LATEST_READY".to_owned(),
		None => format!("mode {mode}"),
	}
}

impl GpuFacts {
	/// The page's rows, leaving out what is not known.
	pub(crate) fn rows(&self) -> Vec<(&'static str, String)> {
		let driver = self
			.driver
			.as_deref()
			.map(str::trim)
			.filter(|d| !d.is_empty());
		let api = self
			.version
			.map(|packed| format!("{} {}", self.api, version(packed)));
		let driver = match (driver, api) {
			(Some(driver), Some(api)) => Some(format!("{driver} \u{b7} {api}")),
			(driver, api) => driver.map(str::to_owned).or(api),
		};
		let swapchain = self.swapchain.as_ref().map(|chain| {
			let plural = if chain.images == 1 { "" } else { "s" };
			let mode = mode_name(chain.mode);
			format!(
				"{} \u{b7} {mode} \u{b7} {} image{plural}",
				chain.format, chain.images
			)
		});
		let atlas = self.atlas.map(|usage| {
			let side = usage.side;
			let pages = match usage.pages {
				1 => String::new(),
				pages => format!(" \u{d7} {pages} pages"),
			};
			format!(
				"{side} \u{d7} {side}{pages} \u{b7} {}% used",
				usage.percent()
			)
		});
		[
			("Device", Some(self.device.clone())),
			("Driver", driver),
			("Swapchain", swapchain),
			("Glyph atlas", atlas),
			("Presenter", Some(self.presenter.clone())),
		]
		.into_iter()
		.filter_map(|(label, value)| Some((label, value?)))
		.collect()
	}
}
