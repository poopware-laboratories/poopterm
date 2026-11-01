//! The main window's frames drawn by a real GPU on a headless window: every
//! grid scene over chrome software painted, as quads alone, under an
//! overlay, with images left to software, and through resizes, each shown.
use super::{Gpu, Layers, Next};
use crate::{
	gpu::{quads::QuadPen, vk::outside_tests::headless_window},
	render::{
		glyphs::FontSet,
		grid::draw_grid,
		grid_case_tests::{BG, Case, SIZE, cases, direct, fonts},
		pen::Pen,
	},
};

/// What painting the frame needs: the fonts, and the scene.
struct Scene {
	fonts: FontSet,
	case: Case,
}

/// A gradient around the grid, as chrome and wallpaper would be, or with
/// no backdrop a quad of the background under it.
fn around(scene: &mut Scene, pixels: Option<&mut [u32]>, pen: &mut QuadPen) {
	match pixels {
		Some(pixels) => {
			for (i, pixel) in pixels.iter_mut().enumerate() {
				*pixel = (i as u32 * 7) & 0xff_ffff;
			}
		}
		None => {
			let (width, height) = pen.size();
			pen.rect(0, 0, width, height, BG);
		}
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
		let layers = [Layers::OverBackdrop, Layers::Whole, Layers::Quads];
		for (size, layers) in sizes.into_iter().zip(layers) {
			let mut pixels = vec![BG; size.0 * size.1];
			let next = gpu.frame(&mut scene, (size, &mut pixels), layers, around, whole);
			shown.push((name.clone(), size, layers, next));
		}
		let mut pixels = vec![BG; SIZE.0 * SIZE.1];
		let next = gpu.frame(
			&mut scene,
			(SIZE, &mut pixels),
			Layers::Quads,
			around,
			whole,
		);
		shown.push((name.clone(), SIZE, Layers::Quads, next));
	}
	let unshown: Vec<_> = shown
		.iter()
		.filter(|(.., next)| *next != Next::Done)
		.collect();
	assert!(unshown.is_empty(), "{unshown:?}");
}
