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
