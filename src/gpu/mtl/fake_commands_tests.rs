//! The fake device's queue, command buffers and encoders, classes of our
//! own registered with the runtime: each records what it is sent, in
//! order, so a frame's commands are checked call by call, and refuses or
//! fails a frame when told.
use super::fake_tests::{COMMANDS, ENCODER, FAILS, refuses};
use crate::objc::{
	Arguments, Object, Raw, Sel, class,
	probe_tests::{class_named, register},
	sel, string,
};
use std::{
	cell::RefCell,
	ffi::{CStr, c_void},
	sync::OnceLock,
};

/// What the fake was sent, objects by address.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Call {
	/// A render pass over a texture, with its load and store actions.
	Pass {
		texture: usize,
		load: usize,
		store: usize,
	},
	State(usize),
	VertexBuffer {
		buffer: usize,
		offset: usize,
		index: usize,
	},
	FragmentBuffer {
		buffer: usize,
		offset: usize,
		index: usize,
	},
	VertexBytes {
		bytes: [f32; 2],
		index: usize,
	},
	FragmentTexture {
		texture: usize,
		index: usize,
	},
	Draw {
		primitive: usize,
		start: usize,
		count: usize,
		instances: usize,
		base: usize,
	},
	Blit,
	Synchronize(usize),
	/// A drawable shown once the commands are done.
	Present(usize),
	End,
	Commit,
	Wait,
}

thread_local! {
	static CALLS: RefCell<Vec<Call>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn record(call: Call) {
	CALLS.with(|calls| calls.borrow_mut().push(call));
}

/// Takes what the fake was sent on this thread.
pub(crate) fn calls() -> Vec<Call> {
	CALLS.with(|calls| std::mem::take(&mut *calls.borrow_mut()))
}

/// An object of the class registered as `raw`, owned by the caller and
/// handed over raw; autoreleased when `unowned`, as methods that do not
/// hand over what they return do.
pub(crate) fn made(raw: usize, unowned: bool) -> Raw {
	// SAFETY: a subclass of `NSObject`, whose `init` is `NSObject`'s.
	let object = unsafe { class_named(raw).instance() }.unwrap();
	let raw = object.raw();
	std::mem::forget(object);
	if unowned {
		// SAFETY: `autorelease` takes nothing and returns the object, whose
		// reference the caller's pool now holds.
		let _: Raw = unsafe { ().send(raw, sel(c"autorelease")) };
	}
	raw
}

/// An autoreleased `NSError` that says `said`.
pub(crate) fn error(said: &str) -> Raw {
	let (key, value) = (
		string("NSLocalizedDescription").unwrap(),
		string(said).unwrap(),
	);
	let (dictionary, error) = (class(c"NSDictionary").unwrap(), class(c"NSError").unwrap());
	// SAFETY: `dictionaryWithObject:forKey:` takes two objects and returns an
	// autoreleased dictionary; `errorWithDomain:code:userInfo:` a string, an
	// `NSInteger` and that dictionary, and returns an autoreleased error.
	unsafe {
		let info: Raw = dictionary.send(c"dictionaryWithObject:forKey:", (value.raw(), key.raw()));
		let domain = string("PooptermFake").unwrap();
		error.send(
			c"errorWithDomain:code:userInfo:",
			(domain.raw(), 1isize, info),
		)
	}
}

extern "C" fn set_state(_: Raw, _: Sel, state: Raw) {
	record(Call::State(state as usize));
}

extern "C" fn set_vertex_buffer(_: Raw, _: Sel, buffer: Raw, offset: usize, index: usize) {
	record(Call::VertexBuffer {
		buffer: buffer as usize,
		offset,
		index,
	});
}

extern "C" fn set_fragment_buffer(_: Raw, _: Sel, buffer: Raw, offset: usize, index: usize) {
	record(Call::FragmentBuffer {
		buffer: buffer as usize,
		offset,
		index,
	});
}

extern "C" fn set_vertex_bytes(_: Raw, _: Sel, bytes: *const f32, length: usize, index: usize) {
	assert_eq!(length, 8, "the target's size is two floats");
	// SAFETY: the caller passes `length` bytes, two floats.
	let bytes = unsafe { [*bytes, *bytes.add(1)] };
	record(Call::VertexBytes { bytes, index });
}

extern "C" fn set_fragment_texture(_: Raw, _: Sel, texture: Raw, index: usize) {
	record(Call::FragmentTexture {
		texture: texture as usize,
		index,
	});
}

extern "C" fn draw(
	_: Raw,
	_: Sel,
	primitive: usize,
	start: usize,
	count: usize,
	instances: usize,
	base: usize,
) {
	record(Call::Draw {
		primitive,
		start,
		count,
		instances,
		base,
	});
}

extern "C" fn synchronize(_: Raw, _: Sel, resource: Raw) {
	record(Call::Synchronize(resource as usize));
}

extern "C" fn end(_: Raw, _: Sel) {
	record(Call::End);
}

fn encoder_class() -> usize {
	static ENCODER_CLASS: OnceLock<usize> = OnceLock::new();
	*ENCODER_CLASS.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 8] = [
			(
				c"setRenderPipelineState:",
				set_state as *const c_void,
				c"v@:@",
			),
			(
				c"setVertexBuffer:offset:atIndex:",
				set_vertex_buffer as *const c_void,
				c"v@:@QQ",
			),
			(
				c"setFragmentBuffer:offset:atIndex:",
				set_fragment_buffer as *const c_void,
				c"v@:@QQ",
			),
			(
				c"setVertexBytes:length:atIndex:",
				set_vertex_bytes as *const c_void,
				c"v@:^vQQ",
			),
			(
				c"setFragmentTexture:atIndex:",
				set_fragment_texture as *const c_void,
				c"v@:@Q",
			),
			(
				c"drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:",
				draw as *const c_void,
				c"v@:QQQQQ",
			),
			(
				c"synchronizeResource:",
				synchronize as *const c_void,
				c"v@:@",
			),
			(c"endEncoding", end as *const c_void, c"v@:"),
		];
		register(c"PooptermFakeEncoder", &methods).raw() as usize
	})
}

