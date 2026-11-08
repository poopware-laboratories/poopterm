//! Tests of the main window's Metal glue: when Metal may draw, which
//! windows it draws in, that any Metal failure abandons the GPU, and
//! frames and Settings scenes drawn by a real GPU on a layer no view hosts,
//! through resizes and changes of scale.
use super::{Gpu, NOT_ASKED, Next, allows_metal, next, ns_view};
use crate::{
	config::RendererPreference,
	gpu::mtl::{
		device::{Error, tests::device},
		window::MetalWindow,
	},
	objc::Missing,
	render::{
		grid::draw_grid,
		grid_case_tests::{BG, cases, fonts},
		pen::Pen,
		scene::Scene,
	},
};
use std::{ffi::c_void, num::NonZeroU32, ptr::NonNull};
use winit::raw_window_handle::{AppKitWindowHandle, RawWindowHandle, XcbWindowHandle};

const ATLAS: (usize, usize) = (256, 4);

/// The glue on a layer no view hosts, or `None` without a Metal device.
fn headless() -> Option<Gpu<()>> {
	device()?;
	// SAFETY: no view.
	let window = unsafe { MetalWindow::new(None, ATLAS) }.unwrap();
	Some(Gpu::with(window, (), ATLAS))
}

#[test]
fn metal_draws_only_when_the_gpu_is_asked_for_by_name() {
	assert!(allows_metal(RendererPreference::Gpu));
	assert!(!allows_metal(RendererPreference::Auto));
	assert!(!allows_metal(RendererPreference::Software));
}

#[test]
fn metal_not_asked_for_is_refused_before_it_looks_for_a_device() {
	let view = NonNull::new(0x1000 as *mut c_void);
	for preference in [RendererPreference::Auto, RendererPreference::Software] {
		// SAFETY: refused before the view is used.
		let refused = unsafe { Gpu::open((), view, preference) };
		assert_eq!(refused.err().as_deref(), Some(NOT_ASKED));
	}
	assert_eq!(
		NOT_ASKED,
		"Metal draws only when asked for (renderer = \"gpu\")"
	);
}

#[test]
fn an_appkit_window_is_drawn_in_through_its_view_and_no_other_kind() {
	let view = NonNull::new(0x7000 as *mut c_void).unwrap();
	let appkit = RawWindowHandle::AppKit(AppKitWindowHandle::new(view));
	assert_eq!(ns_view(appkit), Some(view));
	let xcb = RawWindowHandle::Xcb(XcbWindowHandle::new(NonZeroU32::new(9).unwrap()));
	assert_eq!(ns_view(xcb), None);
	// SAFETY: no view to use.
	let refused = unsafe { Gpu::open((), None, RendererPreference::Gpu) };
	assert_eq!(
		refused.err().as_deref(),
		Some("Metal cannot draw in this kind of window")
	);
}

#[test]
fn any_metal_failure_abandons_the_gpu_and_says_why() {
	let failures = [
		Error::NoDevice,
		Error::Missing(Missing(c"CAMetalLayer")),
		Error::Refused("drawable texture"),
		Error::Shader("no function".into()),
		Error::Gpu("the GPU hung".into()),
		Error::Draw,
		Error::Region,
	];
	for error in failures {
		let said = error.to_string();
		assert_eq!(next(Err(error)), Next::Abandon(said));
	}
}

#[test]
fn every_grid_scene_drawn_by_metal_in_a_window_is_shown() {
	let Some(mut gpu) = headless() else { return };
	let mut fonts = fonts();
	for case in cases() {
		let view = case.view();
		let size = (160, 96);
		let mut drawn = Next::Redraw;
		// A frame that grew the atlas is drawn again.
		for _ in 0..4 {
			drawn = gpu.frame(size, |pen| {
				let (width, height) = pen.size();
				pen.rect(0, 0, width, height, BG);
				draw_grid(pen, &mut fonts, &view, None);
			});
			if drawn != Next::Redraw {
				break;
			}
		}
		assert_eq!(drawn, Next::Done, "{}", case.name);
	}
}

#[test]
fn frames_are_shown_through_resizes_and_changes_of_scale() {
	let Some(mut gpu) = headless() else { return };
	for (size, scale) in [
		((200, 100), 2.0),
		((201, 99), 2.0),
		((100, 50), 1.0),
		((1, 1), 3.0),
	] {
		gpu.rescale(scale);
		let drawn = gpu.frame(size, |pen| {
			let (width, height) = pen.size();
			pen.rect(0, 0, width, height, BG);
		});
		assert_eq!(drawn, Next::Done, "{size:?} at {scale}");
	}
	// A window with no area has nothing to show, and draws once it has.
	assert_eq!(gpu.frame((0, 40), |_| {}), Next::Done);
}

#[test]
fn a_settings_scene_is_drawn_by_metal_as_quads() {
	let Some(mut gpu) = headless() else { return };
	let mut scene = Scene::new(120, 80);
	scene.fill(0x1e_1e2e);
	scene.round_rect(8, 8, 60, 24, 6, 0x2a_2a3a);
	scene.text(&mut fonts(), 10, 40, "Settings", 16.0, 14, 0xee_eeee);
	let mut drawn = Next::Redraw;
	for _ in 0..4 {
		drawn = gpu.scene(&scene, &mut fonts());
		if drawn != Next::Redraw {
			break;
		}
	}
	assert_eq!(drawn, Next::Done);
	assert!(!gpu.device_name().is_empty());
}

