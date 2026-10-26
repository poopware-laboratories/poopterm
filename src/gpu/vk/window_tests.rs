//! Tests of a window drawn by the GPU against the fake driver of
//! `driver_tests`: the device chosen for its surface or why none was, the
//! swapchain made at the window's size and remade only when the window or
//! the presentation engine asks, and frames that cannot be shown skipped.
use super::{
	buffer_tests::INTEL,
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind, Unsuited},
	driver_tests::{Driver, finish, install, with, with_devices},
	memory::MemoryType,
	present_driver_tests::{OUT_OF_DATE, SUBOPTIMAL, SURFACE_LOST, TIMEOUT},
	renderer_tests::glyph,
	surface_driver_tests::{WAYLAND, gpu, present},
	surface_query_tests::mesa_wayland,
	swapchain_plan::{Capabilities, SurfaceFormat, Unfit},
	window::{ACQUIRE_WAIT, Drawn, GpuWindow},
};
use crate::{
	gpu::quads::QuadPen,
	graphics::{Bitmap, Placement, Rect},
	render::pen::{Ink, Pen},
};
use std::sync::Arc;

pub(super) const ATLAS: (usize, usize) = (64, 2);
pub(super) const SIZE: [u32; 2] = [40, 30];

pub(super) fn showing() -> Driver {
	machine(&[gpu(Kind::Integrated, vec![present(GRAPHICS, true)])])
}

/// A driver with these devices, each with Intel's memory types.
pub(super) fn machine(devices: &[Described]) -> Driver {
	let mut driver = with_devices(devices);
	driver.memory = (INTEL.iter())
		.map(|&flags| MemoryType { flags, heap: 0 })
		.collect();
	driver
}

pub(super) fn open(driver: Driver, software: bool) -> Result<GpuWindow, Error> {
	// SAFETY: the fake driver only records the window's handles.
	unsafe { GpuWindow::new(install(driver), WAYLAND, software, ATLAS) }
}

/// A frame of one rectangle, `size` pixels large.
pub(super) fn pen(size: [u32; 2]) -> QuadPen {
	let size = (size[0] as usize, size[1] as usize);
	let mut pen = QuadPen::new(size, ATLAS);
	pen.begin_frame(size);
	pen.rect(0, 0, 4, 3, 0x20_4060);
	pen
}

pub(super) fn draw(window: &mut GpuWindow, size: [u32; 2]) -> Result<Drawn, Error> {
	window.draw(&mut pen(size), size)
}

/// Each swapchain made so far: its size and whether it retired another.
pub(super) fn swapchains() -> Vec<([u32; 2], bool)> {
	with(|driver| {
		(driver.swapchains.iter())
			.map(|made| (made.extent, made.old != 0))
			.collect()
	})
}

fn presents() -> usize {
	with(|driver| driver.presents.len())
}

pub(super) fn close(window: GpuWindow) -> Driver {
	drop(window);
	let driver = finish();
	assert!(driver.live_swapchains.is_empty());
	assert!(driver.live_surfaces.is_empty());
	assert!(driver.live_devices.is_empty());
	assert!(driver.live.is_empty());
	driver
}

#[test]
fn the_device_that_shows_in_the_window_is_chosen() {
	let hidden = gpu(Kind::Integrated, vec![present(GRAPHICS, false)]);
	let shows = gpu(Kind::Discrete, vec![present(GRAPHICS, true)]);
	let window = open(machine(&[hidden, shows]), false).unwrap();
	assert_eq!(window.device_name(), "Discrete");
	// Nothing is made until there is a frame to show.
	assert!(swapchains().is_empty());
	let driver = close(window);
	assert_eq!(driver.made.len(), 1);
}

#[test]
fn no_device_says_why_each_one_cannot_draw_and_leaves_nothing() {
	let cpu = gpu(Kind::Cpu, vec![present(GRAPHICS, true)]);
	let hidden = gpu(Kind::Integrated, vec![present(GRAPHICS, false)]);
	let error = open(machine(&[cpu.clone(), hidden.clone()]), false).err();
	let why = vec![
		("Cpu".to_owned(), Unsuited::Cpu),
		("Integrated".to_owned(), Unsuited::NoQueue),
	];
	assert_eq!(error, Some(Error::NoDevice(why)));
	let driver = finish();
	assert!(driver.made.is_empty() && driver.live.is_empty());
	assert!(driver.live_surfaces.is_empty());
	// Asked for software, the CPU device draws.
	let window = open(machine(&[cpu, hidden]), true).unwrap();
	assert_eq!(window.device_name(), "Cpu");
	close(window);
	assert_eq!(
		open(machine(&[]), false).err(),
		Some(Error::NoDevice(Vec::new()))
	);
	finish();
}

