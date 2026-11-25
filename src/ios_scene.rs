//! The scene lifecycle iOS 27 requires, which winit 0.30 predates: UIKit
//! makes the scene and calls our scene delegate when it connects, and each
//! window winit makes is put in it (`scene_pairing` decides when). Without
//! a scene the app ran as an old one, fixed in size: on an iPhone Duo open,
//! a phone-sized window between black bars.
// Calls into C or the kernel; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
use std::{ffi::c_void, sync::Mutex};

use winit::{
	raw_window_handle::{HasWindowHandle, RawWindowHandle},
	window::Window,
};

use crate::{
	objc::{self, Object, Raw, Sel},
	scene_pairing::Pairing,
};

/// The class `Info.plist` names in `UISceneDelegateClassName`.
const DELEGATE: &std::ffi::CStr = c"PooptermSceneDelegate";

/// Windows (`UIWindow`) and the scene (`UIWindowScene`), as addresses:
/// both are made, used and dropped on the main thread only.
static PAIRING: Mutex<Pairing<usize, usize>> = Mutex::new(Pairing::new());

/// Registers the scene delegate class, before UIKit starts.
pub(crate) fn register() {
	let methods = [
		objc::Method {
			selector: c"scene:willConnectToSession:options:",
			imp: will_connect as *const c_void,
			types: c"v@:@@@",
		},
		objc::Method {
			selector: c"sceneDidDisconnect:",
			imp: did_disconnect as *const c_void,
			types: c"v@:@",
		},
	];
	// SAFETY: `will_connect` is `extern "C" fn(Raw, Sel, Raw, Raw, Raw)`,
	// returning nothing and taking three objects (`v@:@@@`), and
	// `did_disconnect` takes one (`v@:@`).
	let made =
		unsafe { objc::subclass(DELEGATE, c"UIResponder", c"UIWindowSceneDelegate", &methods) };
	if let Err(missing) = made {
		crate::warnings::warn(format_args!("iOS scenes: {missing}"));
	}
}

/// Puts winit's `window` in the scene, now or once it connects.
pub(crate) fn adopt(window: &Window) {
	let Ok(handle) = window.window_handle() else {
		return;
	};
	let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
		return;
	};
	// SAFETY: the handle's view is winit's live `UIView`, whose `window`
	// returns its `UIWindow`, or nil.
	let ui_window: Raw =
		unsafe { objc::Arguments::send((), handle.ui_view.as_ptr(), objc::sel(c"window")) };
	if ui_window.is_null() {
		return;
	}
	let pairs = lock().window(ui_window as usize);
	place(pairs);
}

fn lock() -> std::sync::MutexGuard<'static, Pairing<usize, usize>> {
	PAIRING.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// The delegate's `scene:willConnectToSession:options:`.
extern "C" fn will_connect(_: Raw, _: Sel, scene: Raw, _: Raw, _: Raw) {
	// SAFETY: UIKit passes the connecting `UIWindowScene`; it is retained
	// for as long as it is the scene windows go in, and never released,
	// as the app has only this one.
	let Some(scene) = (unsafe { Object::retained(scene) }) else {
		return;
	};
	let raw = scene.raw();
	std::mem::forget(scene);
	let pairs = lock().scene(raw as usize);
	place(pairs);
}

/// The delegate's `sceneDidDisconnect:`.
extern "C" fn did_disconnect(_: Raw, _: Sel, _: Raw) {
	lock().disconnect();
}

/// Puts each window in its scene and shows it; UIKit sizes it to the scene.
fn place(pairs: Vec<(usize, usize)>) {
	for (window, scene) in pairs {
		let (window, scene) = (window as Raw, scene as Raw);
		// SAFETY: `window` is a live `UIWindow` and `scene` the retained
		// `UIWindowScene`; `setWindowScene:` takes the scene.
		unsafe { objc::Arguments::send::<()>((scene,), window, objc::sel(c"setWindowScene:")) };
		// The root view controller learns of its new scene (orientation, safe
		// area) only when set again, as winit does for windows made early.
		// SAFETY: `rootViewController` takes nothing and returns the
		// window's controller or nil, kept alive by the window.
		let controller: Raw =
			unsafe { objc::Arguments::send((), window, objc::sel(c"rootViewController")) };
		// SAFETY: the controller is retained while it is detached, and given
		// back: `setRootViewController:` takes a controller or nil.
		if let Some(controller) = unsafe { Object::retained(controller) } {
			let set = objc::sel(c"setRootViewController:");
			// SAFETY: as above, nil then the same controller.
			unsafe { objc::Arguments::send::<()>((objc::NIL,), window, set) };
			// SAFETY: as above.
			unsafe { objc::Arguments::send::<()>((controller.raw(),), window, set) };
		}
		// SAFETY: `makeKeyAndVisible` takes nothing.
		unsafe { objc::Arguments::send::<()>((), window, objc::sel(c"makeKeyAndVisible")) };
	}
}
