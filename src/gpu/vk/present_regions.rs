//! `VK_KHR_incremental_present`: the regions of an image that changed
//! since the last one shown, which a present may name so the compositor
//! redraws only those. The image is always drawn whole; the regions are a
//! hint that never makes a present wrong, only cheaper.
use std::ffi::c_void;

pub(crate) const STRUCTURE_TYPE_PRESENT_REGIONS: i32 = 1_000_084_000;

/// `VkRectLayerKHR`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RectLayer {
	pub offset: [i32; 2],
	pub extent: [u32; 2],
	pub layer: u32,
}

/// `VkPresentRegionKHR`: one swapchain's rectangles.
#[repr(C)]
pub(crate) struct PresentRegion {
	pub rectangle_count: u32,
	pub rectangles: *const RectLayer,
}

/// `VkPresentRegionsKHR`, chained to `VkPresentInfoKHR`.
#[repr(C)]
pub(crate) struct PresentRegions {
	pub s_type: i32,
	pub next: *const c_void,
	pub swapchain_count: u32,
	pub regions: *const PresentRegion,
}

/// Pixel rows `start..end` that changed, as rectangles across an image
/// `extent` pixels large, cut to it.
pub(crate) fn rects(bands: &[(usize, usize)], extent: [u32; 2]) -> Vec<RectLayer> {
	let [width, height] = extent;
	let rect = |&(start, end): &(usize, usize)| {
		let end = u32::try_from(end).unwrap_or(u32::MAX).min(height);
		let start = i32::try_from(start).ok()?;
		let rows = end.checked_sub(start as u32).filter(|&rows| rows > 0)?;
		(width > 0).then_some(RectLayer {
			offset: [0, start],
			extent: [width, rows],
			layer: 0,
		})
	};
	bands.iter().filter_map(rect).collect()
}
