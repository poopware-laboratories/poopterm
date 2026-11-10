//! Tests of the glyph atlas kept within the memory budget, against the fake
//! driver of `driver_tests`: on a device with `VK_EXT_memory_budget`, the
//! atlas grows only into half of what its heap has free; elsewhere, and
//! without the loader's function, up to the fixed cap as before.
use super::{
	call::Error,
	driver_tests::{Driver, finish, install, with},
	renderer_tests::glyph,
	window::{Drawn, GpuWindow},
	window_tests::{ATLAS, SIZE, close, open, showing},
};
use crate::{
	gpu::quads::QuadPen,
	render::pen::{Ink, Pen},
};

const KB: u64 = 1 << 10;

/// A machine whose heap 0, where atlases go, has `budget` and `used`
/// bytes, offering the extension when `offered`.
fn budgeted(offered: bool, budget: u64, used: u64) -> Driver {
	let mut driver = showing();
	if offered {
		driver.device_extensions = vec!["VK_EXT_memory_budget"];
	}
	driver.heap_budgets = vec![(budget, used)];
	driver
}

/// Draws three glyphs of 40 × 40 pixels: two more pages than the 64-pixel
/// atlas of two pages (32 KB) holds, so it would grow to four (64 KB).
fn three_glyphs(window: &mut GpuWindow) -> Result<Drawn, Error> {
	let mut pen = QuadPen::new((40, 30), ATLAS);
	pen.begin_frame((40, 30));
	for ch in "abc".chars() {
		let ink = Ink::Drawn(ch, (40, 40));
		pen.glyph(&glyph(40, 40, false), ink, (0, 0), (0, 0, 40, 30), false, 0);
	}
	window.draw(&mut pen, SIZE)
}

fn images() -> usize {
	with(|driver| driver.images.len())
}

#[test]
fn an_atlas_grows_into_half_of_what_its_heap_has_free() {
	// 200 KB free: up to 116 KB, so four pages of 16 KB fit.
	let mut window = open(budgeted(true, 300 * KB, 100 * KB), false).unwrap();
	assert_eq!(three_glyphs(&mut window), Ok(Drawn::Again));
	let made = with(|driver| {
		let atlas = driver.images.last().unwrap();
		(atlas.extent, atlas.layers)
	});
	assert_eq!(made, ([64, 64, 1], 4));
	close(window);
}

#[test]
fn an_atlas_that_would_take_more_than_half_of_what_is_free_refuses_the_frame() {
	// 50 KB free: up to 41 KB, short of the 64 KB four pages take.
	let mut window = open(budgeted(true, 100 * KB, 50 * KB), false).unwrap();
	let before = images();
	assert_eq!(three_glyphs(&mut window), Err(Error::Draw));
	assert_eq!(images(), before, "no atlas made");
	close(window);
}

#[test]
fn a_heap_used_past_its_budget_leaves_no_room_to_grow() {
	let mut window = open(budgeted(true, 100 * KB, 400 * KB), false).unwrap();
	assert_eq!(three_glyphs(&mut window), Err(Error::Draw));
	close(window);
}

#[test]
fn a_device_without_the_extension_grows_to_the_fixed_cap_as_before() {
	// The same tight heap, unknown to the renderer: it grows, and the fake
	// would report a budget asked of a device not offering one.
	let mut window = open(budgeted(false, 100 * KB, 50 * KB), false).unwrap();
	assert_eq!(three_glyphs(&mut window), Ok(Drawn::Again));
	close(window);
}

#[test]
fn a_loader_without_the_function_grows_to_the_fixed_cap() {
	let mut driver = budgeted(true, 100 * KB, 50 * KB);
	driver.lacks = vec![c"vkGetPhysicalDeviceMemoryProperties2"];
	let mut window = open(driver, false).unwrap();
	assert_eq!(three_glyphs(&mut window), Ok(Drawn::Again));
	close(window);
}

#[test]
fn a_heap_without_a_budget_is_not_held_to_one() {
	// The driver filled in nothing for the heap: no budget, the fixed cap.
	let mut window = open(budgeted(true, 0, 0), false).unwrap();
	assert_eq!(three_glyphs(&mut window), Ok(Drawn::Again));
	close(window);
}

#[test]
fn the_free_memory_of_a_heap_is_its_budget_less_its_usage() {
	let entry = install(budgeted(true, 900 * KB, 300 * KB));
	let instance = entry.create_instance(c"poopterm").unwrap();
	let physical = instance.physical_devices().unwrap()[0];
	assert_eq!(instance.heap_free(physical, 0), Some(600 * KB));
	assert_eq!(instance.heap_free(physical, 3), None);
	drop(instance);
	finish();
}

#[test]
fn the_fake_driver_refuses_a_budget_the_device_does_not_offer() {
	// Were it blind to it, the tests above would prove nothing.
	let entry = install(budgeted(false, 900 * KB, 300 * KB));
	let instance = entry.create_instance(c"poopterm").unwrap();
	let physical = instance.physical_devices().unwrap()[0];
	instance.heap_free(physical, 0);
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	let rule = "vkGetPhysicalDeviceMemoryProperties2: a budget the device offers";
	assert_eq!(invalid, [rule]);
	drop(instance);
	finish();
}

/// NVIDIA's memory without resizable BAR: video memory on heap 0, where
/// atlases go, system memory on 1, the BAR on 2; with these budgets.
fn nvidia(budgets: [(u64, u64); 3]) -> Driver {
	use super::memory::{DEVICE_LOCAL, HOST_COHERENT, HOST_VISIBLE, MemoryType};
	let host = HOST_VISIBLE | HOST_COHERENT;
	let mut driver = budgeted(true, 0, 0);
	driver.memory = [(DEVICE_LOCAL, 0), (host, 1), (DEVICE_LOCAL | host, 2)]
		.map(|(flags, heap)| MemoryType { flags, heap })
		.to_vec();
	driver.heap_budgets = budgets.to_vec();
	driver
}

#[test]
fn the_budget_is_that_of_the_heap_the_atlas_goes_to() {
	let (tight, roomy) = ((100 * KB, 50 * KB), (8 << 30, 0));
	// Video memory tight, the rest roomy: refused.
	let mut window = open(nvidia([tight, roomy, roomy]), false).unwrap();
	assert_eq!(three_glyphs(&mut window), Err(Error::Draw));
	close(window);
	// Video memory roomy, the rest tight: grown.
	let mut window = open(nvidia([roomy, tight, tight]), false).unwrap();
	assert_eq!(three_glyphs(&mut window), Ok(Drawn::Again));
	close(window);
}
