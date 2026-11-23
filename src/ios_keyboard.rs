//! Where the on-screen keyboard is (iOS), so the terminal stays above it:
//! an observer of UIKit's keyboard notifications reads the frame the
//! keyboard is moving to, keeps its top edge in screen points (what it
//! covers depends on where the window is: `ui::insets::window_insets`),
//! and wakes the event loop to lay the window out again. It also keeps the
//! keyboard from rewriting what is typed (`input::typing`), and passes a
//! hardware keyboard's presses with modifiers on as keys
//! (`input::hardware_keys`), which winit would lose.
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

/// Turns off what the keyboard does to typed text (smart quotes and
/// dashes, correction, capitals) by answering winit's view's
/// `UITextInputTraits` getters, which it leaves to UIKit's defaults.
/// Called once, before the view first takes the keyboard.
pub(crate) fn plain_typing() {
	use crate::input::typing::{NONE, TRAITS};
	let methods: Vec<objc::Method> = TRAITS
		.iter()
		.map(|&(selector, value)| objc::Method {
			selector,
			imp: if value == NONE {
				none as *const _
			} else {
				no as *const _
			},
			types: c"q@:",
		})
		.collect();
	// SAFETY: `none` and `no` take the receiver and selector and return an
	// `NSInteger`, as `q@:` encodes.
	let _ = unsafe { objc::add_methods(c"WinitUIView", &methods) };
	let presses = [
		objc::Method {
			selector: c"pressesBegan:withEvent:",
			imp: presses_began as *const _,
			types: c"v@:@@",
		},
		objc::Method {
			selector: c"pressesEnded:withEvent:",
			imp: presses_ended as *const _,
			types: c"v@:@@",
		},
	];
	// SAFETY: both take the receiver, selector, a set of presses and an
	// event, and return nothing, as `v@:@@` encodes.
	let _ = unsafe { objc::add_methods(c"WinitUIView", &presses) };
}

/// The view's `pressesBegan:withEvent:`: keys taken (`hardware_keys`) go
/// to the app; the rest, and every press when none is, to UIKit.
extern "C" fn presses_began(view: Raw, _: Sel, presses: Raw, event: Raw) {
	let taken = taken_keys(presses);
	if taken.is_empty() {
		// SAFETY: `view` is a `WinitUIView`, a `UIResponder`, which answers
		// `pressesBegan:withEvent:` with a set and an event.
		unsafe {
			objc::send_super(
				view,
				c"WinitUIView",
				c"pressesBegan:withEvent:",
				(presses, event),
			)
		};
		return;
	}
	let proxy = PROXY.lock().unwrap_or_else(|poison| poison.into_inner());
	for (key, mods) in taken {
		if let Some(proxy) = proxy.as_ref() {
			let _ = proxy.send_event(UserEvent::HardwareKey(key, mods));
		}
	}
}

/// The view's `pressesEnded:withEvent:`: UIKit sees the end of the presses
/// it saw begin.
extern "C" fn presses_ended(view: Raw, _: Sel, presses: Raw, event: Raw) {
	if taken_keys(presses).is_empty() {
		// SAFETY: as in `presses_began`, for `pressesEnded:withEvent:`.
		unsafe {
			objc::send_super(
				view,
				c"WinitUIView",
				c"pressesEnded:withEvent:",
				(presses, event),
			)
		};
	}
}

/// The keys the app takes of `presses`, an `NSSet` of `UIPress`.
fn taken_keys(presses: Raw) -> Vec<(winit::keyboard::Key, crate::input::keys::Modifiers)> {
	let mut taken = Vec::new();
	// SAFETY: UIKit passes a set of presses, alive through the call; nil
	// is checked. `allObjects` returns an array the caller does not own.
	let Some(all) = (unsafe { objc::Object::retained(presses) }) else {
		return taken;
	};
	// SAFETY: an `NSSet` answers `allObjects` with an `NSArray`.
	let array: Raw = unsafe { all.send(c"allObjects", ()) };
	// SAFETY: the array is live; `count` returns an `NSUInteger`.
	let Some(array) = (unsafe { objc::Object::retained(array) }) else {
		return taken;
	};
	// SAFETY: an `NSArray` answers `count`.
	let count: usize = unsafe { array.send(c"count", ()) };
	for index in 0..count {
		// SAFETY: `index` is below `count`; the press is a `UIPress`.
		let press: Raw = unsafe { array.send(c"objectAtIndex:", (index,)) };
		if let Some(key) = press_key(press) {
			taken.push(key);
		}
	}
	taken
}

/// The key the app takes for one `UIPress`, if it takes it.
fn press_key(press: Raw) -> Option<(winit::keyboard::Key, crate::input::keys::Modifiers)> {
	// SAFETY: `press` is a live `UIPress` from the set; `key` returns its
	// `UIKey` (iOS 13.4) or nil, which is checked.
	let press = unsafe { objc::Object::retained(press) }?;
	if !press.responds(c"key") {
		return None;
	}
	// SAFETY: the press answers `key`, as checked.
	let key: Raw = unsafe { press.send(c"key", ()) };
	// SAFETY: a `UIKey` or nil.
	let key = unsafe { objc::Object::retained(key) }?;
	// SAFETY: a `UIKey` answers `modifierFlags` with an `NSInteger`.
	let flags: isize = unsafe { key.send(c"modifierFlags", ()) };
	// SAFETY: and `keyCode` with an `NSInteger` (`UIKeyboardHIDUsage`).
	let usage: isize = unsafe { key.send(c"keyCode", ()) };
	// SAFETY: and `charactersIgnoringModifiers` with an `NSString`.
	let characters: Raw = unsafe { key.send(c"charactersIgnoringModifiers", ()) };
	// SAFETY: a string or nil.
	let characters = unsafe { objc::Object::retained(characters) };
	// SAFETY: the string is an `NSString`.
	let text = characters
		.map(|string| unsafe { objc::text(&string) })
		.unwrap_or_default();
	crate::input::hardware_keys::press(flags, &text, usage)
}

extern "C" fn none(_: Raw, _: Sel) -> isize {
	crate::input::typing::NONE
}

extern "C" fn no(_: Raw, _: Sel) -> isize {
	crate::input::typing::NO
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