#[test]
fn frames_of_one_size_share_a_swapchain_made_at_that_size() {
	let mut window = open(showing(), false).unwrap();
	for _ in 0..5 {
		assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	}
	assert_eq!(swapchains(), [(SIZE, false)]);
	assert_eq!(presents(), 5);
	// Each frame drew its rectangle, waiting a bounded time for an image.
	let drawn = with(|driver| {
		(driver.recorded.iter())
			.filter(|line| line.starts_with("draw 6 vertices of 1 instances"))
			.count()
	});
	assert_eq!(drawn, 5);
	let waits = with(|driver| driver.acquire_timeouts.clone());
	assert_eq!(waits, [ACQUIRE_WAIT.as_nanos() as u64; 5]);
	close(window);
}

#[test]
fn a_resize_remakes_the_swapchain_from_the_old_one() {
	let mut window = open(showing(), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	assert_eq!(draw(&mut window, [64, 48]), Ok(Drawn::Shown));
	draw(&mut window, [64, 48]).unwrap();
	assert_eq!(swapchains(), [(SIZE, false), ([64, 48], true)]);
	assert_eq!(presents(), 3);
	close(window);
}

#[test]
fn a_window_without_area_skips_frames_and_keeps_its_swapchain() {
	let mut window = open(showing(), false).unwrap();
	// Minimized before the first frame: nothing to make yet.
	assert_eq!(draw(&mut window, [0, 30]), Ok(Drawn::Skipped));
	assert!(swapchains().is_empty());
	draw(&mut window, SIZE).unwrap();
	assert_eq!(draw(&mut window, [40, 0]), Ok(Drawn::Skipped));
	// So is a surface that reports no area for a window that has some.
	with(|driver| {
		driver.surface_capabilities = Some(Capabilities {
			current_extent: [0, 0],
			..mesa_wayland()
		})
	});
	assert_eq!(draw(&mut window, [64, 48]), Ok(Drawn::Skipped));
	with(|driver| driver.surface_capabilities = None);
	// Restored to its size, it shows on the swapchain it had.
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	assert_eq!(swapchains(), [(SIZE, false)]);
	assert_eq!(presents(), 2);
	close(window);
}

#[test]
fn an_out_of_date_swapchain_skips_the_frame_and_is_remade_for_the_next() {
	let mut window = open(showing(), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	with(|driver| driver.acquire_answers = vec![OUT_OF_DATE]);
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Skipped));
	assert_eq!(swapchains().len(), 1);
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	assert_eq!(swapchains(), [(SIZE, false), (SIZE, true)]);
	// Out of date when presented: not shown, and remade as well.
	with(|driver| driver.present_answers = vec![OUT_OF_DATE]);
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Skipped));
	draw(&mut window, SIZE).unwrap();
	assert_eq!(swapchains().len(), 3);
	// Four asked to be shown, one of them refused.
	assert_eq!(presents(), 4);
	close(window);
}

#[test]
fn a_suboptimal_swapchain_shows_the_frame_and_is_remade_after() {
	for answers in [(vec![SUBOPTIMAL], vec![]), (vec![], vec![SUBOPTIMAL])] {
		let mut window = open(showing(), false).unwrap();
		draw(&mut window, SIZE).unwrap();
		with(|driver| (driver.acquire_answers, driver.present_answers) = answers.clone());
		assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
		assert_eq!(swapchains().len(), 1, "{answers:?}");
		draw(&mut window, SIZE).unwrap();
		assert_eq!(swapchains().len(), 2, "{answers:?}");
		close(window);
	}
}

#[test]
fn no_image_coming_free_in_time_skips_the_frame_only() {
	let mut window = open(showing(), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	with(|driver| driver.acquire_answers = vec![TIMEOUT]);
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Skipped));
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	assert_eq!(swapchains().len(), 1);
	assert_eq!(presents(), 2);
	close(window);
}

#[test]
fn a_frame_the_gpu_cannot_draw_is_refused_before_taking_an_image() {
	let mut window = open(showing(), false).unwrap();
	let mut lacking = pen(SIZE);
	lacking.complete = false;
	assert_eq!(window.draw(&mut lacking, SIZE), Err(Error::Draw));
	assert!(with(|driver| driver.acquire_timeouts.is_empty()));
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	close(window);
}

