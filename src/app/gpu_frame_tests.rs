//! Tests of drawing the main window's frames on the GPU against the fake
//! Vulkan driver: the whole frame drawn as quads, a frame the GPU cannot
//! draw handing the window to software, and what each outcome leads to.
use super::{Gpu, Next, drawn_as};
use crate::{
	fonts::Metrics,
	gpu::{
		quads::QuadPen,
		vk::{
			call::Error,
			outside_tests::{
				FAKE_ATLAS, fake_close as close, fake_failing, fake_out_of_date, fake_presents as presents,
				fake_recorded as recorded, fake_window,
			},
		},
	},
	render::{
		damage::GridDamage,
		glyphs::Glyph,
		pen::{Ink, Pen},
	},
	ui::diagnostics::{Drawing, Reason},
};

const SIZE: (usize, usize) = (40, 30);
const CHROME: u32 = 0x20_3040;

fn gpu() -> Gpu<&'static str> {
	Gpu::with(fake_window(), "the window", FAKE_ATLAS)
}

/// Draws like the app: the chrome as a quad, and a cell of grid with a
/// letter; with `huge`, a letter wider than the GPU makes an atlas.
fn quads(pen: &mut QuadPen, huge: bool) {
	pen.rect(0, 0, SIZE.0, SIZE.1, CHROME);
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
	if huge {
		huge_into(pen);
	}
}

/// A letter wider than the fake device makes an atlas, which no atlas
/// grows to hold, so the GPU refuses its frame.
fn huge_into(pen: &mut QuadPen) {
	let glyph = Glyph {
		metrics: Metrics {
			width: 16385,
			height: 1,
			..Metrics::default()
		},
		coverage: vec![0x80; 16385],
		color: None,
	};
	let ink = Ink::Drawn('w', (16385, 1));
	pen.glyph(&glyph, ink, (0, 0), (0, 0, 40, 30), false, 0xffffff);
}

/// Draws a frame `size` pixels large, saying what it came to.
fn frame_of(gpu: &mut Gpu<&str>, size: (usize, usize), huge: bool) -> Next {
	gpu.frame(size, |pen| quads(pen, huge))
}

fn frame(gpu: &mut Gpu<&str>, huge: bool) -> Next {
	frame_of(gpu, SIZE, huge)
}

/// What a frame the GPU refused comes to: the window goes to software.
fn refused() -> Next {
	Next::Abandon(Error::Draw.to_string())
}

#[test]
fn a_frame_of_quads_uploads_no_backdrop() {
	let mut gpu = gpu();
	assert_eq!(frame(&mut gpu, false), Next::Done);
	// The letter to the atlas, then the chrome, the cell and its letter.
	let calls = recorded();
	assert!(calls[0].contains("size 3x2"), "{calls:?}");
	assert_eq!(calls[1..], ["draw 6 vertices of 3 instances from 0"]);
	assert_eq!(presents(), 1);
	frame(&mut gpu, false);
	assert_eq!(recorded(), ["draw 6 vertices of 3 instances from 0"]);
	close(gpu.gpu);
}

#[test]
fn a_frame_the_gpu_cannot_draw_hands_the_window_to_software() {
	let mut gpu = gpu();
	assert_eq!(frame(&mut gpu, true), refused());
	// Nothing drawn or shown for it: software paints the window from now on.
	let calls = recorded();
	assert!(
		calls.iter().all(|call| !call.starts_with("draw")),
		"{calls:?}"
	);
	assert_eq!(presents(), 0);
	close(gpu.gpu);
}

#[test]
fn a_window_without_area_shows_nothing_and_waits() {
	let mut gpu = gpu();
	for size in [(40, 0), (0, 30)] {
		assert_eq!(frame_of(&mut gpu, size, false), Next::Done, "{size:?}");
	}
	assert_eq!(presents(), 0);
	assert_eq!(*gpu.window(), "the window");
	close(gpu.gpu);
}

#[test]
fn a_frame_no_atlas_holds_is_refused_before_a_swapchain_image_is_taken() {
	let mut gpu = gpu();
	frame(&mut gpu, false);
	// Refused at once, without touching the out-of-date swapchain: the
	// next frame drawn finds it so, and remakes it.
	fake_out_of_date();
	assert_eq!(frame(&mut gpu, true), refused());
	assert_eq!(presents(), 1);
	assert_eq!(frame(&mut gpu, false), Next::Redraw);
	assert_eq!(frame(&mut gpu, false), Next::Done);
	close(gpu.gpu);
}

#[test]
fn a_resized_window_of_quads_still_uploads_no_backdrop() {
	let mut gpu = gpu();
	frame(&mut gpu, false);
	recorded();
	assert_eq!(frame_of(&mut gpu, (24, 10), false), Next::Done);
	assert_eq!(recorded(), ["draw 6 vertices of 3 instances from 0"]);
	assert_eq!(presents(), 2);
	close(gpu.gpu);
}

