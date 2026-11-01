//! Tests of drawing the main window's frames on the GPU against the fake
//! Vulkan driver: the whole frame drawn as quads, the grid drawn by the GPU
//! over what software painted, software painting the whole frame when an
//! overlay covers the grid or the GPU cannot draw it, and what each outcome
//! leads to.
use super::{Gpu, Layers, Next};
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
		damage::GridDamage,
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

/// What each frame drew, in order: `Around` for all but the grid when it
/// painted a backdrop, `Quads` for a frame of quads alone, `Whole` for the
/// whole frame painted by software.
#[derive(Debug, PartialEq)]
enum Drew {
	Around,
	Quads,
	Whole,
}
use Drew::{Around, Quads, Whole};

#[derive(Default)]
struct Painted(Vec<Drew>);

/// Draws like the app: the chrome, into the backdrop when there is one,
/// else as a quad, and a cell of grid with a letter on the pen; with
/// `picture`, the grid holds an image.
fn around(picture: bool) -> impl FnOnce(&mut Painted, Option<&mut [u32]>, &mut QuadPen) {
	move |painted, pixels, pen| {
		match pixels {
			Some(pixels) => {
				pixels.fill(CHROME);
				painted.0.push(Around);
			}
			None => {
				pen.rect(0, 0, SIZE.0, SIZE.1, CHROME);
				painted.0.push(Quads);
			}
		}
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
	}
}

