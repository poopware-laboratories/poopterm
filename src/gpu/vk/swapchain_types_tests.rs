//! The layout of the swapchain create info against the specification's C
//! declaration on 64-bit targets: size, alignment and every field's
//! offset.
use super::swapchain_types::*;
use std::mem::{align_of, offset_of, size_of};

#[test]
fn swapchain_create_info_is_laid_out_as_vk_swapchain_create_info_khr() {
	let size = (
		size_of::<SwapchainCreateInfo>(),
		align_of::<SwapchainCreateInfo>(),
	);
	assert_eq!(size, (104, 8));
	let offsets = [
		offset_of!(SwapchainCreateInfo, s_type),
		offset_of!(SwapchainCreateInfo, next),
		offset_of!(SwapchainCreateInfo, flags),
		offset_of!(SwapchainCreateInfo, surface),
		offset_of!(SwapchainCreateInfo, min_image_count),
		offset_of!(SwapchainCreateInfo, image_format),
		offset_of!(SwapchainCreateInfo, image_color_space),
		offset_of!(SwapchainCreateInfo, image_extent),
		offset_of!(SwapchainCreateInfo, image_array_layers),
		offset_of!(SwapchainCreateInfo, image_usage),
		offset_of!(SwapchainCreateInfo, image_sharing_mode),
		offset_of!(SwapchainCreateInfo, queue_family_index_count),
		offset_of!(SwapchainCreateInfo, queue_family_indices),
		offset_of!(SwapchainCreateInfo, pre_transform),
		offset_of!(SwapchainCreateInfo, composite_alpha),
		offset_of!(SwapchainCreateInfo, present_mode),
		offset_of!(SwapchainCreateInfo, clipped),
		offset_of!(SwapchainCreateInfo, old_swapchain),
	];
	let expected = [
		0, 8, 16, 24, 32, 36, 40, 44, 52, 56, 60, 64, 72, 80, 84, 88, 92, 96,
	];
	assert_eq!(offsets, expected);
}

#[test]
fn present_info_is_laid_out_as_vk_present_info_khr() {
	assert_eq!(
		(size_of::<PresentInfo>(), align_of::<PresentInfo>()),
		(64, 8)
	);
	let offsets = [
		offset_of!(PresentInfo, s_type),
		offset_of!(PresentInfo, next),
		offset_of!(PresentInfo, wait_semaphore_count),
		offset_of!(PresentInfo, wait_semaphores),
		offset_of!(PresentInfo, swapchain_count),
		offset_of!(PresentInfo, swapchains),
		offset_of!(PresentInfo, image_indices),
		offset_of!(PresentInfo, results),
	];
	assert_eq!(offsets, [0, 8, 16, 24, 32, 40, 48, 56]);
}
