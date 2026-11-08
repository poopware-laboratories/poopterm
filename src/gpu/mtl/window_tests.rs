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

/// Commits this thread's Core Animation transaction, as a run loop would.
fn flush() {
	let _pool = crate::objc::Pool::new();
	let transaction = crate::objc::class(c"CATransaction").unwrap();
	// SAFETY: `flush` takes nothing.
	unsafe { transaction.send::<()>(c"flush", ()) };
}

/// What the layer says its drawables' size and contents' scale are.
pub(crate) fn told(window: &MetalWindow) -> (Size, f64) {
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

#[test]
fn the_layer_draws_bgra_with_our_device_three_drawables_at_most_and_opaque() {
	use crate::objc::{Bool, Raw};
	let Some(window) = window() else { return };
	// SAFETY: the layer's getters take nothing and return an enum, `BOOL`s,
	// an `NSUInteger` and the device.
	let (format, only, most, opaque, device): (usize, Bool, usize, Bool, Raw) = unsafe {
		(
			window.layer.send(c"pixelFormat", ()),
			window.layer.send(c"framebufferOnly", ()),
			window.layer.send(c"maximumDrawableCount", ()),
			window.layer.send(c"isOpaque", ()),
			window.layer.send(c"device", ()),
		)
	};
	assert_eq!(format, 80);
	assert_ne!(only, 0);
	assert_eq!(most, DRAWABLES);
	assert_ne!(opaque, 0);
	assert_eq!(device, window.device.device.raw());
	// Nothing is told the layer before the first frame.
	assert_eq!(window.told, None);
}

#[test]
fn a_window_dropped_lets_go_of_its_layer_once_its_frames_are_done() {
	use crate::objc::probe_tests::Weak;
	let Some(mut window) = window() else { return };
	let mut pen = pen((16, 16));
	assert_eq!(window.draw(&mut pen, [16, 16], 1.0), Ok(Drawn::Shown));
	let mut layer = Weak::new(&window.layer);
	drop(window);
	// Core Animation holds layers changed on a thread until its transaction
	// commits, which the main thread's run loop does in the app.
	flush();
	assert!(!layer.alive());
}

/// What a fake view was sent: the layer it was given, and whether it was
/// told it wants one, in order.
#[derive(Debug, PartialEq)]
enum Hosted {
	Layer(usize),
	Wants(bool),
}

thread_local! {
	static HOSTED: std::cell::RefCell<Vec<Hosted>> = const { std::cell::RefCell::new(Vec::new()) };
}

extern "C" fn set_layer(_: crate::objc::Raw, _: crate::objc::Sel, layer: crate::objc::Raw) {
	HOSTED.with(|hosted| hosted.borrow_mut().push(Hosted::Layer(layer as usize)));
}

extern "C" fn set_wants(_: crate::objc::Raw, _: crate::objc::Sel, wants: crate::objc::Bool) {
	HOSTED.with(|hosted| hosted.borrow_mut().push(Hosted::Wants(wants != 0)));
}

/// A fake `NSView`, which records the layer it is given.
fn fake_view() -> crate::objc::Object {
	use crate::objc::probe_tests::{class_named, register};
	use std::{ffi::c_void, sync::OnceLock};
	static VIEW: OnceLock<usize> = OnceLock::new();
	let raw = *VIEW.get_or_init(|| {
		let methods: [(&std::ffi::CStr, *const c_void, &std::ffi::CStr); 2] = [
			(c"setLayer:", set_layer as *const c_void, c"v@:@"),
			(c"setWantsLayer:", set_wants as *const c_void, c"v@:c"),
		];
		register(c"PooptermFakeView", &methods).raw() as usize
	});
	// SAFETY: a subclass of `NSObject`, whose `init` is `NSObject`'s.
	unsafe { class_named(raw).instance() }.unwrap()
}

#[test]
fn a_view_is_given_the_layer_before_it_is_told_it_wants_one() {
	use crate::objc::probe_tests::Weak;
	if device().is_none() {
		return;
	}
	let view = fake_view();
	let mut watched = Weak::new(&view);
	let at = std::ptr::NonNull::new(view.raw());
	HOSTED.with(|hosted| hosted.borrow_mut().clear());
	// SAFETY: the fake view outlives the window, on this thread.
	let window = unsafe { MetalWindow::new(at, ATLAS) }.unwrap();
	let hosted = HOSTED.with(|hosted| std::mem::take(&mut *hosted.borrow_mut()));
	// Given first, it hosts our layer rather than making its own.
	assert_eq!(
		hosted,
		[
			Hosted::Layer(window.layer.raw() as usize),
			Hosted::Wants(true)
		]
	);
	// The window keeps no reference to the view: it lives as its owner says.
	drop(window);
	flush();
	drop(view);
	assert!(!watched.alive());
}

#[test]
fn a_window_without_a_view_tells_no_view_anything() {
	let Some(_window) = window() else { return };
	assert_eq!(HOSTED.with(|hosted| hosted.borrow().len()), 0);
}