fn whole(painted: &mut Painted, pixels: &mut [u32]) {
	pixels.fill(WHOLE);
	painted.0.push(Whole);
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

/// Draws a frame of these `layers`, saying what it came to and what was
/// drawn for it.
fn frame(gpu: &mut Gpu<&str>, layers: Layers, picture: bool) -> (Next, Vec<Drew>, Vec<u32>) {
	let mut pixels = vec![0; SIZE.0 * SIZE.1];
	let mut painted = Painted::default();
	let next = gpu.frame(
		&mut painted,
		(SIZE, &mut pixels),
		layers,
		around(picture),
		whole,
	);
	(next, painted.0, pixels)
}

const BACKDROP: Layers = Layers::OverBackdrop;
const OVERLAY: Layers = Layers::Whole;

#[test]
fn a_frame_of_quads_uploads_no_backdrop() {
	let mut gpu = gpu();
	let (next, painted, pixels) = frame(&mut gpu, Layers::Quads, false);
	assert_eq!((next, painted), (Next::Done, vec![Quads]));
	assert!(
		pixels.iter().all(|&pixel| pixel == 0),
		"software painted nothing"
	);
	// The letter to the atlas, then the chrome, the cell and its letter.
	let calls = recorded();
	assert!(calls[0].contains("size 3x2"), "{calls:?}");
	assert_eq!(calls[1..], ["draw 6 vertices of 3 instances from 0"]);
	assert_eq!(presents(), 1);
	frame(&mut gpu, Layers::Quads, false);
	assert_eq!(recorded(), ["draw 6 vertices of 3 instances from 0"]);
	close(gpu.gpu);
}

#[test]
fn a_frame_of_quads_the_gpu_cannot_draw_is_painted_whole_and_sent_as_a_backdrop() {
	let mut gpu = gpu();
	let (next, painted, pixels) = frame(&mut gpu, Layers::Quads, true);
	assert_eq!((next, painted), (Next::Done, vec![Quads, Whole]));
	assert!(pixels.iter().all(|&pixel| pixel == WHOLE));
	let calls = recorded();
	// The backdrop copied, and no quad left to draw over it.
	assert!(calls[0].contains("size 40x30"), "{calls:?}");
	assert_eq!(calls[1..], [""; 0]);
	assert_eq!(presents(), 1);
	close(gpu.gpu);
}

#[test]
fn the_grid_is_drawn_by_the_gpu_over_the_chrome_software_painted() {
	let mut gpu = gpu();
	let (next, painted, pixels) = frame(&mut gpu, BACKDROP, false);
	assert_eq!((next, painted), (Next::Done, vec![Around]));
	assert!(pixels.iter().all(|&pixel| pixel == CHROME));
	// The letter to the atlas, the chrome into the whole image, then the
	// cell and its letter over it.
	let calls = recorded();
	assert!(calls[0].contains("size 3x2"), "{calls:?}");
	assert!(calls[1].contains("size 40x30"), "{calls:?}");
	assert_eq!(calls[2..], ["draw 6 vertices of 2 instances from 0"]);
	assert_eq!(presents(), 1);
	// Each frame starts with an empty pen, and the atlas keeps the letter.
	frame(&mut gpu, BACKDROP, false);
	let calls = recorded();
	assert!(calls[0].contains("size 40x30"), "{calls:?}");
	assert_eq!(calls[1..], ["draw 6 vertices of 2 instances from 0"]);
	close(gpu.gpu);
}

#[test]
fn an_overlay_over_the_grid_has_software_paint_the_whole_frame() {
	let mut gpu = gpu();
	let (next, painted, pixels) = frame(&mut gpu, OVERLAY, false);
	assert_eq!((next, painted), (Next::Done, vec![Whole]));
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
	let (next, painted, pixels) = frame(&mut gpu, BACKDROP, true);
	assert_eq!((next, painted), (Next::Done, vec![Around, Whole]));
	assert!(pixels.iter().all(|&pixel| pixel == WHOLE));
	assert_eq!(presents(), 1, "only the frame software painted");
	// The next frame without the picture is the GPU's again.
	assert_eq!(frame(&mut gpu, BACKDROP, false).1, [Around]);
	close(gpu.gpu);
}

#[test]
fn a_frame_the_swapchain_could_not_take_is_drawn_again() {
	let mut gpu = gpu();
	frame(&mut gpu, BACKDROP, false);
	fake_out_of_date();
	let (next, painted, _) = frame(&mut gpu, BACKDROP, false);
	assert_eq!((next, painted), (Next::Redraw, vec![Around]));
	assert_eq!(frame(&mut gpu, BACKDROP, false).0, Next::Done);
	close(gpu.gpu);
}

#[test]
fn a_failing_gpu_is_abandoned_with_the_reason() {
	let mut gpu = gpu();
	fake_failing(Some((c"vkQueueSubmit2", -4)));
	let (next, painted, _) = frame(&mut gpu, BACKDROP, false);
	assert_eq!(next, Next::Abandon(Error::Failed(-4).to_string()));
	assert_eq!(painted, [Around], "software is not asked to paint for it");
	fake_failing(None);
	close(gpu.gpu);
}

#[test]
fn a_window_without_area_shows_nothing_and_waits() {
	let mut gpu = gpu();
	let next = gpu.frame(
		&mut Painted::default(),
		((40, 0), &mut []),
		BACKDROP,
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
	frame(&mut gpu, BACKDROP, false);
	frame(&mut gpu, OVERLAY, false);
	recorded();
	frame(&mut gpu, BACKDROP, false);
	let uploads = recorded()
		.into_iter()
		.filter(|call| call.starts_with("upload"));
	assert_eq!(uploads.count(), 1, "the backdrop alone");
	close(gpu.gpu);
}

#[test]
fn the_letters_of_a_frame_left_to_software_are_sent_with_the_next() {
	let mut gpu = gpu();
	frame(&mut gpu, BACKDROP, true);
	recorded();
	frame(&mut gpu, BACKDROP, false);
	let calls = recorded();
	assert!(calls[0].contains("size 3x2"), "{calls:?}");
	close(gpu.gpu);
}

#[test]
fn a_frame_software_painted_follows_the_swapchain_and_the_gpu_too() {
	let mut gpu = gpu();
	frame(&mut gpu, BACKDROP, false);
	fake_out_of_date();
	let (next, painted, _) = frame(&mut gpu, OVERLAY, false);
	assert_eq!((next, painted), (Next::Redraw, vec![Whole]));
	fake_failing(Some((c"vkQueueSubmit2", -4)));
	let (next, painted, _) = frame(&mut gpu, BACKDROP, true);
	assert_eq!(next, Next::Abandon(Error::Failed(-4).to_string()));
	assert_eq!(painted, [Around, Whole]);
	fake_failing(None);
	close(gpu.gpu);
}

#[test]
fn a_resized_window_gets_the_whole_backdrop_of_its_new_size() {
	let mut gpu = gpu();
	frame(&mut gpu, BACKDROP, false);
	recorded();
	let mut pixels = vec![0; 24 * 10];
	let mut painted = Painted::default();
	let next = gpu.frame(
		&mut painted,
		((24, 10), &mut pixels),
		BACKDROP,
		around(false),
		whole,
	);
	assert_eq!(next, Next::Done);
	let calls = recorded();
	assert!(calls[0].contains("size 24x10"), "{calls:?}");
	assert_eq!(presents(), 2);
	close(gpu.gpu);
}

#[test]
fn a_resized_window_of_quads_still_uploads_no_backdrop() {
	let mut gpu = gpu();
	frame(&mut gpu, Layers::Quads, false);
	recorded();
	let mut pixels = vec![0; 24 * 10];
	let mut painted = Painted::default();
	let next = gpu.frame(
		&mut painted,
		((24, 10), &mut pixels),
		Layers::Quads,
		around(false),
		whole,
	);
	assert_eq!((next, painted.0), (Next::Done, vec![Quads]));
	assert_eq!(recorded(), ["draw 6 vertices of 3 instances from 0"]);
	assert_eq!(presents(), 2);
	close(gpu.gpu);
}

#[test]
fn the_letters_of_a_frame_of_quads_left_to_software_are_sent_with_the_next() {
	let mut gpu = gpu();
	frame(&mut gpu, Layers::Quads, true);
	recorded();
	let (next, painted, _) = frame(&mut gpu, Layers::Quads, false);
	assert_eq!((next, painted), (Next::Done, vec![Quads]));
	let calls = recorded();
	assert!(calls[0].contains("size 3x2"), "{calls:?}");
	assert_eq!(calls[1..], ["draw 6 vertices of 3 instances from 0"]);
	close(gpu.gpu);
}

#[test]
fn a_frame_of_quads_the_swapchain_could_not_take_is_drawn_again_as_quads() {
	let mut gpu = gpu();
	frame(&mut gpu, Layers::Quads, false);
	fake_out_of_date();
	let (next, painted, pixels) = frame(&mut gpu, Layers::Quads, false);
	assert_eq!((next, painted), (Next::Redraw, vec![Quads]));
	assert!(
		pixels.iter().all(|&pixel| pixel == 0),
		"software painted nothing"
	);
	assert_eq!(frame(&mut gpu, Layers::Quads, false).0, Next::Done);
	close(gpu.gpu);
}

#[test]
fn a_gpu_failing_a_frame_of_quads_is_abandoned_without_software_painting() {
	let mut gpu = gpu();
	fake_failing(Some((c"vkQueueSubmit2", -4)));
	let (next, painted, _) = frame(&mut gpu, Layers::Quads, false);
	assert_eq!(next, Next::Abandon(Error::Failed(-4).to_string()));
	assert_eq!(painted, [Quads]);
	fake_failing(None);
	close(gpu.gpu);
}

/// Draws a frame of quads whose grid is two rows, each a rectangle, the
/// ones the pen says to draw; returns what it came to and the rows drawn.
fn grid_frame(gpu: &mut Gpu<&str>, picture: bool) -> (Next, Option<Vec<bool>>) {
	let mut pixels = vec![0; SIZE.0 * SIZE.1];
	let mut drawn = None;
	let around = |drawn: &mut Option<Vec<bool>>, _: Option<&mut [u32]>, pen: &mut QuadPen| {
		*drawn = pen.rows_to_draw().map(<[bool]>::to_vec);
		pen.rect(0, 0, SIZE.0, SIZE.1, CHROME);
		for row in 0..2 {
			if drawn.as_ref().is_none_or(|rows: &Vec<bool>| rows[row]) {
				pen.begin_row(row);
				pen.rect(0, row * 16, 8, 16, 0xff_0000);
			}
		}
		pen.end_grid(2);
		if picture {
			picture_into(pen);
		}
	};
	let next = gpu.frame(
		&mut drawn,
		(SIZE, &mut pixels),
		Layers::Quads,
		around,
		|_, _| {},
	);
	(next, drawn)
}

#[test]
fn rows_planned_for_the_next_frame_are_the_ones_it_draws() {
	let mut gpu = gpu();
	assert!(!gpu.keeps_rows());
	let damage = || {
		Some(GridDamage {
			scroll: 0,
			rows: vec![false, true],
		})
	};
	// The pen keeps no rows yet: every row is drawn.
	gpu.plan_rows(damage(), 16);
	assert_eq!(grid_frame(&mut gpu, false), (Next::Done, None));
	assert!(gpu.keeps_rows());
	assert_eq!(recorded(), ["draw 6 vertices of 3 instances from 0"]);
	gpu.plan_rows(damage(), 16);
	assert_eq!(
		grid_frame(&mut gpu, false),
		(Next::Done, Some(vec![false, true]))
	);
	// The chrome, the kept row and the redrawn one.
	assert_eq!(recorded(), ["draw 6 vertices of 3 instances from 0"]);
	gpu.plan_rows(None, 16);
	assert_eq!(grid_frame(&mut gpu, false), (Next::Done, None));
	close(gpu.gpu);
}

#[test]
fn a_frame_refused_or_painted_whole_leaves_the_pen_without_rows() {
	let mut gpu = gpu();
	grid_frame(&mut gpu, false);
	assert!(gpu.keeps_rows());
	// A frame with an image is refused, and the pen started over.
	assert_eq!(grid_frame(&mut gpu, true).0, Next::Done);
	assert!(!gpu.keeps_rows());
	grid_frame(&mut gpu, false);
	assert!(gpu.keeps_rows());
	// A frame the GPU failed to show started the pen over too.
	fake_failing(Some((c"vkQueueSubmit2", -4)));
	assert!(matches!(grid_frame(&mut gpu, false).0, Next::Abandon(_)));
	fake_failing(None);
	assert!(!gpu.keeps_rows());
	close(gpu.gpu);
}
