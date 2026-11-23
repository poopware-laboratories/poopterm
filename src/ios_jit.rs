//! The iOS side of `jit enable`: the app's bundle identifier, and opening
//! StikDebug's link, which UIKit takes only on the main thread.
// Calls into Foundation, UIKit and libdispatch; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
use std::ffi::c_void;

use crate::objc::{self, Object, Pool, Raw};

unsafe extern "C" {
	/// The main queue (`dispatch_get_main_queue()` is a macro for it).
	static _dispatch_main_q: c_void;
	fn dispatch_async_f(queue: *const c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
}

/// The app's bundle identifier.
pub(crate) fn bundle_identifier() -> Option<String> {
	let _pool = Pool::new();
	let class = objc::class(c"NSBundle").ok()?;
	// SAFETY: `mainBundle` returns the app's bundle, autoreleased into the pool.
	let bundle: Raw = unsafe { class.send(c"mainBundle", ()) };
	// SAFETY: the bundle lives at least as long as the pool.
	let bundle = unsafe { Object::retained(bundle) }?;
	// SAFETY: `bundleIdentifier` returns an NSString or nil.
	let identifier: Raw = unsafe { bundle.send(c"bundleIdentifier", ()) };
	// SAFETY: as above, autoreleased.
	let identifier = unsafe { Object::retained(identifier) }?;
	// SAFETY: an NSString.
	Some(unsafe { objc::text(&identifier) })
}

/// Opens `url` with the app that handles its scheme, from the main thread.
pub(crate) fn open_url(url: &str) {
	let context = Box::into_raw(Box::new(url.to_owned()));
	// SAFETY: the main queue lives for the app's life; `open_on_main` takes
	// back the box it is given, once.
	unsafe { dispatch_async_f(&raw const _dispatch_main_q, context.cast(), open_on_main) };
}

extern "C" fn open_on_main(context: *mut c_void) {
	// SAFETY: the box `open_url` leaked for this call.
	let url = unsafe { Box::from_raw(context.cast::<String>()) };
	let _pool = Pool::new();
	let (Some(text), Ok(urls), Ok(apps), Ok(dictionaries)) = (
		objc::string(&url),
		objc::class(c"NSURL"),
		objc::class(c"UIApplication"),
		objc::class(c"NSDictionary"),
	) else {
		return;
	};
	// SAFETY: `URLWithString:` takes an NSString and returns an NSURL or nil.
	let link: Raw = unsafe { urls.send(c"URLWithString:", (text.raw(),)) };
	// SAFETY: the app object, on the main thread.
	let app: Raw = unsafe { apps.send(c"sharedApplication", ()) };
	// SAFETY: an empty, autoreleased dictionary.
	let options: Raw = unsafe { dictionaries.send(c"dictionary", ()) };
	if link.is_null() || app.is_null() {
		return;
	}
	// SAFETY: `openURL:options:completionHandler:` takes an NSURL, an
	// NSDictionary and a block, nil here; called on the main thread.
	unsafe {
		objc::Arguments::send::<()>(
			(link, options, std::ptr::null_mut::<c_void>()),
			app,
			objc::sel(c"openURL:options:completionHandler:"),
		)
	};
}
