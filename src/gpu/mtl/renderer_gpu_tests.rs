//! The grid drawn by Metal: every shared grid scene of
//! `gpu::reference_tests`, drawn as quads by the renderer and read back
//! from the real GPU, matches what the software renderer paints.
use super::{
	commands::tests::synchronize,
	device::{Device, Error, tests::device},
	renderer::Renderer,
	texture::{Texture, Usage, tests::download},
};
use crate::render::grid_case_tests::SIZE;
use std::rc::Rc;

/// The atlas the pen and the renderer share: four pages 256 pixels square.
const ATLAS: (usize, usize) = (256, 4);

/// Draws every scene on `device` and says which differ from software, and
/// by how many pixels.
fn unlike_software(device: &Rc<Device>) -> Result<Vec<String>, Error> {
	let mut renderer = Renderer::new(device, ATLAS)?;
	let size = [SIZE.0 as u32, SIZE.1 as u32, 1];
	let target = Texture::new(device, size, false, Usage::Target)?;
	crate::gpu::reference_tests::unlike_software(ATLAS, |pen| {
		renderer.frame(pen, &target, |frame| synchronize(frame, &target))?;
		renderer.finish()?;
		Ok(download(&target))
	})
}

#[test]
fn every_grid_scene_drawn_by_metal_paints_what_software_paints() {
	let Some(device) = device() else { return };
	assert_eq!(unlike_software(&device), Ok(vec![]));
}
