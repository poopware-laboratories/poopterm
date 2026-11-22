//! Where the on-screen keyboard is (iOS), so the terminal stays above it:
//! an observer of UIKit's keyboard notifications reads the frame the
//! keyboard is moving to, keeps how many points of the screen's bottom it
//! covers, and wakes the event loop to lay the window out again.
// Calls into C or the kernel; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
use std::sync::{
	Mutex,
	atomic::{AtomicU64, Ordering},
};

use winit::event_loop::EventLoopProxy;

use crate::{
	objc::{self, Raw, Sel},
	pty::UserEvent,
};

/// Points of the screen's bottom the keyboard covers, as `f64` bits.
static COVERED: AtomicU64 = AtomicU64::new(0);
/// Where the observer sends word of a change.
static PROXY: Mutex<Option<EventLoopProxy<UserEvent>>> = Mutex::new(None);

/// `CGRect`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Rect {
	x: f64,
	y: f64,
	width: f64,
	height: f64,
}

/// Points of the screen's bottom the keyboard covers now.
pub(crate) fn covered() -> f64 {
	f64::from_bits(COVERED.load(Ordering::Relaxed))
}

/// Starts following the keyboard; each change sends `UserEvent::Keyboard`.
/// Called once, on the main thread, as the window opens.
pub(crate) fn observe(proxy: EventLoopProxy<UserEvent>) {
	*PROXY.lock().unwrap_or_else(|poison| poison.into_inner()) = Some(proxy);
	let Ok(class) = objc::action_class(c"PooptermKeyboardObserver", c"keyboardChanged:", changed)
	else {
		return;
	};
	// SAFETY: the class descends from `NSObject` and `init` takes nothing.
	let Some(observer) = (unsafe { class.instance() }) else {
		return;
	};
	let Ok(center) = objc::class(c"NSNotificationCenter") else {
		return;
	};
	// SAFETY: `defaultCenter` takes nothing and returns the shared center.
	let center: Raw = unsafe { center.send(c"defaultCenter", ()) };
	for name in [
		"UIKeyboardWillChangeFrameNotification",
		"UIKeyboardWillHideNotification",
	] {
		let Some(name) = objc::string(name) else {
			continue;
		};
		// SAFETY: the center is live; `addObserver:selector:name:object:`
		// takes an observer, a selector it answers (`v@:@`), a name and nil
		// for any sender, and keeps the observer unretained, so it is kept
		// alive below for the life of the app.
		unsafe {
			objc::Arguments::send::<()>(
				(
					observer.raw(),
					objc::sel(c"keyboardChanged:"),
					name.raw(),
					std::ptr::null_mut::<std::ffi::c_void>(),
				),
				center,
				objc::sel(c"addObserver:selector:name:object:"),
			)
		};
	}
	std::mem::forget(observer);
}

/// The observer's method: a keyboard notification arrived.
extern "C" fn changed(_: Raw, _: Sel, notification: Raw) {
	let top = keyboard_top(notification);
	let screen = screen_height();
	if let (Some(top), Some(screen)) = (top, screen) {
		COVERED.store(
			crate::ui::insets::covered_points(screen, top).to_bits(),
			Ordering::Relaxed,
		);
	}
	let proxy = PROXY.lock().unwrap_or_else(|poison| poison.into_inner());
	if let Some(proxy) = proxy.as_ref() {
		let _ = proxy.send_event(UserEvent::Keyboard);
	}
}

/// The top of the frame the keyboard is moving to, in screen points.
fn keyboard_top(notification: Raw) -> Option<f64> {
	if notification.is_null() {
		return None;
	}
	let key = objc::string("UIKeyboardFrameEndUserInfoKey")?;
	// SAFETY: a keyboard notification is an `NSNotification`, whose
	// `userInfo` is a dictionary or nil.
	let info: Raw = unsafe { objc::Arguments::send((), notification, objc::sel(c"userInfo")) };
	if info.is_null() {
		return None;
	}
	// SAFETY: `objectForKey:` takes a key and returns the value or nil.
	let value: Raw =
		unsafe { objc::Arguments::send((key.raw(),), info, objc::sel(c"objectForKey:")) };
	if value.is_null() {
		return None;
	}
	// SAFETY: the frame's value is an `NSValue` holding a `CGRect`, which
	// `CGRectValue` returns.
	let frame: Rect = unsafe { objc::Arguments::send((), value, objc::sel(c"CGRectValue")) };
	Some(frame.y)
}

/// The main screen's height in points.
fn screen_height() -> Option<f64> {
	let screen = objc::class(c"UIScreen").ok()?;
	// SAFETY: `mainScreen` takes nothing and returns the screen.
	let main: Raw = unsafe { screen.send(c"mainScreen", ()) };
	if main.is_null() {
		return None;
	}
	// SAFETY: `bounds` takes nothing and returns a `CGRect`.
	let bounds: Rect = unsafe { objc::Arguments::send((), main, objc::sel(c"bounds")) };
	Some(bounds.height)
}
