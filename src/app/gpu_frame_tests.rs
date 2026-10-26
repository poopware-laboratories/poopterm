//! Tests of drawing the main window's frames on the GPU against the fake
//! Vulkan driver: the grid drawn by the GPU over what software painted,
//! software painting the whole frame when an overlay covers the grid or the
//! GPU cannot draw it, and what each outcome leads to.
use super::{Gpu, Next};
use crate::{
	fonts::Metrics,
	gpu::{
		quads::QuadPen,
		vk::{
			call::Error,
			outside_tests::{
				fake_close as close, fake_failing, fake_out_of_date, fake_presents as presents,
				fake_recorded as recorded, fake_window,
			},
		},
	},
	graphics::{Bitmap, Placement, Rect},
	render::{
		glyphs::Glyph,
		pen::{Ink, Pen},
	},
};
use std::sync::Arc;

const SIZE: (usize, usize) = (40, 30);
const CHROME: u32 = 0x20_3040;
const WHOLE: u32 = 0x50_6070;

fn gpu() -> Gpu<&'static str> {
	Gpu::with(fake_window(), "the window")
}

/// What each frame painted, in order: `true` for all but the grid, which
/// it drew with the pen, `false` for the whole frame.
#[derive(Default)]
struct Painted(Vec<bool>);

/// Paints like the app: the chrome, and a cell of grid with a letter on
/// the pen; with
/// `picture`, the grid holds an image.
fn around(picture: bool) -> impl FnOnce(&mut Painted, &mut [u32], &mut QuadPen) {
	move |painted, pixels, pen| {
		pixels.fill(CHROME);
		pen.rect(8, 4, 8, 16, 0xff_0000);
		let glyph = Glyph {
			metrics: Metrics {
				width: 3,
				height: 2,
				..Metrics::default()
			},
			coverage: vec![0x80; 6],
			color: None,
		};
		let ink = Ink::Drawn('a', (3, 2));
		pen.glyph(&glyph, ink, (9, 5), (0, 0, 40, 30), false, 0xffffff);
		if picture {
			picture_into(pen);
		}
		painted.0.push(true);
	}
}

fn whole(painted: &mut Painted, pixels: &mut [u32]) {
	pixels.fill(WHOLE);
	painted.0.push(false);
}

fn picture_into(pen: &mut QuadPen) {
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
	pen.placement(whole, &Placement::natural(bitmap, 0, 0), (0, 0), (8, 16));
}

/// Draws a frame, saying what it came to and what was painted for it.
fn frame(gpu: &mut Gpu<&str>, overlay: bool, picture: bool) -> (Next, Vec<bool>, Vec<u32>) {
	let mut pixels = vec![0; SIZE.0 * SIZE.1];
	let mut painted = Painted::default();
	let next = gpu.frame(
		&mut painted,
		(SIZE, &mut pixels),
		overlay,
		around(picture),
		whole,
	);
	(next, painted.0, pixels)
}

#[test]
fn the_grid_is_drawn_by_the_gpu_over_the_chrome_software_painted() {
	let mut gpu = gpu();
	let (next, painted, pixels) = frame(&mut gpu, false, false);
	assert_eq!((next, painted), (Next::Done, vec![true]));
	assert!(pixels.iter().all(|&pixel| pixel == CHROME));
	// The letter to the atlas, the chrome into the whole image, then the
	// cell and its letter over it.
	let calls = recorded();
	assert!(calls[0].contains("size 3x2"), "{calls:?}");
	assert!(calls[1].contains("size 40x30"), "{calls:?}");
	assert_eq!(calls[2..], ["draw 6 vertices of 2 instances from 0"]);
	assert_eq!(presents(), 1);
	// Each frame starts with an empty pen, and the atlas keeps the letter.
	frame(&mut gpu, false, false);
	let calls = recorded();
	assert!(calls[0].contains("size 40x30"), "{calls:?}");
	assert_eq!(calls[1..], ["draw 6 vertices of 2 instances from 0"]);
	close(gpu.gpu);
}

#[test]
fn an_overlay_over_the_grid_has_software_paint_the_whole_frame() {
	let mut gpu = gpu();
	let (next, painted, pixels) = frame(&mut gpu, true, false);
	assert_eq!((next, painted), (Next::Done, vec![false]));
	assert!(pixels.iter().all(|&pixel| pixel == WHOLE));
	let calls = recorded();
	assert!(calls[0].contains("size 40x30"), "{calls:?}");
	assert!(!calls.iter().any(|call| call.contains("of 2 instances")));
	assert_eq!(presents(), 1);
	close(gpu.gpu);
}