#[test]
fn a_new_scale_reaches_the_layer_with_the_next_frame_and_not_before() {
	use crate::gpu::mtl::window::tests::told;
	let Some(mut gpu) = headless() else { return };
	let background = |pen: &mut crate::gpu::quads::QuadPen| {
		let (width, height) = pen.size();
		pen.rect(0, 0, width, height, BG);
	};
	gpu.rescale(2.0);
	assert_eq!(gpu.frame((60, 40), background), Next::Done);
	assert_eq!(told(&gpu.gpu).1, 2.0);
	gpu.rescale(1.5);
	assert_eq!(told(&gpu.gpu).1, 2.0);
	assert_eq!(gpu.frame((60, 40), background), Next::Done);
	let (size, scale) = told(&gpu.gpu);
	// The size is the window's in pixels, whatever the scale.
	assert_eq!((size.width, size.height, scale), (60.0, 40.0, 1.5));
}

#[test]
fn a_frame_after_one_metal_showed_redraws_only_the_rows_planned() {
	use crate::render::damage::GridDamage;
	let Some(mut gpu) = headless() else { return };
	let mut fonts = fonts();
	let case = cases().into_iter().next().unwrap();
	let view = case.view();
	let size = (160, 96);
	let mut planned = Vec::new();
	let mut draw = |gpu: &mut Gpu<()>, planned: &mut Vec<Option<Vec<bool>>>| {
		for _ in 0..4 {
			let next = gpu.frame(size, |pen| {
				planned.push(pen.rows_to_draw().map(<[bool]>::to_vec));
				let (width, height) = pen.size();
				pen.rect(0, 0, width, height, BG);
				draw_grid(pen, &mut fonts, &view, None);
			});
			if next != Next::Redraw {
				return next;
			}
		}
		Next::Redraw
	};
	assert_eq!(draw(&mut gpu, &mut planned), Next::Done);
	assert!(gpu.keeps_rows());
	let rows = view.frame.rows;
	let mut damaged = vec![false; rows];
	damaged[1] = true;
	gpu.plan_rows(
		Some(GridDamage {
			scroll: 0,
			rows: damaged.clone(),
		}),
		16,
	);
	planned.clear();
	assert_eq!(draw(&mut gpu, &mut planned), Next::Done);
	// Only the row that changed is drawn again; the others are kept.
	assert_eq!(planned, [Some(damaged)]);
	assert!(gpu.keeps_rows());
	// Without damage, every row.
	gpu.plan_rows(None, 16);
	planned.clear();
	assert_eq!(draw(&mut gpu, &mut planned), Next::Done);
	assert_eq!(planned, [None]);
}

#[test]
fn software_says_why_when_metal_was_not_asked_for() {
	use crate::{
		render::backend::Backend,
		ui::{
			diagnostics::{Drawing, Reason, statuses},
			diagnostics_page::started,
		},
	};
	let drawing = started(Backend::Software, Some(NOT_ASKED), true);
	assert_eq!(
		drawing,
		Drawing::Software(Reason::Unavailable(NOT_ASKED.into()))
	);
	let [status, ..] = statuses(&drawing);
	assert_eq!(
		status.detail,
		"Metal unavailable: Metal draws only when asked for (renderer = \"gpu\")"
	);
	// Asked for software, it says so instead.
	let asked = started(Backend::Software, None, true);
	assert_eq!(asked, Drawing::Software(Reason::Asked));
}

#[test]
fn a_label_larger_than_a_page_grows_the_atlas_and_is_drawn() {
	let Some(mut gpu) = headless() else { return };
	let mut scene = Scene::new(160, 120);
	scene.fill(0x1e_1e2e);
	// A 600 pixel glyph, as a huge scale factor makes: past a 256 page.
	scene.text(&mut fonts(), 0, 0, "W", 600.0, 500, 0xee_eeee);
	let mut again = 0;
	let mut next = gpu.scene(&scene, &mut fonts());
	while next == Next::Redraw && again < 4 {
		again += 1;
		next = gpu.scene(&scene, &mut fonts());
	}
	assert_eq!(next, Next::Done);
	// Drawn again once the atlas grew.
	assert!(again > 0);
}

#[test]
fn a_glyph_no_atlas_holds_hands_the_window_to_software_saying_why() {
	use super::drawn_as;
	use crate::{
		fonts::Metrics,
		render::{glyphs::Glyph, pen::Ink},
		ui::diagnostics::{Drawing, Reason},
	};
	let Some(mut gpu) = headless() else { return };
	// One past the widest texture Metal makes.
	let glyph = Glyph {
		metrics: Metrics {
			width: 16385,
			height: 1,
			..Metrics::default()
		},
		coverage: vec![0x80; 16385],
		color: None,
	};
	let mut next = Next::Redraw;
	for _ in 0..8 {
		next = gpu.frame((40, 30), |pen| {
			pen.rect(0, 0, 40, 30, BG);
			let ink = Ink::Drawn('w', (16385, 1));
			pen.glyph(&glyph, ink, (0, 0), (0, 0, 40, 30), false, 0xff_ffff);
		});
		if next != Next::Redraw {
			break;
		}
	}
	assert_eq!(next, Next::Abandon(Error::Draw.to_string()));
	// The pen started over: the next frame draws every row.
	assert!(!gpu.keeps_rows());
	let mut drawing = Drawing::Gpu;
	drawn_as(&mut drawing, &next);
	assert_eq!(
		drawing,
		Drawing::Software(Reason::Abandoned("a frame the GPU cannot hold".into()))
	);
}
