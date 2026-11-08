//! A fake `MTLDevice`, a class of our own registered with the runtime: it
//! makes queues, names, buffers, textures, shader libraries, functions
//! and pipelines, or refuses each when told, and records what it was asked
//! to make and what its textures were sent, so the bindings' failure paths
//! and the descriptors they fill are tested without a GPU's say. Its queue
//! and commands are `fake_commands_tests`'.
use super::{device::Device, fake_commands_tests::made as object_of, texture::Region};
use crate::objc::{
	Arguments, Object, Raw, Sel, class,
	probe_tests::{Weak, class_named, register},
	sel, string,
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
pub(crate) const LIBRARY: u32 = 16;
pub(crate) const FUNCTION: u32 = 32;
pub(crate) const PIPELINE: u32 = 64;
pub(crate) const COMMANDS: u32 = 128;
pub(crate) const ENCODER: u32 = 256;
/// Not a refusal: frames the GPU fails.
pub(crate) const FAILS: u32 = 512;

/// The fake's name.
pub(crate) const FAKE: &str = "Poopterm Fake GPU";

/// What the fake was asked to make.
#[derive(Clone, Debug, PartialEq, Eq)]
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
	/// Source compiled, by its length.
	Library(usize),
	/// A library's function, by name.
	Function(String),
	/// A pipeline of a color attachment's pixel format, whether it blends,
	/// and its source and destination color and alpha factors.
	Pipeline {
		format: usize,
		blends: bool,
		factors: [usize; 4],
	},
	/// Texels written to a texture: the texture, the region's origin and
	/// size, the slice, the bytes a row and an image, and the first texel.
	Upload {
		texture: usize,
		origin: [usize; 3],
		size: [usize; 3],
		slice: usize,
		row: usize,
		image: usize,
		first: u32,
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

pub(crate) fn refuses(what: u32) -> bool {
	REFUSED.with(Cell::get) & what != 0
}

extern "C" fn new_queue(_: Raw, _: Sel) -> Raw {
	if refuses(QUEUE) {
		std::ptr::null_mut()
	} else {
		super::fake_commands_tests::queue()
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
		return std::ptr::null_mut();
	}
	let buffer = object_of(buffer_class(), false);
	let data = class(c"NSMutableData").unwrap();
	// SAFETY: `dataWithLength:` returns autoreleased zeroed bytes of that
	// length, which the buffer then retains as its own until it goes.
	unsafe {
		let bytes: Raw = data.send(c"dataWithLength:", (length,));
		objc_setAssociatedObject(buffer, BYTES.as_ptr().cast(), bytes, RETAIN);
	}
	buffer
}

/// The key of a fake buffer's bytes, and `OBJC_ASSOCIATION_RETAIN_NONATOMIC`.
static BYTES: &CStr = c"bytes";
const RETAIN: usize = 1;

#[link(name = "objc")]
unsafe extern "C" {
	fn objc_setAssociatedObject(object: Raw, key: *const c_void, value: Raw, policy: usize);
	fn objc_getAssociatedObject(object: Raw, key: *const c_void) -> Raw;
}

/// A fake buffer's bytes, which it holds.
extern "C" fn contents(this: Raw, _: Sel) -> *mut c_void {
	// SAFETY: every fake buffer holds an `NSMutableData`, whose
	// `mutableBytes` lives as long as it.
	unsafe {
		let data = objc_getAssociatedObject(this, BYTES.as_ptr().cast());
		().send(data, sel(c"mutableBytes"))
	}
}

fn buffer_class() -> usize {
	static BUFFER_CLASS: OnceLock<usize> = OnceLock::new();
	*BUFFER_CLASS.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 1] =
			[(c"contents", contents as *const c_void, c"^v@:")];
		register(c"PooptermFakeBuffer", &methods).raw() as usize
	})
}

/// Records what a fake texture was sent to write.
extern "C" fn replace(
	this: Raw,
	_: Sel,
	region: Region,
	_level: usize,
	slice: usize,
	texels: *const u32,
	row: usize,
	image: usize,
) {
	// SAFETY: the caller passes the region's texels, and a region of none
	// is never sent.
	let first = unsafe { *texels };
	let upload = Made::Upload {
		texture: this as usize,
		origin: region.origin,
		size: region.size,
		slice,
		row,
		image,
		first,
	};
	MADE.with(|made| made.borrow_mut().push(upload));
}

fn texture_class() -> usize {
	static TEXTURE_CLASS: OnceLock<usize> = OnceLock::new();
	*TEXTURE_CLASS.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 1] = [(
			c"replaceRegion:mipmapLevel:slice:withBytes:bytesPerRow:bytesPerImage:",
			replace as *const c_void,
			c"v@:{MTLRegion={MTLOrigin=QQQ}{MTLSize=QQQ}}QQ^vQQ",
		)];
		register(c"PooptermFakeTexture", &methods).raw() as usize
	})
}

