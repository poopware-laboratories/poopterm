//! A window drawn by a real GPU, on a headless surface: the device chosen
//! for it shows every grid scene the GPU draws, through resizes and more
//! frames than it has images, and refuses the scene with images, for
//! software to draw. Run by the loader's tests and by its validation child.
use super::{
	call::Error,
	entry::Entry,
	loader_tests::{entry, required},
	surface::Window,
	window::{Drawn, GpuWindow},
};
use crate::{
	gpu::quads::QuadPen,
	render::{
		grid::draw_grid,
		grid_case_tests::{BG, SIZE, cases, fonts},
		pen::Pen,
	},
};

const ATLAS: (usize, usize) = (256, 4);

/// What drawing every grid scene in a headless window came to, at its
/// size, then half, then a pixel; then the last scene a dozen times; then
/// with the window minimized.
pub(super) fn headless_window(entry: Entry) -> Result<Vec<Result<Drawn, Error>>, Error> {
	// A CPU device is allowed: CI has no GPU.
	// SAFETY: a headless surface has no window handles to keep live.
	let mut window = unsafe { GpuWindow::new(entry, Window::Headless, true, ATLAS) }?;
	let (mut pen, mut fonts) = (QuadPen::new(SIZE, ATLAS), fonts());
	let (width, height) = SIZE;
	let mut drawn = Vec::new();
	let mut draw = |case: &crate::render::grid_case_tests::Case, size: [usize; 2]| {
		pen.begin_frame(SIZE);
		pen.rect(0, 0, width, height, BG);
		draw_grid(&mut pen, &mut fonts, &case.view(), None);
		drawn.push(window.draw(&mut pen, size.map(|n| n as u32)));
	};
	let cases = cases();
	for size in [[width, height], [width / 2, height / 2], [1, 1]] {
		cases.iter().for_each(|case| draw(case, size));
	}
	let shown = cases.iter().rfind(|case| case.frame.images.is_empty());
	for size in [[width, height]; 12].into_iter().chain([[0, height]]) {
		draw(shown.expect("a scene without images"), size);
	}
	Ok(drawn)
}

/// Whether every frame [`headless_window`] drew was shown but those with
/// images, refused, and the minimized one, skipped.
pub(super) fn as_expected(drawn: &[Result<Drawn, Error>]) -> bool {
	let cases = cases();
	let (scenes, rest) = drawn.split_at(cases.len() * 3);
	let scenes = scenes
		.iter()
		.zip(cases.iter().cycle())
		.all(|(drawn, case)| {
			let expected = match case.frame.images.is_empty() {
				true => Ok(Drawn::Shown),
				false => Err(Error::Draw),
			};
			*drawn == expected
		});
	let (minimized, repeated) = rest.split_last().expect("frames after the scenes");
	scenes
		&& repeated.len() == 12
		&& repeated.iter().all(|drawn| *drawn == Ok(Drawn::Shown))
		&& *minimized == Ok(Drawn::Skipped)
}

#[test]
fn a_headless_window_shows_every_grid_scene_through_resizes() {
	let Some(entry) = entry() else { return };
	let offered = entry.instance_extensions().expect("extensions");
	if !offered.iter().any(|name| name == "VK_EXT_headless_surface") {
		assert!(!required(), "no headless surfaces: {offered:?}");
		return eprintln!("skipped: no headless surfaces");
	}
	let drawn = headless_window(entry).expect("a window");
	assert!(as_expected(&drawn), "{drawn:?}");
	assert!(drawn.contains(&Err(Error::Draw)), "the scene with images");
}
