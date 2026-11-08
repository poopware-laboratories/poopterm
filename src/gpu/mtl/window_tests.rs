//! Tests of a window drawn by Metal: what the layer is told as the window
//! changes size and scale, and frames drawn into a real layer's drawables,
//! on a layer no view hosts.
use super::{DRAWABLES, Drawn, MetalWindow, Size, resized};
use crate::{
	gpu::{
		atlas::Wanted,
		mtl::device::{Error, tests::device},
		quads::QuadPen,
	},
	render::pen::Pen,
};

const ATLAS: (usize, usize) = (64, 4);

/// A window on a layer no view hosts, or `None` without a Metal device.
fn window() -> Option<MetalWindow> {
	device()?;
	// SAFETY: no view.
	Some(unsafe { MetalWindow::new(None, ATLAS) }.unwrap())
}

/// A frame of one rect over `size`.
fn pen(size: (usize, usize)) -> QuadPen {
	let mut pen = QuadPen::new(size, ATLAS);
	pen.begin_frame(size);
	pen.rect(0, 0, size.0, size.1, 0x20_3040);
	pen
}

/// What the layer says its drawables' size and contents' scale are.
fn told(window: &MetalWindow) -> (Size, f64) {
	// SAFETY: `drawableSize` takes nothing and returns a `CGSize`, two
	// doubles returned in registers; `contentsScale` a `CGFloat`.
	unsafe {
		(
			window.layer.send(c"drawableSize", ()),
			window.layer.send(c"contentsScale", ()),
		)
	}
}

#[test]
fn the_layer_is_told_the_size_in_pixels_and_the_scale_first_and_on_each_change() {
	let size = |width, height| Size { width, height };
	assert_eq!(
		resized(None, [800, 600], 2.0),
		Some((size(800.0, 600.0), 2.0))
	);
	let told = Some(([800, 600], 2.0));
	assert_eq!(resized(told, [800, 600], 2.0), None);
	// Larger, or narrower by a pixel.
	assert_eq!(
		resized(told, [801, 600], 2.0),
		Some((size(801.0, 600.0), 2.0))
	);
	assert_eq!(
		resized(told, [800, 599], 2.0),
		Some((size(800.0, 599.0), 2.0))
	);
	// Moved to a display of another scale, at the same size in pixels.
	assert_eq!(
		resized(told, [800, 600], 1.0),
		Some((size(800.0, 600.0), 1.0))
	);
	assert_eq!(
		resized(told, [800, 600], 1.5),
		Some((size(800.0, 600.0), 1.5))
	);
}

#[test]
fn a_scale_that_is_no_scale_is_one() {
	for nonsense in [0.0, -2.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
		let (_, scale) = resized(None, [10, 10], nonsense).unwrap();
		assert_eq!(scale, 1.0, "{nonsense}");
		// And told once, is not told again.
		assert_eq!(resized(Some(([10, 10], 1.0)), [10, 10], nonsense), None);
	}
}

#[test]
fn sizes_as_large_as_a_u32_reach_the_layer_exactly() {
	let (drawables, _) = resized(None, [u32::MAX, 1], 1.0).unwrap();
	assert_eq!(drawables.width, 4_294_967_295.0);
}

#[test]
fn a_frame_is_drawn_into_a_drawable_of_the_window_s_size_and_scale() {
	let Some(mut window) = window() else { return };
	assert!(!window.device_name().is_empty());
	let mut pen = pen((120, 80));
	assert_eq!(window.draw(&mut pen, [120, 80], 2.0), Ok(Drawn::Shown));
	assert_eq!(
		told(&window),
		(
			Size {
				width: 120.0,
				height: 80.0
			},
			2.0
		)
	);
	// Resized, then on a display of another scale.
	let mut pen = self::pen((64, 48));
	assert_eq!(window.draw(&mut pen, [64, 48], 2.0), Ok(Drawn::Shown));
	assert_eq!(
		told(&window),
		(
			Size {
				width: 64.0,
				height: 48.0
			},
			2.0
		)
	);
	let mut pen = self::pen((64, 48));
	assert_eq!(window.draw(&mut pen, [64, 48], 1.0), Ok(Drawn::Shown));
	assert_eq!(told(&window).1, 1.0);
	assert_eq!(window.renderer.finish(), Ok(()));
}

#[test]
fn a_window_without_area_draws_nothing_and_tells_the_layer_nothing() {
	let Some(mut window) = window() else { return };
	for size in [[0, 80], [120, 0], [0, 0]] {
		let mut pen = pen((120, 80));
		assert_eq!(
			window.draw(&mut pen, size, 2.0),
			Ok(Drawn::Skipped),
			"{size:?}"
		);
	}
	assert_eq!(window.told, None);
}

#[test]
fn many_frames_in_a_row_are_shown_as_drawables_come_free() {
	let Some(mut window) = window() else { return };
	for frame in 0..DRAWABLES * 4 {
		let mut pen = pen((32, 32));
		let drawn = window.draw(&mut pen, [32, 32], 1.0);
		assert!(
			matches!(drawn, Ok(Drawn::Shown | Drawn::Again)),
			"{frame}: {drawn:?}"
		);
	}
}

#[test]
fn a_frame_lacking_room_in_the_atlas_grows_it_and_is_drawn_again() {
	let Some(mut window) = window() else { return };
	let mut pen = pen((32, 32));
	pen.complete = false;
	pen.wanted = Wanted {
		side: 0,
		pages: true,
	};
	assert_eq!(window.draw(&mut pen, [32, 32], 1.0), Ok(Drawn::Again));
	assert_eq!(window.renderer.atlas.size, [64, 64, 8]);
	// The pen starts over at the atlas it grew to.
	assert!(pen.complete && pen.quads.is_empty());
}

#[test]
fn a_frame_lacking_room_no_atlas_holds_is_refused_for_software_to_draw() {
	let Some(mut window) = window() else { return };
	let mut pen = pen((32, 32));
	pen.complete = false;
	pen.wanted = Wanted {
		side: 1 << 20,
		pages: false,
	};
	assert_eq!(window.draw(&mut pen, [32, 32], 1.0), Err(Error::Draw));
	assert!(pen.complete && pen.quads.is_empty());
	assert_eq!(window.renderer.atlas.size, [64, 64, 4]);
}
