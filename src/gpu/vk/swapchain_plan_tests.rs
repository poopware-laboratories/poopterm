//! Tests of choosing a swapchain from what a surface supports: sizes on
//! Wayland and X11, image counts, formats, present modes and alpha, and
//! the surfaces no swapchain suits.
use super::{
	image::Format,
	swapchain_plan::{
		Capabilities, FIFO, INHERIT, MAILBOX, OPAQUE, POST_MULTIPLIED, PRE_MULTIPLIED, Plan,
		SRGB_NONLINEAR, SurfaceFormat, USAGE, Unfit, plan,
	},
};

const BGRA: SurfaceFormat = SurfaceFormat {
	format: 44,
	color_space: SRGB_NONLINEAR,
};
const RGBA: SurfaceFormat = SurfaceFormat {
	format: 37,
	color_space: SRGB_NONLINEAR,
};
const BGRA_SRGB: SurfaceFormat = SurfaceFormat {
	format: 50,
	color_space: SRGB_NONLINEAR,
};
/// `VK_COLOR_SPACE_DISPLAY_P3_NONLINEAR_EXT`.
const BGRA_P3: SurfaceFormat = SurfaceFormat {
	format: 44,
	color_space: 1_000_104_001,
};
/// `VK_PRESENT_MODE_IMMEDIATE_KHR` and `_FIFO_RELAXED_KHR`, which tear.
const IMMEDIATE: i32 = 0;
const FIFO_RELAXED: i32 = 3;
/// `VK_SURFACE_TRANSFORM_ROTATE_90_BIT_KHR`.
const ROTATE_90: u32 = 2;

/// A surface like Mesa's on Wayland: the swapchain sets the size.
fn wayland() -> Capabilities {
	Capabilities {
		min_images: 2,
		max_images: 0,
		current_extent: [u32::MAX; 2],
		min_extent: [1, 1],
		max_extent: [16384, 16384],
		current_transform: 1,
		alpha: OPAQUE | PRE_MULTIPLIED,
		usage: 0x9f,
	}
}

/// A surface like X11's: the window's size, which the swapchain must use.
fn x11(size: [u32; 2]) -> Capabilities {
	Capabilities {
		min_images: 3,
		max_images: 8,
		current_extent: size,
		min_extent: size,
		max_extent: size,
		current_transform: 1,
		alpha: OPAQUE | INHERIT,
		usage: 0x9f,
	}
}

fn planned(capabilities: &Capabilities, window: [u32; 2]) -> Result<Plan, Unfit> {
	plan(capabilities, &[BGRA, RGBA], &[FIFO, MAILBOX], window)
}

#[test]
fn a_wayland_surface_takes_the_windows_size_and_one_image_more_than_needed() {
	let expected = Plan {
		format: Format::Bgra,
		mode: MAILBOX,
		extent: [1280, 720],
		images: 3,
		transform: 1,
		alpha: OPAQUE,
	};
	assert_eq!(planned(&wayland(), [1280, 720]), Ok(expected));
}

#[test]
fn an_x11_surface_keeps_its_own_size_whatever_the_window_reports() {
	// The window's size may lag behind a resize the server already made.
	let made = planned(&x11([800, 600]), [801, 599]).unwrap();
	assert_eq!(made.extent, [800, 600]);
	assert_eq!(made.images, 4);
}

#[test]
fn the_size_is_clamped_to_what_the_surface_allows_on_each_axis() {
	let mut capabilities = wayland();
	capabilities.min_extent = [64, 32];
	capabilities.max_extent = [4096, 2048];
	let cases = [
		([10, 10], [64, 32]),
		([5000, 100], [4096, 100]),
		([100, 9000], [100, 2048]),
		([64, 32], [64, 32]),
		([4096, 2048], [4096, 2048]),
	];
	for (window, extent) in cases {
		assert_eq!(planned(&capabilities, window).unwrap().extent, extent);
	}
}

#[test]
fn a_window_without_area_is_hidden_not_an_error() {
	let mut minimized = x11([0, 0]);
	assert_eq!(planned(&minimized, [800, 600]), Err(Unfit::Hidden));
	minimized.current_extent = [800, 0];
	assert_eq!(planned(&minimized, [800, 600]), Err(Unfit::Hidden));
	let mut capabilities = wayland();
	capabilities.min_extent = [0, 0];
	assert_eq!(planned(&capabilities, [0, 600]), Err(Unfit::Hidden));
	assert_eq!(planned(&capabilities, [600, 0]), Err(Unfit::Hidden));
}

#[test]
fn only_one_axis_left_to_the_swapchain_still_takes_the_surfaces_size() {
	// Only both axes at u32::MAX leave the size to the swapchain.
	let mut capabilities = wayland();
	capabilities.current_extent = [u32::MAX, 700];
	capabilities.max_extent = [u32::MAX, 16384];
	let made = planned(&capabilities, [640, 480]).unwrap();
	assert_eq!(made.extent, [u32::MAX, 700]);
}

