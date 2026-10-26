//! Tests of the windows' Wayland glue against the fake libwayland: which
//! windows our presenter shows frames in, when it opens and what a failure
//! to open leaves behind, and what each frame shown leads to.
use super::{describe, handles, open, present};
use crate::{
	app::renderer::Shown,
	present::wayland::{
		client::{Client, Proxy},
		outside_tests::{
			FAKE_DISPLAY, fake_broken, fake_client, fake_damage, fake_leaked, fake_lose,
			fake_refuse_pools, fake_release_all, fake_shown, fake_surface, fake_without_xrgb,
		},
		presenter::{Error, Presenter},
	},
};
use std::{cell::Cell, ffi::c_void, ptr::NonNull};
use winit::raw_window_handle::{
	RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle, XcbDisplayHandle,
	XcbWindowHandle, XlibDisplayHandle, XlibWindowHandle,
};

const WIDTH: usize = 4;

fn pointer(address: usize) -> NonNull<c_void> {
	NonNull::new(address as *mut c_void).unwrap()
}

/// A presenter on a fresh fake compositor, for a surface of version 6.
fn opened() -> Presenter {
	let client = fake_client();
	let surface = fake_surface(6);
	// SAFETY: the fake's display and surface live as long as the thread.
	unsafe { open(Some((FAKE_DISPLAY, surface)), || Ok(client)) }.expect("a presenter")
}

/// `rows` rows of `WIDTH` pixels, each saying where it is and which frame.
fn frame(rows: usize, number: u32) -> Vec<u32> {
	let pixels = (0..rows * WIDTH).map(|index| number << 16 | index as u32);
	pixels.collect()
}

fn rules_kept() {
	let broken = fake_broken();
	assert!(broken.is_empty(), "{broken:#?}");
}

#[test]
fn a_wayland_window_is_shown_in_through_its_display_and_surface() {
	let window = WaylandWindowHandle::new(pointer(0x5000));
	let display = WaylandDisplayHandle::new(pointer(0xd000));
	let found = handles(window.into(), display.into());
	let expected = (0xd000 as Proxy, 0x5000 as Proxy);
	assert_eq!(found, Some(expected), "the display first, then the surface");
}

#[test]
fn x11_and_mismatched_windows_are_left_to_softbuffer() {
	let wayland_window: RawWindowHandle = WaylandWindowHandle::new(pointer(0x5000)).into();
	let wayland_display: RawDisplayHandle = WaylandDisplayHandle::new(pointer(0xd000)).into();
	let xlib_window: RawWindowHandle = XlibWindowHandle::new(0x4a0_0007).into();
	let xlib_display: RawDisplayHandle = XlibDisplayHandle::new(Some(pointer(0xd000)), 0).into();
	let xcb_window = XcbWindowHandle::new(0x4a0_0007.try_into().unwrap()).into();
	let xcb_display = XcbDisplayHandle::new(Some(pointer(0xd000)), 0).into();
	let pairs = [
		(xlib_window, xlib_display),
		(xcb_window, xcb_display),
		(wayland_window, xlib_display),
		(xlib_window, wayland_display),
	];
	for (window, display) in pairs {
		assert_eq!(handles(window, display), None, "{window:?} on {display:?}");
	}
}

#[test]
fn a_window_without_wayland_handles_never_loads_libwayland() {
	let loaded = Cell::new(false);
	let load = || {
		loaded.set(true);
		Err::<Client, _>("unreachable".to_owned())
	};
	// SAFETY: no handles, so nothing is used.
	assert!(unsafe { open(None, load) }.is_none());
	assert!(!loaded.get());
}

#[test]
fn a_missing_libwayland_leaves_the_window_to_softbuffer() {
	let surface = (fake_client(), fake_surface(6)).1;
	let load = || Err::<Client, _>("libwayland-client.so.0: not found".to_owned());
	// SAFETY: the fake's display and surface live as long as the thread.
	let presenter = unsafe { open(Some((FAKE_DISPLAY, surface)), load) };
	assert!(presenter.is_none());
	assert_eq!(fake_leaked(), 0, "nothing was made");
}

#[test]
fn a_compositor_without_our_pixel_format_leaves_nothing_behind() {
	let client = fake_client();
	let surface = fake_surface(6);
	fake_without_xrgb();
	// SAFETY: the fake's display and surface live as long as the thread.
	let presenter = unsafe { open(Some((FAKE_DISPLAY, surface)), || Ok(client)) };
	assert!(presenter.is_none());
	assert_eq!(fake_leaked(), 0, "wl_shm and the queue are gone");
	rules_kept();
}

#[test]
fn a_whole_frame_is_shown_at_the_height_its_pixels_make() {
	let mut presenter = opened();
	let pixels = frame(3, 1);
	assert_eq!(present(&mut presenter, &pixels, WIDTH, None), Shown::Done);
	assert_eq!(fake_shown(), [([WIDTH, 3], pixels)]);
	assert_eq!(fake_damage(), [vec![[0, 0, WIDTH as i32, 3]]]);
	rules_kept();
}