#[test]
fn a_grid_the_gpu_cannot_draw_is_painted_by_software_in_the_same_frame() {
	let mut gpu = gpu();
	let (next, painted, pixels) = frame(&mut gpu, false, true);
	assert_eq!((next, painted), (Next::Done, vec![true, false]));
	assert!(pixels.iter().all(|&pixel| pixel == WHOLE));
	assert_eq!(presents(), 1, "only the frame software painted");
	// The next frame without the picture is the GPU's again.
	assert_eq!(frame(&mut gpu, false, false).1, [true]);
	close(gpu.gpu);
}

#[test]
fn a_frame_the_swapchain_could_not_take_is_drawn_again() {
	let mut gpu = gpu();
	frame(&mut gpu, false, false);
	fake_out_of_date();
	let (next, painted, _) = frame(&mut gpu, false, false);
	assert_eq!((next, painted), (Next::Redraw, vec![true]));
	assert_eq!(frame(&mut gpu, false, false).0, Next::Done);
	close(gpu.gpu);
}

#[test]
fn a_failing_gpu_is_abandoned_with_the_reason() {
	let mut gpu = gpu();
	fake_failing(Some((c"vkQueueSubmit2", -4)));
	let (next, painted, _) = frame(&mut gpu, false, false);
	assert_eq!(next, Next::Abandon(Error::Failed(-4).to_string()));
	assert_eq!(painted, [true], "software is not asked to paint for it");
	fake_failing(None);
	close(gpu.gpu);
}

#[test]
fn a_window_without_area_shows_nothing_and_waits() {
	let mut gpu = gpu();
	let next = gpu.frame(
		&mut Painted::default(),
		((40, 0), &mut []),
		false,
		around(false),
		whole,
	);
	assert_eq!(next, Next::Done);
	assert_eq!(presents(), 0);
	assert_eq!(*gpu.window(), "the window");
	close(gpu.gpu);
}

#[test]
fn frames_under_an_overlay_keep_the_letters_the_gpu_holds() {
	let mut gpu = gpu();
	frame(&mut gpu, false, false);
	frame(&mut gpu, true, false);
	recorded();
	frame(&mut gpu, false, false);
	let uploads = recorded()
		.into_iter()
		.filter(|call| call.starts_with("upload"));
	assert_eq!(uploads.count(), 1, "the backdrop alone");
	close(gpu.gpu);
}

#[test]
fn the_letters_of_a_frame_left_to_software_are_sent_with_the_next() {
	let mut gpu = gpu();
	frame(&mut gpu, false, true);
	recorded();
	frame(&mut gpu, false, false);
	let calls = recorded();
	assert!(calls[0].contains("size 3x2"), "{calls:?}");
	close(gpu.gpu);
}

#[test]
fn a_frame_software_painted_follows_the_swapchain_and_the_gpu_too() {
	let mut gpu = gpu();
	frame(&mut gpu, false, false);
	fake_out_of_date();
	let (next, painted, _) = frame(&mut gpu, true, false);
	assert_eq!((next, painted), (Next::Redraw, vec![false]));
	fake_failing(Some((c"vkQueueSubmit2", -4)));
	let (next, painted, _) = frame(&mut gpu, false, true);
	assert_eq!(next, Next::Abandon(Error::Failed(-4).to_string()));
	assert_eq!(painted, [true, false]);
	fake_failing(None);
	close(gpu.gpu);
}

#[test]
fn a_resized_window_gets_the_whole_backdrop_of_its_new_size() {
	let mut gpu = gpu();
	frame(&mut gpu, false, false);
	recorded();
	let mut pixels = vec![0; 24 * 10];
	let mut painted = Painted::default();
	let next = gpu.frame(
		&mut painted,
		((24, 10), &mut pixels),
		false,
		around(false),
		whole,
	);
	assert_eq!(next, Next::Done);
	let calls = recorded();
	assert!(calls[0].contains("size 24x10"), "{calls:?}");
	assert_eq!(presents(), 2);
	close(gpu.gpu);
}
