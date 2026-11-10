//! Tests of the regions a present names: the structures laid out as
//! `VK_KHR_incremental_present` declares them, and the bands of changed
//! pixel rows as rectangles inside the image.
use super::present_regions::*;
use std::mem::{align_of, offset_of, size_of};

#[test]
fn the_structures_are_laid_out_as_vulkan_declares_them() {
	assert_eq!((size_of::<RectLayer>(), align_of::<RectLayer>()), (20, 4));
	assert_eq!(
		[
			offset_of!(RectLayer, offset),
			offset_of!(RectLayer, extent),
			offset_of!(RectLayer, layer),
		],
		[0, 8, 16]
	);
	assert_eq!(
		(size_of::<PresentRegion>(), align_of::<PresentRegion>()),
		(16, 8)
	);
	assert_eq!(offset_of!(PresentRegion, rectangles), 8);
	assert_eq!(
		(size_of::<PresentRegions>(), align_of::<PresentRegions>()),
		(32, 8)
	);
	assert_eq!(
		[
			offset_of!(PresentRegions, s_type),
			offset_of!(PresentRegions, next),
			offset_of!(PresentRegions, swapchain_count),
			offset_of!(PresentRegions, regions),
		],
		[0, 8, 16, 24]
	);
	// VK_STRUCTURE_TYPE_PRESENT_REGIONS_KHR.
	assert_eq!(STRUCTURE_TYPE_PRESENT_REGIONS, 1_000_084_000);
}

fn rect(y: i32, width: u32, height: u32) -> RectLayer {
	RectLayer {
		offset: [0, y],
		extent: [width, height],
		layer: 0,
	}
}

#[test]
fn each_band_spans_the_image_width() {
	assert_eq!(
		rects(&[(0, 44), (300, 320)], [800, 600]),
		[rect(0, 800, 44), rect(300, 800, 20)]
	);
}

#[test]
fn bands_past_the_image_are_cut_to_it_or_dropped() {
	// A band the window lost when it shrank, and one running off the end.
	assert_eq!(
		rects(&[(590, 640), (700, 720)], [800, 600]),
		[rect(590, 800, 10)]
	);
}

#[test]
fn empty_bands_and_images_name_no_region() {
	assert_eq!(rects(&[(10, 10), (30, 20)], [800, 600]), []);
	assert_eq!(rects(&[(0, 10)], [0, 600]), []);
	assert_eq!(rects(&[], [800, 600]), []);
}

#[test]
fn bands_too_large_for_vulkans_offsets_are_dropped() {
	let far = i32::MAX as usize + 1;
	assert_eq!(rects(&[(far, far + 10)], [800, u32::MAX]), []);
}

#[test]
fn each_band_is_the_rectangle_a_model_makes_of_it_or_none() {
	// Xorshift, so a failing case comes back the same.
	let mut state = 0xa54f_f53a_5f1d_36f1_u64;
	let mut below = |n: u32| {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		(state % u64::from(n)) as u32
	};
	for case in 0..3000 {
		let extent = [below(3) * below(500), 1 + below(400)];
		let count = below(6);
		let bands: Vec<(usize, usize)> = (0..count)
			.map(|_| {
				(
					below(extent[1] + 20) as usize,
					below(extent[1] + 20) as usize,
				)
			})
			.collect();
		let expected: Vec<_> = (bands.iter())
			.filter_map(|&(start, end)| {
				let end = (end as u32).min(extent[1]);
				let rows = end.checked_sub(start as u32).filter(|&rows| rows > 0)?;
				(extent[0] > 0).then_some(([0, start as i32], [extent[0], rows]))
			})
			.collect();
		let got: Vec<_> = (rects(&bands, extent).into_iter())
			.inspect(|rect| assert_eq!(rect.layer, 0, "case {case}"))
			.map(|rect| (rect.offset, rect.extent))
			.collect();
		assert_eq!(got, expected, "case {case}: {bands:?} in {extent:?}");
		// Every rectangle lies inside the image, as the present requires.
		for (offset, size) in got {
			assert!(offset[1] as u32 + size[1] <= extent[1], "case {case}");
		}
	}
}
