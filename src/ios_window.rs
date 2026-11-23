//! What iOS reports about the window: where it and its safe area are on
//! the screen and how large, read for `ui::insets::window_insets` along with
//! where the keyboard is, and on iPadOS 26 and later how far its window
//! controls reach into the top left corner.
// Calls into C or the kernel; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
use winit::{
	dpi::PhysicalPosition,
	raw_window_handle::{HasWindowHandle, RawWindowHandle},
	window::Window,
};

use crate::{
	objc::{self, Raw},
	ui::insets::{Insets, corner_reserve, window_insets},
};

/// `UIEdgeInsets`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Edges {
	top: f64,
	left: f64,
	bottom: f64,
	right: f64,
}

/// `UIViewLayoutRegionAdaptivityAxisNone` and `…Horizontal`.
const PLAIN: usize = 0;
const AROUND_CORNERS: usize = 1;

/// Readies the new `window`: gives it its scene, and the on-screen
/// keyboard, which typing needs on iOS, typing plain text.
pub(crate) fn attach(
	window: &Window,
	proxy: winit::event_loop::EventLoopProxy<crate::pty::UserEvent>,
) {
	crate::ios_scene::adopt(window);
	crate::ios_keyboard::plain_typing();
	window.set_ime_allowed(true);
	crate::ios_keyboard::observe(proxy);
}

/// The insets of `window`, drawn at `factor` pixels a point.
pub(crate) fn insets(window: &Window, factor: f64) -> Insets {
	let place = |at: Result<PhysicalPosition<i32>, _>| at.map_or((0, 0), |at| (at.x, at.y));
	let at = (
		place(window.inner_position()),
		place(window.outer_position()),
	);
	let (outer, inner) = (window.outer_size(), window.inner_size());
	let insets = window_insets(outer, inner, at, crate::ios_keyboard::top(), factor);
	let corner = view(window)
		.and_then(|view| Some((left_inset(view, PLAIN)?, left_inset(view, AROUND_CORNERS)?)))
		.map_or(0, |(plain, adapted)| corner_reserve(plain, adapted, factor));
	Insets { corner, ..insets }
}

/// winit's `UIView` for `window`.
fn view(window: &Window) -> Option<Raw> {
	let handle = window.window_handle().ok()?;
	match handle.as_raw() {
		RawWindowHandle::UiKit(handle) => Some(handle.ui_view.as_ptr()),
		_ => None,
	}
}

/// The left inset of `view`'s safe area adapted along `axis`, in points;
/// none before iPadOS 26, which lacks layout regions.
fn left_inset(view: Raw, axis: usize) -> Option<f64> {
	let regions = objc::class(c"UIViewLayoutRegion").ok()?;
	// SAFETY: `view` is winit's live `UIView`, an `NSObject`.
	let view = unsafe { objc::Object::retained(view) }?;
	if !view.responds(c"edgeInsetsForLayoutRegion:") {
		return None;
	}
	// SAFETY: `safeAreaLayoutRegionWithCornerAdaptation:` takes a
	// `UIViewLayoutRegionAdaptivityAxis` (`NSUInteger`) and returns an
	// autoreleased region; the class exists, so the method does.
	let region: Raw = unsafe { regions.send(c"safeAreaLayoutRegionWithCornerAdaptation:", (axis,)) };
	if region.is_null() {
		return None;
	}
	// SAFETY: the view answers `edgeInsetsForLayoutRegion:`, which takes a
	// region and returns `UIEdgeInsets`, four `double`s returned in
	// registers on arm64.
	let edges: Edges = unsafe { view.send(c"edgeInsetsForLayoutRegion:", (region,)) };
	Some(edges.left)
}
