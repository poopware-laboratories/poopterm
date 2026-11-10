//! Tests of the regions a GPU window names as it presents: only on a
//! device made with `VK_KHR_incremental_present`, never on a swapchain's
//! first image, and only for the attempt the pen's bands were meant for.
use super::{
	driver_tests::{Driver, with},
	present_driver_tests::OUT_OF_DATE,
	present_regions::RectLayer,
	window::{Drawn, GpuWindow},
	window_tests::{SIZE, close, open, pen, showing},
};

fn incremental() -> Driver {
	let mut driver = showing();
	driver.device_extensions = vec!["VK_KHR_incremental_present"];
	driver
}

/// Draws a frame `size` large whose pen says `changed` pixel rows changed.
fn draw_changed(
	window: &mut GpuWindow,
	size: [u32; 2],
	changed: Option<Vec<(usize, usize)>>,
) -> Drawn {
	let mut pen = pen(size);
	pen.changed = changed;
	window.draw(&mut pen, size).unwrap()
}

/// The regions each present named so far.
fn named() -> Vec<Vec<RectLayer>> {
	with(|driver| driver.present_regions.clone())
}

fn band(y: i32, height: u32) -> RectLayer {
	RectLayer {
		offset: [0, y],
		extent: [SIZE[0], height],
		layer: 0,
	}
}

#[test]
fn a_device_offering_it_is_made_with_it() {
	let window = open(incremental(), false).unwrap();
	let made = with(|driver| driver.made.last().unwrap().extensions.clone());
	assert_eq!(made, ["VK_KHR_swapchain", "VK_KHR_incremental_present"]);
	close(window);
}

#[test]
fn after_the_first_image_presents_name_what_changed() {
	let mut window = open(incremental(), false).unwrap();
	// The swapchain's first image is new whole, whatever the pen says.
	draw_changed(&mut window, SIZE, Some(vec![(0, 4)]));
	draw_changed(&mut window, SIZE, Some(vec![(0, 4), (20, 30)]));
	// Unknown changes name nothing: the whole image.
	draw_changed(&mut window, SIZE, None);
	assert_eq!(named(), [vec![], vec![band(0, 4), band(20, 10)], vec![]]);
	close(window);
}

#[test]
fn a_device_without_it_presents_whole_images() {
	let mut window = open(showing(), false).unwrap();
	for _ in 0..3 {
		draw_changed(&mut window, SIZE, Some(vec![(0, 4)]));
	}
	assert_eq!(named(), [vec![], vec![], vec![]]);
	let made = with(|driver| driver.made.last().unwrap().extensions.clone());
	assert_eq!(made, ["VK_KHR_swapchain"]);
	close(window);
}

#[test]
fn a_remade_swapchain_starts_whole_again() {
	let mut window = open(incremental(), false).unwrap();
	draw_changed(&mut window, SIZE, None);
	draw_changed(&mut window, [64, 48], Some(vec![(0, 4)]));
	let after = draw_changed(&mut window, [64, 48], Some(vec![(0, 4)]));
	assert_eq!(after, Drawn::Shown);
	let wide = RectLayer {
		offset: [0, 0],
		extent: [64, 4],
		layer: 0,
	};
	assert_eq!(named(), [vec![], vec![], vec![wide]]);
	close(window);
}

#[test]
fn bands_are_cut_to_the_image_the_window_now_is() {
	let mut window = open(incremental(), false).unwrap();
	draw_changed(&mut window, SIZE, None);
	draw_changed(&mut window, SIZE, Some(vec![(25, 99), (500, 600)]));
	assert_eq!(named()[1], [band(25, SIZE[1] - 25)]);
	close(window);
}

#[test]
fn a_frame_drawn_again_after_out_of_date_is_whole() {
	let mut window = open(incremental(), false).unwrap();
	draw_changed(&mut window, SIZE, None);
	with(|driver| driver.present_answers = vec![OUT_OF_DATE]);
	let drawn = draw_changed(&mut window, SIZE, Some(vec![(0, 4)]));
	assert_eq!(drawn, Drawn::Again);
	// The retry knows nothing of the changes, nor does the new swapchain.
	draw_changed(&mut window, SIZE, None);
	draw_changed(&mut window, SIZE, Some(vec![(0, 4)]));
	assert_eq!(
		named(),
		[vec![], vec![band(0, 4)], vec![], vec![band(0, 4)]]
	);
	close(window);
}
