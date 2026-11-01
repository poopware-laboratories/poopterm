//! Tests of drawing a scene, such as a page of the Settings window, on the
//! GPU: as quads, drawn again after the atlas grew when its glyphs did not
//! fit; on the fake Vulkan driver and on a real GPU.
use super::{Gpu, Next};
use crate::{
	gpu::vk::outside_tests::{
		FAKE_ATLAS, fake_close as close, fake_presents as presents, fake_recorded as recorded,
		fake_window, headless_window,
	},
	render::{grid_case_tests::fonts, painter::ButtonStyle, scene::Scene},
};

const SIZE: (usize, usize) = (160, 120);
const LABEL: u32 = 0xee_eeee;

/// A page of the Settings window `size` pixels large at `scale`: its border,
/// a rounded field, a button lit and one not, and a label.
fn page(size: (usize, usize), scale: usize) -> Scene {
	let mut scene = Scene::new(size.0, size.1);
	let style = ButtonStyle {
		scale,
		background: 0x1e_1e2e,
		border: 0x90_90a0,
		accent: 0x30_60ff,
	};
	scene.fill(style.border);
	let inside = (
		size.0.saturating_sub(4 * scale),
		size.1.saturating_sub(4 * scale),
	);
	scene.rect(2 * scale, 2 * scale, inside.0, inside.1, style.background);
	scene.round_rect(8, 40, 100, 24, 6 * scale, 0x2a_2a3a);
	scene.button((8, 70, 60, 24), style, false);
	scene.button((76, 70, 60, 24), style, true);
	let px = 18.0 * scale as f32;
	scene.text(&mut fonts(), 8, 8, "Settings", px, 14 * scale as i32, LABEL);
	scene
}

/// The page with a label too large for an atlas page: 2000 pixels, as a
/// huge scale factor makes.
fn overflowing() -> Scene {
	let mut scene = page(SIZE, 1);
	scene.text(&mut fonts(), 0, 0, "W", 2000.0, 1500, LABEL);
	scene
}

/// Draws `scene` on `gpu`, saying what it came to.
fn draw<W>(gpu: &mut Gpu<W>, scene: &Scene) -> Next {
	gpu.scene(scene, &mut fonts())
}

/// Draws `scene` until it is not to be drawn again: what it came to, and
/// how many times it was drawn again.
fn drawn<W>(gpu: &mut Gpu<W>, scene: &Scene) -> (Next, usize) {
	for again in 0..8 {
		match draw(gpu, scene) {
			Next::Redraw => {}
			next => return (next, again),
		}
	}
	panic!("drawn again forever");
}

#[test]
fn a_scene_is_drawn_as_quads_and_shown_without_a_backdrop() {
	let mut gpu = Gpu::with(fake_window(), "settings", FAKE_ATLAS);
	assert_eq!(draw(&mut gpu, &page(SIZE, 1)), Next::Done);
	let calls = recorded();
	let (uploads, draws) = calls.split_at(calls.len() - 1);
	// Glyphs and corners to the atlas, never the whole frame.
	assert!(!uploads.is_empty(), "{calls:?}");
	let backdrop = format!("size {}x{}", SIZE.0, SIZE.1);
	assert!(
		uploads.iter().all(|call| !call.contains(&backdrop)),
		"{calls:?}"
	);
	assert!(draws[0].starts_with("draw 6 vertices of "), "{calls:?}");
	assert_eq!(presents(), 1);
	// What the atlas holds is not sent again.
	assert_eq!(draw(&mut gpu, &page(SIZE, 1)), Next::Done);
	assert_eq!(recorded(), draws);
	close(gpu.gpu);
}

#[test]
fn a_scene_with_a_glyph_wider_than_a_page_is_drawn_again_once_the_atlas_grew() {
	let mut gpu = Gpu::with(fake_window(), "settings", FAKE_ATLAS);
	assert_eq!(draw(&mut gpu, &overflowing()), Next::Redraw);
	let calls = recorded();
	assert!(
		calls.iter().all(|call| !call.starts_with("draw")),
		"{calls:?}"
	);
	assert_eq!(presents(), 0);
	// Drawn whole, as quads, on the GPU.
	assert_eq!(draw(&mut gpu, &overflowing()), Next::Done);
	assert!(
		recorded()
			.last()
			.unwrap()
			.starts_with("draw 6 vertices of ")
	);
	assert_eq!(presents(), 1);
	close(gpu.gpu);
}

#[test]
fn the_pen_fills_the_atlas_of_its_window_and_grows_with_it() {
	// Glyphs that each fit an atlas page of the fake window, but not all
	// together: a pen sized for a larger atlas would place some outside it.
	let mut scene = page(SIZE, 1);
	for (row, px) in [24.0, 28.0, 32.0].into_iter().enumerate() {
		scene.text(
			&mut fonts(),
			0,
			30 * row,
			"ABCDEFGHIJKLMNOPQRSTUVWXYZ",
			px,
			22,
			LABEL,
		);
	}
	let mut gpu = Gpu::with(fake_window(), "settings", FAKE_ATLAS);
	// Pages double until they hold every letter: nothing is placed past
	// them, which the driver would refuse.
	let (next, again) = drawn(&mut gpu, &scene);
	assert_eq!(next, Next::Done);
	assert!(again >= 1, "the atlas grew");
	assert_eq!(draw(&mut gpu, &page(SIZE, 1)), Next::Done);
	close(gpu.gpu);
}

#[test]
fn a_scene_of_a_window_without_area_shows_nothing() {
	let mut gpu = Gpu::with(fake_window(), "settings", FAKE_ATLAS);
	for size in [(0, 0), (SIZE.0, 0), (0, SIZE.1)] {
		assert_eq!(draw(&mut gpu, &page(size, 1)), Next::Done, "{size:?}");
	}
	assert_eq!(presents(), 0);
	close(gpu.gpu);
}

#[test]
fn every_scene_drawn_by_a_real_gpu_is_shown_at_every_scale_and_size() {
	let Some(window) = headless_window(super::ATLAS) else {
		return;
	};
	let mut gpu = Gpu::with(window, (), super::ATLAS);
	let mut shown = Vec::new();
	for scale in [1, 2, 3] {
		for size in [SIZE, (SIZE.0 * 2, SIZE.1 * 3), (1, 1), SIZE] {
			shown.push(((scale, size), draw(&mut gpu, &page(size, scale))));
		}
	}
	// A glyph wider than a page: drawn again once, on a larger atlas.
	shown.push(((1, SIZE), drawn(&mut gpu, &overflowing()).0));
	let unshown: Vec<_> = shown
		.iter()
		.filter(|(_, next)| *next != Next::Done)
		.collect();
	assert!(unshown.is_empty(), "{unshown:?}");
}
