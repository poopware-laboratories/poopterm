//! The quad pipeline: the shaders in `shaders/quad.metal`, compiled from
//! source by Metal when the renderer starts, so building needs no Xcode,
//! blending each quad over a BGRA target as the Vulkan pipeline does.
use super::{
	device::{Device, Error, owned, retained},
	texture::BGRA,
};
use crate::objc::{Bool, NIL, Object, Pool, Raw, describe, string};
use std::ptr;

/// The shaders' source.
pub(crate) static SOURCE: &str = include_str!("../../../shaders/quad.metal");

/// `MTLBlendFactor`s: the color is `source × alpha + target × (1 − alpha)`,
/// the alpha `source + target × (1 − alpha)`; the operation is an add.
const ONE: usize = 1;
const SOURCE_ALPHA: usize = 4;
const ONE_MINUS_SOURCE_ALPHA: usize = 5;

pub(crate) struct Pipeline {
	pub(super) state: Object,
}

/// The library `source` compiles to on `device`, or what the compiler said.
pub(crate) fn library(device: &Device, source: &str) -> Result<Object, Error> {
	let _pool = Pool::new();
	let source = string(source).ok_or(Error::Refused("shader source"))?;
	let mut error: Raw = ptr::null_mut();
	// SAFETY: `newLibraryWithSource:options:error:` takes a string, options
	// or nil and a pointer to an error, which it sets autoreleased (the pool
	// holds it) when it returns nil, and returns a library the caller owns.
	let made: Raw = unsafe {
		device.device.send(
			c"newLibraryWithSource:options:error:",
			(source.raw(), NIL, &mut error as *mut Raw),
		)
	};
	// SAFETY: the library, owned, or nil; the error is an `NSError` or nil.
	unsafe { Object::owned(made) }
		.ok_or_else(|| Error::Shader(unsafe { describe(error, "no library") }))
}

/// The function `name` of `library`.
fn function(library: &Object, name: &std::ffi::CStr) -> Result<Object, Error> {
	let _pool = Pool::new();
	let name = string(&name.to_string_lossy()).ok_or(Error::Refused("function name"))?;
	// SAFETY: `newFunctionWithName:` takes a string and returns a function
	// the caller owns, or nil.
	unsafe {
		owned(
			library.send(c"newFunctionWithName:", (name.raw(),)),
			"shader function",
		)
	}
}

impl Pipeline {
	/// The quad pipeline on `device`.
	pub(crate) fn new(device: &Device) -> Result<Self, Error> {
		let library = library(device, SOURCE)?;
		let vertex = function(&library, c"quad_vertex")?;
		let fragment = function(&library, c"quad_fragment")?;
		let _pool = Pool::new();
		let class = crate::objc::class(c"MTLRenderPipelineDescriptor")?;
		let mut error: Raw = ptr::null_mut();
		// SAFETY: a pipeline descriptor takes `new`; its functions are set
		// with a function each, and `colorAttachments` returns its array of
		// attachment descriptors, whose `objectAtIndexedSubscript:` returns the
		// first, which the descriptor holds; each setter takes an enum or
		// `BOOL`. `newRenderPipelineStateWithDescriptor:error:` takes the
		// descriptor and a pointer to an error, set autoreleased (the pool
		// holds it) when it returns nil, and returns a state the caller owns.
		let made: Raw = unsafe {
			let descriptor = class
				.instance()
				.ok_or(Error::Refused("pipeline descriptor"))?;
			let _: () = descriptor.send(c"setVertexFunction:", (vertex.raw(),));
			let _: () = descriptor.send(c"setFragmentFunction:", (fragment.raw(),));
			let attachments: Raw = descriptor.send(c"colorAttachments", ());
			let attachments = retained(attachments, "color attachments")?;
			let first: Raw = attachments.send(c"objectAtIndexedSubscript:", (0usize,));
			let first = retained(first, "color attachment")?;
			let _: () = first.send(c"setPixelFormat:", (BGRA,));
			let _: () = first.send(c"setBlendingEnabled:", (1 as Bool,));
			let _: () = first.send(c"setSourceRGBBlendFactor:", (SOURCE_ALPHA,));
			let _: () = first.send(c"setDestinationRGBBlendFactor:", (ONE_MINUS_SOURCE_ALPHA,));
			let _: () = first.send(c"setSourceAlphaBlendFactor:", (ONE,));
			let _: () = first.send(
				c"setDestinationAlphaBlendFactor:",
				(ONE_MINUS_SOURCE_ALPHA,),
			);
			device.device.send(
				c"newRenderPipelineStateWithDescriptor:error:",
				(descriptor.raw(), &mut error as *mut Raw),
			)
		};
		// SAFETY: the state, owned, or nil; the error is an `NSError` or nil.
		let state = unsafe { Object::owned(made) }
			.ok_or_else(|| Error::Shader(unsafe { describe(error, "no pipeline") }))?;
		Ok(Self { state })
	}
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
