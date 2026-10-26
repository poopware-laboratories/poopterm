//! The main window's frames drawn by a real GPU on a headless window: every
//! grid scene over chrome software painted, under an overlay, with images
//! left to software, and through resizes, each shown.
use super::{Gpu, Next};
use crate::{
	gpu::{quads::QuadPen, vk::outside_tests::headless_window},
	render::{
		glyphs::FontSet,
		grid::draw_grid,
		grid_case_tests::{BG, Case, SIZE, cases, direct, fonts},
	},
};

/// What painting the frame needs: the fonts, and the scene.
struct Scene {
	fonts: FontSet,
	case: Case,
}

/// A gradient around the grid, as chrome and wallpaper would be.
fn around(scene: &mut Scene, pixels: &mut [u32], pen: &mut QuadPen) {
	for (i, pixel) in pixels.iter_mut().enumerate() {
		*pixel = (i as u32 * 7) & 0xff_ffff;
	}
	draw_grid(pen, &mut scene.fonts, &scene.case.view(), None);
}

fn whole(scene: &mut Scene, pixels: &mut [u32]) {
	let painted = direct(&scene.case.view());
	let shared = pixels.len().min(painted.len());
	pixels[..shared].copy_from_slice(&painted[..shared]);
}

#[test]
fn every_scene_drawn_by_a_real_gpu_is_shown_however_it_is_painted() {
	let Some(window) = headless_window((256, 4)) else {
		return;
	};
	let mut gpu = Gpu::with(window, ());
	let mut shown = Vec::new();
	for case in cases() {
		let name = case.name.clone();
		let mut scene = Scene {
			fonts: fonts(),
			case,
		};
		let sizes = [SIZE, (SIZE.0 / 2, SIZE.1 / 3), (1, 1)];
		for (size, overlay) in sizes.into_iter().zip([false, true, false]) {
			let mut pixels = vec![BG; size.0 * size.1];
			let next = gpu.frame(&mut scene, (size, &mut pixels), overlay, around, whole);
			shown.push((name.clone(), size, overlay, next));
		}
	}
	let unshown: Vec<_> = shown
		.iter()
		.filter(|(.., next)| *next != Next::Done)
		.collect();
	assert!(unshown.is_empty(), "{unshown:?}");
}
