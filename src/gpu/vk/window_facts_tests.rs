//! Tests of what a GPU window says of itself on the Diagnostics page: its
//! formats by their specification names, its window system, and its
//! swapchain as made.
use super::{
	image::Format,
	surface::Platform,
	swapchain_plan::{FIFO, MAILBOX, OPAQUE, Plan},
};
use crate::gpu::facts::Swapchain;

#[test]
fn formats_go_by_their_specification_names() {
	assert_eq!(Format::Bgra.name(), "B8G8R8A8_UNORM");
	assert_eq!(Format::Rgba.name(), "R8G8B8A8_UNORM");
}

#[test]
fn each_window_system_names_the_presenter() {
	assert_eq!(Platform::Wayland.presenter(), "Vulkan on Wayland");
	assert_eq!(Platform::Xlib.presenter(), "Vulkan on X11");
	assert_eq!(Platform::Headless.presenter(), "Vulkan, headless");
}

fn plan(format: Format, mode: i32, images: u32) -> Plan {
	Plan {
		format,
		mode,
		extent: [800, 600],
		images,
		transform: 1,
		alpha: OPAQUE,
	}
}

#[test]
fn a_swapchain_counts_the_images_it_got_not_the_ones_it_asked_for() {
	// Drivers may make more images than the plan's minimum.
	let facts = plan(Format::Bgra, FIFO, 3).facts(4);
	assert_eq!(
		facts,
		Swapchain {
			format: "B8G8R8A8_UNORM",
			mode: FIFO,
			images: 4,
		}
	);
}

#[test]
fn a_swapchain_keeps_its_format_and_mode() {
	let facts = plan(Format::Rgba, MAILBOX, 2).facts(2);
	assert_eq!(
		(facts.format, facts.mode, facts.images),
		("R8G8B8A8_UNORM", MAILBOX, 2)
	);
	// More images than a u32 holds cannot be, but stay bounded.
	assert_eq!(
		plan(Format::Bgra, FIFO, 2).facts(usize::MAX).images,
		u32::MAX
	);
}

#[test]
fn a_window_says_what_it_draws_on_and_its_swapchain_once_made() {
	use super::{
		version::api_version,
		window_tests::{SIZE, close, draw, open, showing},
	};
	let mut driver = showing();
	driver.driver_info = Some(("Intel open-source Mesa driver", "Mesa 26.2.1"));
	let mut window = open(driver, false).unwrap();
	let facts = window.facts();
	assert_eq!(
		(facts.device.as_str(), facts.api, facts.version),
		("Integrated", "Vulkan", Some(api_version(1, 3)))
	);
	assert_eq!(facts.driver.as_deref(), Some("Mesa 26.2.1"));
	assert_eq!(facts.presenter, "Vulkan on Wayland");
	// No swapchain before the first frame; the app adds the atlas.
	assert_eq!((facts.swapchain, facts.atlas), (None, None));
	draw(&mut window, SIZE).unwrap();
	let swapchain = window.facts().swapchain.unwrap();
	assert_eq!(swapchain.format, "B8G8R8A8_UNORM");
	let made = super::driver_tests::with(|driver| driver.swapchains[0].images);
	assert_eq!(swapchain.images, made);
	assert_eq!(window.device_name(), "Integrated");
	close(window);
}

#[test]
fn a_driver_without_facts_leaves_them_unknown() {
	use super::window_tests::{close, open, showing};
	let mut driver = showing();
	driver.lacks.push(c"vkGetPhysicalDeviceProperties2");
	let window = open(driver, false).unwrap();
	assert_eq!(window.facts().driver, None);
	assert!(window.facts().version.is_some());
	close(window);
}