/// A fake library's function `name`, or nil for one it lacks.
extern "C" fn new_function(_: Raw, _: Sel, name: Raw) -> Raw {
	// SAFETY: an `NSString`.
	let name = unsafe { crate::objc::text(&Object::retained(name).unwrap()) };
	let known = ["quad_vertex", "quad_fragment"].contains(&name.as_str());
	MADE.with(|made| made.borrow_mut().push(Made::Function(name.clone())));
	if refuses(FUNCTION) || !known {
		return std::ptr::null_mut();
	}
	// A pipeline descriptor takes only real functions: the real device's,
	// compiled once a thread.
	thread_local! {
		static REAL: Option<Object> = super::device::tests::device()
			.and_then(|device| super::pipeline::library(&device, super::pipeline::SOURCE).ok());
	}
	let name = string(&name).unwrap();
	REAL.with(|library| match library {
		// SAFETY: `newFunctionWithName:` takes a string and returns a
		// function the caller owns, handed over raw.
		Some(library) => unsafe { library.send(c"newFunctionWithName:", (name.raw(),)) },
		None => panic!("the fake's functions need a real Metal device"),
	})
}

fn library_class() -> usize {
	static LIBRARY_CLASS: OnceLock<usize> = OnceLock::new();
	*LIBRARY_CLASS.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 1] = [(
			c"newFunctionWithName:",
			new_function as *const c_void,
			c"@@:@",
		)];
		register(c"PooptermFakeLibrary", &methods).raw() as usize
	})
}

/// Compiles nothing: a library, or nil with an error when told.
extern "C" fn new_library(_: Raw, _: Sel, source: Raw, _options: Raw, error: *mut Raw) -> Raw {
	// SAFETY: an `NSString`, whose `length` returns an `NSUInteger`.
	let length: usize = unsafe { ().send(source, sel(c"length")) };
	MADE.with(|made| made.borrow_mut().push(Made::Library(length)));
	if refuses(LIBRARY) {
		// SAFETY: the caller passes room for an error, set autoreleased.
		unsafe { *error = super::fake_commands_tests::error("fake: use of undeclared identifier") };
		return std::ptr::null_mut();
	}
	object_of(library_class(), false)
}

/// Reads what a pipeline descriptor asks for: a state, or nil with an
/// error when told.
extern "C" fn new_pipeline(_: Raw, _: Sel, descriptor: Raw, error: *mut Raw) -> Raw {
	// SAFETY: an `MTLRenderPipelineDescriptor`, whose `colorAttachments`
	// and their first it holds; the first's getters return an enum each and
	// a `BOOL`.
	let pipeline = unsafe {
		let descriptor = Object::retained(descriptor).unwrap();
		let attachments = Object::retained(descriptor.send(c"colorAttachments", ())).unwrap();
		let first: Raw = attachments.send(c"objectAtIndexedSubscript:", (0usize,));
		let first = Object::retained(first).unwrap();
		let factor = |getter: &CStr| first.send::<usize>(getter, ());
		Made::Pipeline {
			format: factor(c"pixelFormat"),
			blends: first.send::<crate::objc::Bool>(c"isBlendingEnabled", ()) != 0,
			factors: [
				factor(c"sourceRGBBlendFactor"),
				factor(c"destinationRGBBlendFactor"),
				factor(c"sourceAlphaBlendFactor"),
				factor(c"destinationAlphaBlendFactor"),
			],
		}
	};
	MADE.with(|made| made.borrow_mut().push(pipeline));
	if refuses(PIPELINE) {
		// SAFETY: the caller passes room for an error, set autoreleased.
		unsafe { *error = super::fake_commands_tests::error("fake: no such attachment") };
		return std::ptr::null_mut();
	}
	anything()
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
		object_of(texture_class(), false)
	}
}

/// The fake device, refusing `refused` (bits of the constants above) on
/// this thread from now on.
pub(crate) fn fake_device(refused: u32) -> Object {
	static FAKE_DEVICE: OnceLock<usize> = OnceLock::new();
	let raw = *FAKE_DEVICE.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 6] = [
			(c"newCommandQueue", new_queue as *const c_void, c"@@:"),
			(
				c"newLibraryWithSource:options:error:",
				new_library as *const c_void,
				c"@@:@@^@",
			),
			(
				c"newRenderPipelineStateWithDescriptor:error:",
				new_pipeline as *const c_void,
				c"@@:@^@",
			),
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

/// Has the fake refuse `refused` on this thread from now on.
pub(crate) fn refuse(refused: u32) {
	REFUSED.with(|cell| cell.set(refused));
}

/// A `Device` on the fake, which refuses `refused` once made.
pub(crate) fn fake(refused: u32) -> Device {
	let device = Device::with(fake_device(0)).unwrap();
	REFUSED.with(|cell| cell.set(refused));
	// What making the device passed is its tests' business.
	passed();
	device
}
