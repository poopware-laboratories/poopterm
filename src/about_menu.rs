//! The macOS app menu's About item, pointed at our own About page instead
//! of AppKit's generic panel, so both show the same name, version,
//! codename, notices and links.
// Calls into C or the kernel; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
// Messages still share blocks until the Metal renderer work wraps each.
#![expect(clippy::multiple_unsafe_ops_per_block)]
use crate::objc::{self, Object, Raw, Sel, sel};
use std::{
	ffi::CStr,
	sync::{Mutex, OnceLock},
};

/// What winit's About item sends to show AppKit's panel.
const STANDARD: &CStr = c"orderFrontStandardAboutPanel:";
/// What it sends once it is ours.
const OURS: &CStr = c"pooptermShowAbout:";

/// What choosing the item runs: the app's request to show its page.
type Wake = Box<dyn Fn() + Send>;

static WAKE: Mutex<Option<Wake>> = Mutex::new(None);

fn set_wake(wake: Option<Wake>) {
	*WAKE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = wake;
}

/// Our action: runs the installed callback, if any.
extern "C" fn show_about(_receiver: Raw, _selector: Sel, _sender: Raw) {
	if let Some(wake) = &*WAKE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) {
		wake();
	}
}

/// The object the item sends our action to, made once. Menu items do not
/// retain their targets, so it lives for the whole process.
fn target() -> Option<Raw> {
	static TARGET: OnceLock<Option<usize>> = OnceLock::new();
	let made = TARGET.get_or_init(|| {
		let class = objc::action_class(c"PooptermAboutTarget", OURS, show_about).ok()?;
		// SAFETY: the class descends from `NSObject` and `init` takes nothing.
		let target = unsafe { class.instance() }?;
		let raw = target.raw();
		std::mem::forget(target);
		Some(raw as usize)
	});
	made.map(|address| address as Raw)
}

/// Points every item of `menu` and its submenus that shows AppKit's About
/// panel, or already ours, at our page; whether there was one.
///
/// # Safety
///
/// `menu` is an `NSMenu`.
unsafe fn redirect(menu: &Object) -> bool {
	let Some(target) = target() else {
		return false;
	};
	let (standard, ours) = (sel(STANDARD), sel(OURS));
	// SAFETY: an `NSMenu` answers `numberOfItems` with an `NSInteger`.
	let count: isize = unsafe { menu.send(c"numberOfItems", ()) };
	let mut found = false;
	for index in 0..count {
		// SAFETY: `itemAtIndex:` takes an index below the count and returns
		// an `NSMenuItem`, which answers `action`, `submenu`, `setTarget:`
		// and `setAction:` as called.
		unsafe {
			let Some(item) = Object::retained(menu.send(c"itemAtIndex:", (index,))) else {
				continue;
			};
			let action: Sel = item.send(c"action", ());
			if action == standard || action == ours {
				let () = item.send(c"setTarget:", (target,));
				let () = item.send(c"setAction:", (ours,));
				found = true;
			}
			if let Some(submenu) = Object::retained(item.send(c"submenu", ())) {
				found |= redirect(&submenu);
			}
		}
	}
	found
}

/// Points the app menu's About item at `show`, which should open our
/// About page; whether the menu had one. Call it on the main thread once
/// the application has launched and built its menu.
pub(crate) fn show_about_with(show: impl Fn() + Send + 'static) -> bool {
	set_wake(Some(Box::new(show)));
	let Ok(application) = objc::class(c"NSApplication") else {
		return false;
	};
	// SAFETY: `sharedApplication` returns the application, on the main
	// thread as the caller vouches, and `mainMenu` its `NSMenu` or nil.
	unsafe {
		let application: Raw = application.send(c"sharedApplication", ());
		let menu: Raw = objc::Arguments::send((), application, sel(c"mainMenu"));
		Object::retained(menu).is_some_and(|menu| redirect(&menu))
	}
}

#[cfg(test)]
#[path = "about_menu_tests.rs"]
mod tests;
