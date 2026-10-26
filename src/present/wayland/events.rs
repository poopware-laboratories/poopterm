//! What the events on the presenter's queue say: the compositor's globals,
//! whether `wl_shm` offers our pixel format, and which buffers it released.
//! `dispatch` records them while libwayland dispatches our queue; the
//! presenter reads them between dispatches.
use super::client::{Argument, Client, Proxy};
use std::{
	ffi::{CStr, c_int, c_void},
	ptr::{self, NonNull},
};

/// `WL_SHM_FORMAT_XRGB8888`: our pixels, `0x00RRGGBB` in native order.
/// Every compositor must offer it.
pub(super) const XRGB8888: u32 = 1;

#[derive(Default)]
pub(super) struct Events {
	pub registry: Proxy,
	pub shm: Proxy,
	/// Each global's name, interface and version.
	pub globals: Vec<(u32, Vec<u8>, u32)>,
	pub xrgb: bool,
	pub released: Vec<Proxy>,
}

/// The `Events` our proxies' dispatcher writes, at an address that stays
/// put while they point to it.
pub(super) struct Listener(NonNull<Events>);

impl Listener {
	pub(super) fn new() -> Self {
		Self(NonNull::from(Box::leak(Box::default())))
	}

	/// Dispatches `proxy`'s events into these.
	///
	/// # Safety
	///
	/// The proxy is live, on a queue only dispatched from this thread, and
	/// destroyed before the listener drops.
	pub(super) unsafe fn listen(&self, client: &Client, proxy: Proxy) {
		let data = self.0.as_ptr().cast_const().cast();
		// SAFETY: as the caller vouches; `dispatch` takes `Events`.
		unsafe { (client.add_dispatcher)(proxy, dispatch, data, ptr::null_mut()) };
	}

	/// The events so far. Never held across a dispatch of our queue.
	pub(super) fn events(&mut self) -> &mut Events {
		// SAFETY: `dispatch` writes them only while our queue is dispatched,
		// which no caller does while borrowing them.
		unsafe { self.0.as_mut() }
	}
}

impl Drop for Listener {
	fn drop(&mut self) {
		// SAFETY: leaked by `new`, and no proxy dispatches here any more.
		drop(unsafe { Box::from_raw(self.0.as_ptr()) });
	}
}

/// Records each event on our proxies in the `Events` at `data`.
unsafe extern "C" fn dispatch(
	data: *const c_void,
	target: Proxy,
	opcode: u32,
	_message: *const c_void,
	arguments: *mut Argument,
) -> c_int {
	// SAFETY: `data` is a listener's `Events`, which nothing else borrows
	// while our queue dispatches; the arguments follow each event's
	// signature in wayland.xml.
	unsafe {
		let events = &mut *data.cast_mut().cast::<Events>();
		let argument = |index| *arguments.add(index);
		if target == events.registry && opcode == 0 {
			// global: name, interface, version.
			let interface = CStr::from_ptr(argument(1).string).to_bytes().to_vec();
			events
				.globals
				.push((argument(0).uint, interface, argument(2).uint));
		} else if target == events.shm && opcode == 0 {
			// format.
			events.xrgb |= argument(0).uint == XRGB8888;
		} else if target != events.registry && target != events.shm && opcode == 0 {
			// A buffer's release.
			events.released.push(target);
		}
	}
	0
}