#[test]
fn a_failed_frame_is_an_error_and_the_next_remakes_the_swapchain() {
	let mut window = open(showing(), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	// VK_ERROR_DEVICE_LOST, once.
	with(|driver| driver.fails = vec![(c"vkQueueSubmit2", -4)]);
	assert_eq!(draw(&mut window, SIZE), Err(Error::Failed(-4)));
	with(|driver| driver.fails.clear());
	// The image the failed frame took is never given back: start afresh.
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	assert_eq!(swapchains(), [(SIZE, false), (SIZE, true)]);
	close(window);
}

#[test]
fn a_swapchain_that_cannot_be_remade_is_made_anew_next_time() {
	let mut window = open(showing(), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	with(|driver| driver.fails = vec![(c"vkCreateSwapchainKHR", -1)]);
	assert_eq!(draw(&mut window, [64, 48]), Err(Error::Failed(-1)));
	with(|driver| driver.fails.clear());
	// Back at the size of the swapchain the failed remake retired.
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	// It starts from none.
	let made = swapchains();
	assert_eq!(made.last(), Some(&(SIZE, false)));
	close(window);
}

#[test]
fn a_surface_offering_only_rgba_is_refused_as_its_colors_would_swap() {
	let mut driver = showing();
	driver.surface_formats = Some(vec![SurfaceFormat {
		format: 37,
		color_space: 0,
	}]);
	let mut window = open(driver, false).unwrap();
	assert_eq!(draw(&mut window, SIZE), Err(Error::Unfit(Unfit::Format)));
	assert!(swapchains().is_empty());
	close(window);
}

#[test]
fn a_frame_with_images_is_refused_before_taking_an_image() {
	let mut window = open(showing(), false).unwrap();
	let mut pictured = pen(SIZE);
	let bitmap = Arc::new(Bitmap {
		width: 2,
		height: 2,
		opaque: true,
		pixels: vec![0; 4],
	});
	let whole = Rect {
		x: 0,
		y: 0,
		width: 2,
		height: 2,
	};
	let placement = Placement::natural(bitmap, 0, 0);
	pictured.placement(whole, &placement, (0, 0), (8, 16));
	assert_eq!(window.draw(&mut pictured, SIZE), Err(Error::Draw));
	assert!(with(|driver| driver.acquire_timeouts.is_empty()));
	close(window);
}

#[test]
fn a_surface_of_fixed_size_gets_one_swapchain_until_the_window_changes() {
	// As on X11: the surface is the window's size, which the window
	// system may know before the window does.
	let mut driver = showing();
	driver.surface_capabilities = Some(Capabilities {
		current_extent: [42, 31],
		..mesa_wayland()
	});
	let mut window = open(driver, false).unwrap();
	for _ in 0..3 {
		assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	}
	assert_eq!(swapchains(), [([42, 31], false)]);
	with(|driver| {
		driver.surface_capabilities = Some(Capabilities {
			current_extent: [64, 48],
			..mesa_wayland()
		})
	});
	draw(&mut window, [64, 48]).unwrap();
	draw(&mut window, [64, 48]).unwrap();
	assert_eq!(swapchains(), [([42, 31], false), ([64, 48], true)]);
	close(window);
}

#[test]
fn an_image_that_cannot_be_acquired_is_an_error_but_keeps_the_swapchain() {
	let mut window = open(showing(), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	// VK_ERROR_DEVICE_LOST: no image was taken, so none is lost.
	with(|driver| driver.acquire_answers = vec![-4]);
	assert_eq!(draw(&mut window, SIZE), Err(Error::Failed(-4)));
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	assert_eq!(swapchains(), [(SIZE, false)]);
	close(window);
}

#[test]
fn a_surface_lost_when_presenting_is_an_error_and_the_swapchain_is_remade() {
	let mut window = open(showing(), false).unwrap();
	draw(&mut window, SIZE).unwrap();
	with(|driver| driver.present_answers = vec![SURFACE_LOST]);
	assert_eq!(draw(&mut window, SIZE), Err(Error::Failed(SURFACE_LOST)));
	assert_eq!(draw(&mut window, SIZE), Ok(Drawn::Shown));
	assert_eq!(swapchains(), [(SIZE, false), (SIZE, true)]);
	close(window);
}

/// The glyph uploads recorded since the last call.
fn uploads() -> usize {
	with(|driver| {
		let uploads = driver
			.recorded
			.iter()
			.filter(|call| call.starts_with("upload"));
		let count = uploads.count();
		driver.recorded.clear();
		count
	})
}

#[test]
fn a_glyph_is_uploaded_once_and_again_after_a_failed_frame() {
	let mut window = open(showing(), false).unwrap();
	let mut pen = QuadPen::new((40, 30), ATLAS);
	let mut frame = |window: &mut GpuWindow| {
		pen.begin_frame((40, 30));
		let ink = Ink::Drawn('a', (3, 2));
		pen.glyph(&glyph(3, 2, false), ink, (1, 1), (0, 0, 40, 30), false, 0);
		window.draw(&mut pen, SIZE)
	};
	assert_eq!(frame(&mut window), Ok(Drawn::Shown));
	assert_eq!(uploads(), 1);
	assert_eq!(frame(&mut window), Ok(Drawn::Shown));
	assert_eq!(uploads(), 0, "the atlas keeps it");
	// A frame whose submission fails never uploads: the pen starts over,
	// so the next frame uploads the glyph again.
	with(|driver| driver.fails = vec![(c"vkQueueSubmit2", -4)]);
	assert_eq!(frame(&mut window), Err(Error::Failed(-4)));
	with(|driver| driver.fails.clear());
	uploads();
	assert_eq!(frame(&mut window), Ok(Drawn::Shown));
	assert_eq!(uploads(), 1);
	close(window);
}