#[test]
fn image_counts_stay_within_the_surfaces_limits() {
	let cases = [
		// (min, max, planned)
		(1, 0, 2),
		(2, 0, 3),
		(2, 2, 2),
		(2, 3, 3),
		(3, 8, 4),
		(4, 4, 4),
	];
	for (min, max, images) in cases {
		let mut capabilities = wayland();
		(capabilities.min_images, capabilities.max_images) = (min, max);
		let made = planned(&capabilities, [10, 10]).unwrap();
		assert_eq!(made.images, images, "min {min}, max {max}");
	}
}

#[test]
fn bgra_is_preferred_then_rgba_in_whatever_order_listed() {
	let cases = [
		(vec![RGBA, BGRA], Format::Bgra),
		(vec![BGRA_SRGB, RGBA], Format::Rgba),
		(vec![BGRA_P3, RGBA], Format::Rgba),
		(vec![RGBA], Format::Rgba),
	];
	for (formats, format) in cases {
		let made = plan(&wayland(), &formats, &[FIFO], [10, 10]);
		assert_eq!(made.unwrap().format, format, "{formats:?}");
	}
}

#[test]
fn a_surface_listing_only_undefined_takes_bgra() {
	let any = SurfaceFormat {
		format: 0,
		color_space: SRGB_NONLINEAR,
	};
	let made = plan(&wayland(), &[any], &[FIFO], [10, 10]);
	assert_eq!(made.unwrap().format, Format::Bgra);
	// Among others, it is just a format nobody draws in.
	let made = plan(&wayland(), &[any, BGRA_SRGB], &[FIFO], [10, 10]);
	assert_eq!(made, Err(Unfit::Format));
}

#[test]
fn srgb_encoded_or_wide_gamut_formats_alone_do_not_suit() {
	// The shaders blend values as software does; an sRGB format would
	// encode them again, and another color space would show them wrong.
	let cases = [vec![BGRA_SRGB], vec![BGRA_P3], vec![]];
	for formats in cases {
		let made = plan(&wayland(), &formats, &[FIFO], [10, 10]);
		assert_eq!(made, Err(Unfit::Format), "{formats:?}");
	}
}

#[test]
fn mailbox_is_preferred_and_fifo_is_the_fallback_every_driver_has() {
	let cases = [
		(vec![FIFO, MAILBOX, IMMEDIATE], MAILBOX),
		(vec![IMMEDIATE, FIFO_RELAXED, FIFO], FIFO),
		(vec![IMMEDIATE], FIFO),
		(vec![], FIFO),
	];
	for (modes, mode) in cases {
		let made = plan(&wayland(), &[BGRA], &modes, [10, 10]);
		assert_eq!(made.unwrap().mode, mode, "{modes:?}");
	}
}

#[test]
fn opaque_alpha_is_preferred_then_inherited_then_multiplied() {
	let cases = [
		(OPAQUE | INHERIT | PRE_MULTIPLIED, OPAQUE),
		(INHERIT | PRE_MULTIPLIED | POST_MULTIPLIED, INHERIT),
		(POST_MULTIPLIED | PRE_MULTIPLIED, PRE_MULTIPLIED),
		(POST_MULTIPLIED, POST_MULTIPLIED),
	];
	for (supported, alpha) in cases {
		let mut capabilities = wayland();
		capabilities.alpha = supported;
		let made = planned(&capabilities, [10, 10]);
		assert_eq!(made.unwrap().alpha, alpha, "{supported:#x}");
	}
	let mut capabilities = wayland();
	capabilities.alpha = 0;
	assert_eq!(planned(&capabilities, [10, 10]), Err(Unfit::Alpha));
}

#[test]
fn images_must_take_both_drawing_and_copies() {
	for usage in [0x10, 0x2, 0x1 | 0x4 | 0x8] {
		let mut capabilities = wayland();
		capabilities.usage = usage;
		assert_eq!(planned(&capabilities, [10, 10]), Err(Unfit::Usage));
	}
	let mut capabilities = wayland();
	capabilities.usage = USAGE;
	assert!(planned(&capabilities, [10, 10]).is_ok());
}

#[test]
fn the_surfaces_current_transform_is_kept() {
	// Rotating is the compositor's work; asking for another transform
	// would make it rotate twice.
	let mut capabilities = wayland();
	capabilities.current_transform = ROTATE_90;
	assert_eq!(
		planned(&capabilities, [10, 10]).unwrap().transform,
		ROTATE_90
	);
}

#[test]
fn a_hidden_window_is_reported_before_what_the_surface_lacks() {
	// A minimized window says nothing about the GPU: it must not make the
	// app give up on it.
	let mut capabilities = x11([0, 0]);
	capabilities.usage = 0;
	capabilities.alpha = 0;
	assert_eq!(plan(&capabilities, &[], &[], [0, 0]), Err(Unfit::Hidden));
}

#[test]
fn a_driver_reporting_impossible_limits_gets_a_plan_not_a_panic() {
	// The driver makes the swapchain refuse these, which falls back to
	// software; choosing must not crash first.
	let mut capabilities = wayland();
	capabilities.min_extent = [500, 500];
	capabilities.max_extent = [100, 100];
	assert_eq!(
		planned(&capabilities, [300, 300]).unwrap().extent,
		[100, 100]
	);
	capabilities.min_images = u32::MAX;
	assert_eq!(planned(&capabilities, [300, 300]).unwrap().images, u32::MAX);
}