#[test]
fn the_letters_of_a_refused_frame_are_sent_with_the_next() {
	let mut gpu = gpu();
	frame(&mut gpu, true);
	recorded();
	assert_eq!(frame(&mut gpu, false), Next::Done);
	let calls = recorded();
	assert!(calls[0].contains("size 3x2"), "{calls:?}");
	assert_eq!(calls[1..], ["draw 6 vertices of 3 instances from 0"]);
	close(gpu.gpu);
}

#[test]
fn a_frame_of_quads_the_swapchain_could_not_take_is_drawn_again_as_quads() {
	let mut gpu = gpu();
	frame(&mut gpu, false);
	fake_out_of_date();
	assert_eq!(frame(&mut gpu, false), Next::Redraw);
	assert_eq!(frame(&mut gpu, false), Next::Done);
	close(gpu.gpu);
}

#[test]
fn a_gpu_failing_a_frame_of_quads_is_abandoned_and_says_why() {
	let mut gpu = gpu();
	fake_failing(Some((c"vkQueueSubmit2", -4)));
	assert_eq!(
		frame(&mut gpu, false),
		Next::Abandon(Error::Failed(-4).to_string())
	);
	fake_failing(None);
	close(gpu.gpu);
}

/// Draws a frame of quads whose grid is two rows, each a rectangle, the
/// ones the pen says to draw; returns what it came to and the rows drawn.
fn grid_frame(gpu: &mut Gpu<&str>, huge: bool) -> (Next, Option<Vec<bool>>) {
	let mut drawn = None;
	let next = gpu.frame(SIZE, |pen| {
		drawn = pen.rows_to_draw().map(<[bool]>::to_vec);
		pen.rect(0, 0, SIZE.0, SIZE.1, CHROME);
		for row in 0..2 {
			if drawn.as_ref().is_none_or(|rows: &Vec<bool>| rows[row]) {
				pen.begin_row(row);
				pen.rect(0, row * 16, 8, 16, 0xff_0000);
			}
		}
		pen.end_grid(2);
		if huge {
			huge_into(pen);
		}
	});
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
fn a_frame_refused_or_failed_leaves_the_pen_without_rows() {
	let mut gpu = gpu();
	grid_frame(&mut gpu, false);
	assert!(gpu.keeps_rows());
	// A frame with a huge letter is refused, and the pen started over.
	assert_eq!(grid_frame(&mut gpu, true).0, refused());
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

#[test]
fn the_gpu_names_the_device_it_draws_on_for_the_diagnostics_page() {
	let gpu = gpu();
	// The fake driver's window draws on its integrated device.
	assert_eq!(gpu.facts().device, "Integrated");
	close(gpu.gpu);
}

#[test]
fn the_gpus_facts_carry_its_pens_atlas_and_its_swapchain_once_drawn() {
	let mut gpu = gpu();
	let facts = gpu.facts();
	assert_eq!(facts.device, "Integrated");
	let (side, pages) = FAKE_ATLAS;
	let atlas = facts.atlas.unwrap();
	assert_eq!((atlas.side, atlas.pages, atlas.filled), (side, pages, 0));
	assert_eq!(facts.swapchain, None);
	let labels = |gpu: &Gpu<&str>| -> Vec<&'static str> {
		gpu
			.facts()
			.rows()
			.into_iter()
			.map(|(label, _)| label)
			.collect()
	};
	assert_eq!(
		labels(&gpu),
		["Device", "Driver", "Glyph atlas", "Presenter"]
	);
	grid_frame(&mut gpu, false);
	assert!(gpu.facts().swapchain.is_some());
	// Rectangles only: the atlas stays as made.
	assert_eq!(gpu.facts().atlas, Some(atlas));
	assert_eq!(
		labels(&gpu),
		["Device", "Driver", "Swapchain", "Glyph atlas", "Presenter"]
	);
	close(gpu.gpu);
}

#[test]
fn frames_the_gpu_drew_leave_the_window_drawn_on_it() {
	for next in [Next::Done, Next::Redraw] {
		let mut drawing = Drawing::Gpu;
		drawn_as(&mut drawing, &next);
		assert_eq!(drawing, Drawing::Gpu, "{next:?}");
	}
}

#[test]
fn a_frame_the_gpu_abandoned_says_why_the_window_is_now_software() {
	let mut drawing = Drawing::Gpu;
	drawn_as(&mut drawing, &Next::Abandon("device lost".into()));
	let abandoned = Drawing::Software(Reason::Abandoned("device lost".into()));
	assert_eq!(drawing, abandoned);
	// Software draws later frames, so nothing changes the reason back.
	drawn_as(&mut drawing, &Next::Done);
	assert_eq!(drawing, abandoned);
}
