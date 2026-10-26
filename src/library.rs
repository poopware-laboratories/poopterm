//! A shared library opened at run time: the Vulkan loader, libwayland-client.
use std::{
	ffi::{CStr, c_void},
	ptr::NonNull,
};

/// An open library, closed when dropped.
pub(crate) struct Library(NonNull<c_void>);

impl Library {
	/// The library `name`, or why the dynamic linker could not open it.
	pub(crate) fn open(name: &CStr) -> Result<Self, String> {
		// SAFETY: `name` is a C string. Opening runs the library's
		// initializers, which for these libraries is what loading them is for.
		// Binding every symbol now makes a broken library fail here, not later.
		let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
		NonNull::new(handle).map(Self).ok_or_else(last_error)
	}

	/// The address of the symbol `name`, if the library has it.
	pub(crate) fn symbol(&self, name: &CStr) -> Option<NonNull<c_void>> {
		// SAFETY: the handle stays open while `self` lives, and `name` is a
		// C string.
		NonNull::new(unsafe { libc::dlsym(self.0.as_ptr(), name.as_ptr()) })
	}
}

impl Drop for Library {
	fn drop(&mut self) {
		// SAFETY: the handle is open, and nothing taken from the library
		// outlives `self` (an instance owns the entry that owns this, and a
		// Wayland client the functions it took).
		unsafe { libc::dlclose(self.0.as_ptr()) };
	}
}

/// Why the last `dlopen` on this thread failed.
fn last_error() -> String {
	// SAFETY: `dlerror` returns null or a C string that stays valid until the
	// next call on this thread; it is copied at once.
	let message = unsafe { libc::dlerror() };
	if message.is_null() {
		return "unknown error".into();
	}
	// SAFETY: as above, a C string.
	unsafe { CStr::from_ptr(message) }
		.to_string_lossy()
		.into_owned()
}
