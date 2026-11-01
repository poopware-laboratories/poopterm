//! The main window's frames drawn by a real GPU on a headless window: every
//! grid scene as quads, images included, and through resizes, each shown.
use super::{Gpu, Next};
use crate::{
	gpu::{quads::QuadPen, vk::outside_tests::headless_window},
	render::{
		glyphs::FontSet,
		grid::draw_grid,
		grid_case_tests::{BG, Case, SIZE, cases, fonts},
		pen::Pen,
	},
};

const ATLAS: (usize, usize) = (256, 4);

/// What drawing the frame needs: the fonts, and the scene.
struct Scene {
	fonts: FontSet,
	case: Case,
}

/// A quad of the background under the grid.
fn quads(scene: &mut Scene, pen: &mut QuadPen) {
	let (width, height) = pen.size();
	pen.rect(0, 0, width, height, BG);
	draw_grid(pen, &mut scene.fonts, &scene.case.view(), None);
}

#[test]
fn every_scene_drawn_by_a_real_gpu_is_shown_as_quads() {
	let Some(window) = headless_window(ATLAS) else {
		return;
	};
	let mut gpu = Gpu::with(window, (), ATLAS);
	let mut shown = Vec::new();
	for case in cases() {
		let name = case.name.clone();
		let mut scene = Scene {
			fonts: fonts(),
			case,
		};
		for size in [SIZE, (SIZE.0 / 2, SIZE.1 / 3), (1, 1), SIZE] {
			let next = gpu.frame(size, |pen| quads(&mut scene, pen));
			shown.push((name.clone(), size, next));
		}
	}
	let unshown: Vec<_> = shown
		.iter()
		.filter(|(.., next)| *next != Next::Done)
		.collect();
	assert!(unshown.is_empty(), "{unshown:?}");
}
