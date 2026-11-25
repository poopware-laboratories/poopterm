//! Paths inside the app's writable iOS container.
// Calls into Foundation; docs/unsafe.md lists each block.
#![allow(unsafe_code)]
use std::{
	ffi::{CStr, c_char},
	path::PathBuf,
};

use crate::objc::{Object, Raw};

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
	fn NSHomeDirectory() -> Raw;
}

/// Returns Foundation's sandbox home, not an assumed environment path.
pub(crate) fn home_dir() -> Option<PathBuf> {
	// SAFETY: Foundation returns the current process's home as an NSString.
	let raw = unsafe { NSHomeDirectory() };
	// SAFETY: NSHomeDirectory returns an autoreleased NSString or nil.
	let home = unsafe { Object::retained(raw) }?;
	// SAFETY: NSString's UTF8String is a NUL-terminated pointer valid while `home` lives.
	let bytes: *const c_char = unsafe { home.send(c"UTF8String", ()) };
	if bytes.is_null() {
		return None;
	}
	// SAFETY: UTF8String returns a valid NUL-terminated representation for this live NSString.
	let path = unsafe { CStr::from_ptr(bytes) }.to_str().ok()?;
	let path = PathBuf::from(path);
	path.is_absolute().then_some(path)
}