extern "C" fn render_encoder(_: Raw, _: Sel, pass: Raw) -> Raw {
	if refuses(ENCODER) {
		return std::ptr::null_mut();
	}
	// SAFETY: an `MTLRenderPassDescriptor`, whose `colorAttachments` and
	// their first it holds; the first's getters return the texture and the
	// actions.
	let call = unsafe {
		let pass = Object::retained(pass).unwrap();
		let attachments = Object::retained(pass.send(c"colorAttachments", ())).unwrap();
		let first: Raw = attachments.send(c"objectAtIndexedSubscript:", (0usize,));
		let first = Object::retained(first).unwrap();
		Call::Pass {
			texture: first.send::<Raw>(c"texture", ()) as usize,
			load: first.send(c"loadAction", ()),
			store: first.send(c"storeAction", ()),
		}
	};
	record(call);
	made(encoder_class(), true)
}

extern "C" fn blit_encoder(_: Raw, _: Sel) -> Raw {
	record(Call::Blit);
	made(encoder_class(), true)
}

extern "C" fn present(_: Raw, _: Sel, drawable: Raw) {
	record(Call::Present(drawable as usize));
}

extern "C" fn commit(_: Raw, _: Sel) {
	record(Call::Commit);
}

extern "C" fn wait(_: Raw, _: Sel) {
	record(Call::Wait);
}

/// Completed, or failed when told.
extern "C" fn status(_: Raw, _: Sel) -> usize {
	if refuses(FAILS) { 5 } else { 4 }
}

extern "C" fn command_error(_: Raw, _: Sel) -> Raw {
	if refuses(FAILS) {
		error("the fake GPU hung")
	} else {
		std::ptr::null_mut()
	}
}

fn commands_class() -> usize {
	static COMMANDS_CLASS: OnceLock<usize> = OnceLock::new();
	*COMMANDS_CLASS.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 7] = [
			(c"presentDrawable:", present as *const c_void, c"v@:@"),
			(
				c"renderCommandEncoderWithDescriptor:",
				render_encoder as *const c_void,
				c"@@:@",
			),
			(c"blitCommandEncoder", blit_encoder as *const c_void, c"@@:"),
			(c"commit", commit as *const c_void, c"v@:"),
			(c"waitUntilCompleted", wait as *const c_void, c"v@:"),
			(c"status", status as *const c_void, c"Q@:"),
			(c"error", command_error as *const c_void, c"@@:"),
		];
		register(c"PooptermFakeCommands", &methods).raw() as usize
	})
}

extern "C" fn command_buffer(_: Raw, _: Sel) -> Raw {
	if refuses(COMMANDS) {
		return std::ptr::null_mut();
	}
	made(commands_class(), true)
}

/// A new fake queue, owned by the caller and handed over raw.
pub(crate) fn queue() -> Raw {
	static QUEUE_CLASS: OnceLock<usize> = OnceLock::new();
	let raw = *QUEUE_CLASS.get_or_init(|| {
		let methods: [(&CStr, *const c_void, &CStr); 1] =
			[(c"commandBuffer", command_buffer as *const c_void, c"@@:")];
		register(c"PooptermFakeQueue", &methods).raw() as usize
	});
	made(raw, false)
}
