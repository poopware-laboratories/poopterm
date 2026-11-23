//! Where the on-screen keyboard is (iOS), so the terminal stays above it:
//! an observer of UIKit's keyboard notifications reads the frame the
//! keyboard is moving to, keeps its top edge in screen points (what it
//! covers depends on where the window is: `ui::insets::window_insets`),
//! and wakes the event loop to lay the window out again.
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

/// The keyboard's top edge in screen points, as `f64` bits; infinite while
/// none has shown.
static TOP: AtomicU64 = AtomicU64::new(f64::INFINITY.to_bits());
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

/// The keyboard's top edge in screen points now; off the bottom of the
/// screen when it is away.
pub(crate) fn top() -> f64 {
	f64::from_bits(TOP.load(Ordering::Relaxed))
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
	if let Some(top) = keyboard_top(notification) {
		TOP.store(top.to_bits(), Ordering::Relaxed);
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
