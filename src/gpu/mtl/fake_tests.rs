//! A fake `MTLDevice`, a class of our own registered with the runtime: it
//! makes queues, names, buffers and textures, or refuses each when told,
//! and records what it was asked to make, so the bindings' failure paths
//! and the descriptors they fill are tested without a GPU's say.
use super::device::Device;
use crate::objc::{
	Object, Raw, Sel, class,
	probe_tests::{Weak, class_named, register},
	string,
};
use std::{
	cell::{Cell, RefCell},
	ffi::{CStr, c_void},
	sync::OnceLock,
};

/// What the fake refuses, as bits.
pub(crate) const QUEUE: u32 = 1;
pub(crate) const NAME: u32 = 2;
pub(crate) const BUFFER: u32 = 4;
pub(crate) const TEXTURE: u32 = 8;

/// The fake's name.
pub(crate) const FAKE: &str = "Poopterm Fake GPU";

/// What the fake was asked to make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Made {
	Buffer {
		length: usize,
		options: usize,
	},
	/// A texture of a descriptor's kind, pixel format, width, height,
	/// layers, storage mode and usage.
	Texture {
		kind: usize,
		format: usize,
		width: usize,
		height: usize,
		layers: usize,
		storage: usize,
		usage: usize,
	},
}

thread_local! {
	static REFUSED: Cell<u32> = const { Cell::new(0) };
	static MADE: RefCell<Vec<Made>> = const { RefCell::new(Vec::new()) };
	/// Weak references to what the fake was passed or returned unowned.
	static PASSED: RefCell<Vec<Weak>> = const { RefCell::new(Vec::new()) };
}

/// Takes weak references to what the fake was passed (descriptors) or
/// returned without handing it over (names), on this thread.
pub(crate) fn passed() -> Vec<Weak> {
	PASSED.with(|passed| std::mem::take(&mut *passed.borrow_mut()))
}

/// Keeps a weak reference to `raw`, a live object.
fn watch(raw: Raw) {
	// SAFETY: as said.
	let weak = unsafe { Weak::of(raw) };
	PASSED.with(|passed| passed.borrow_mut().push(weak));
}

/// Takes what the fake made on this thread.
pub(crate) fn made() -> Vec<Made> {
	MADE.with(|made| std::mem::take(&mut *made.borrow_mut()))
}

/// A new object, owned by the caller, for what the fake makes.
fn anything() -> Raw {
	let object = class(c"NSObject").unwrap();
	// SAFETY: `new` returns an object the caller owns, handed over raw.
	let made = unsafe { object.instance() }.unwrap();
	let raw = made.raw();
	std::mem::forget(made);
	raw
}

fn refuses(what: u32) -> bool {
	REFUSED.with(Cell::get) & what != 0
}

extern "C" fn new_queue(_: Raw, _: Sel) -> Raw {
	if refuses(QUEUE) {
		std::ptr::null_mut()
	} else {
		anything()
	}
}

extern "C" fn name(_: Raw, _: Sel) -> Raw {
	if refuses(NAME) {
		return std::ptr::null_mut();
	}
	let made = string(FAKE).unwrap();
	// SAFETY: `autorelease` hands the string's reference to the caller's
	// pool, so it is returned without being owned, as `name` returns it.
	unsafe {
		let raw: Raw = made.send(c"autorelease", ());
		std::mem::forget(made);
		watch(raw);
		raw
	}
}

extern "C" fn new_buffer(_: Raw, _: Sel, length: usize, options: usize) -> Raw {
	MADE.with(|made| made.borrow_mut().push(Made::Buffer { length, options }));
	if refuses(BUFFER) {
		std::ptr::null_mut()
	} else {
		anything()
	}
}

extern "C" fn new_texture(_: Raw, _: Sel, descriptor: Raw) -> Raw {
	// SAFETY: an `MTLTextureDescriptor`, whose getters take nothing and
	// return `NSUInteger`s or enums of that size.
	let read = |getter: &CStr| unsafe {
		let descriptor = Object::retained(descriptor).unwrap();
		descriptor.send::<usize>(getter, ())
	};
	let texture = Made::Texture {
		kind: read(c"textureType"),
		format: read(c"pixelFormat"),
		width: read(c"width"),
		height: read(c"height"),
		layers: read(c"arrayLength"),
		storage: read(c"storageMode"),
		usage: read(c"usage"),
	};
	MADE.with(|made| made.borrow_mut().push(texture));
	watch(descriptor);
	if refuses(TEXTURE) {
		std::ptr::null_mut()
	} else {
		anything()
	}
}

/// The fake device, refusing `refused` (bits of the constants above) on
/// this thread from now on.
pub(crate) fn fake_device(refused: u32) -> Object {
	static FAKE_DEVICE: OnceLock<usize> = OnceLock::new();
	let raw = *FAKE_DEVICE.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 4] = [
			(c"newCommandQueue", new_queue as *const c_void, c"@@:"),
			(c"name", name as *const c_void, c"@@:"),
			(
				c"newBufferWithLength:options:",
				new_buffer as *const c_void,
				c"@@:QQ",
			),
			(
				c"newTextureWithDescriptor:",
				new_texture as *const c_void,
				c"@@:@",
			),
		];
		register(c"PooptermFakeDevice", &methods).raw() as usize
	});
	REFUSED.with(|cell| cell.set(refused));
	made();
	passed();
	// SAFETY: a subclass of `NSObject`, whose `init` is `NSObject`'s.
	unsafe { class_named(raw).instance() }.unwrap()
}

/// A `Device` on the fake, which refuses `refused` once made.
pub(crate) fn fake(refused: u32) -> Device {
	let device = Device::with(fake_device(0)).unwrap();
	REFUSED.with(|cell| cell.set(refused));
	// What making the device passed is its tests' business.
	passed();
	device
}