#[test]
fn pixels_past_the_last_whole_row_are_left_out() {
	let mut presenter = opened();
	let pixels = frame(3, 1);
	let shown = present(&mut presenter, &pixels[..2 * WIDTH + 3], WIDTH, None);
	assert_eq!(shown, Shown::Done);
	assert_eq!(fake_shown(), [([WIDTH, 2], pixels[..2 * WIDTH].to_vec())]);
	rules_kept();
}

#[test]
fn a_window_without_area_shows_nothing_and_asks_for_nothing() {
	// A redraw would find no area either: the resize that gives it one
	// draws again.
	let mut presenter = opened();
	assert_eq!(present(&mut presenter, &[], WIDTH, None), Shown::Done);
	assert_eq!(present(&mut presenter, &frame(3, 1), 0, None), Shown::Done);
	assert_eq!(
		present(&mut presenter, &frame(3, 1)[..3], WIDTH, None),
		Shown::Done
	);
	assert!(fake_shown().is_empty());
	rules_kept();
}

#[test]
fn a_partial_frame_damages_only_its_bands() {
	let mut presenter = opened();
	present(&mut presenter, &frame(3, 1), WIDTH, None);
	let second = frame(3, 2);
	assert_eq!(
		present(&mut presenter, &second, WIDTH, Some(&[(1, 2)])),
		Shown::Done
	);
	let shown = fake_shown();
	assert_eq!(shown.len(), 2);
	// A new buffer, as the compositor holds the first: copied whole.
	assert_eq!(shown[1], ([WIDTH, 3], second));
	assert_eq!(fake_damage()[1], [[0, 1, WIDTH as i32, 1]]);
	rules_kept();
}

#[test]
fn a_frame_with_every_buffer_held_is_tried_again_and_loses_nothing() {
	let mut presenter = opened();
	for number in 1..=3 {
		let shown = present(&mut presenter, &frame(3, number), WIDTH, None);
		assert_eq!(shown, Shown::Done, "buffer {number}");
	}
	let fourth = frame(3, 4);
	assert_eq!(
		present(&mut presenter, &fourth, WIDTH, Some(&[(0, 1)])),
		Shown::Retry
	);
	assert_eq!(fake_shown().len(), 3, "nothing committed");
	fake_release_all();
	// The retried frame changed another row: both are damaged.
	let fifth = frame(3, 5);
	assert_eq!(
		present(&mut presenter, &fifth, WIDTH, Some(&[(2, 3)])),
		Shown::Done
	);
	// Row 1, unchanged since frame 3, comes from the buffer holding it.
	let mut expected = fifth;
	expected[WIDTH..2 * WIDTH].copy_from_slice(&frame(3, 3)[WIDTH..2 * WIDTH]);
	assert_eq!(fake_shown().last(), Some(&([WIDTH, 3], expected)));
	let damage = fake_damage().pop().unwrap();
	assert_eq!(damage, [[0, 0, WIDTH as i32, 1], [0, 2, WIDTH as i32, 1]]);
	rules_kept();
}

#[test]
fn a_broken_connection_fails_with_its_reason() {
	let mut presenter = opened();
	fake_lose();
	let shown = present(&mut presenter, &frame(3, 1), WIDTH, None);
	assert_eq!(shown, Shown::Failed(describe(Error::Lost)));
	assert!(fake_shown().is_empty());
}

#[test]
fn buffers_that_cannot_be_made_fail_the_frame() {
	let mut presenter = opened();
	fake_refuse_pools();
	let shown = present(&mut presenter, &frame(3, 1), WIDTH, None);
	assert!(matches!(shown, Shown::Failed(_)), "{shown:?}");
	assert!(fake_shown().is_empty());
	drop(presenter);
	assert_eq!(fake_leaked(), 0);
	rules_kept();
}

#[test]
fn a_presenter_dropped_after_frames_leaves_nothing_behind() {
	let mut presenter = opened();
	present(&mut presenter, &frame(3, 1), WIDTH, None);
	present(&mut presenter, &frame(2, 2), WIDTH, None);
	drop(presenter);
	assert_eq!(fake_leaked(), 0);
	rules_kept();
}

#[test]
fn each_failure_says_what_went_wrong() {
	assert_eq!(
		describe(Error::NoShm),
		"the compositor offers no wl_shm with XRGB8888 pixels"
	);
	assert_eq!(
		describe(Error::Lost),
		"the connection to the compositor broke"
	);
	assert_eq!(
		describe(Error::Memory("4×3 pixels: out of memory".to_owned())),
		"no shared memory for 4×3 pixels: out of memory"
	);
}
