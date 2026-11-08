//! A frame's commands: a command buffer, quads drawn over a target with
//! glyphs from the atlas, a run at a time with its image bound, textures
//! synchronized for the CPU to read, and the buffer committed for the GPU,
//! to wait for. A command buffer holds what its commands use until the GPU
//! is done with it.
use super::{
	buffer::Buffer,
	device::{Device, Error, retained},
	pipeline::Pipeline,
	texture::{Texture, Usage},
};
use crate::{
	gpu::encode::QUAD,
	objc::{Object, Pool, Raw, describe},
};
use std::ops::Range;

/// Bytes a quad, and vertices a quad: two triangles.
const QUAD_BYTES: usize = QUAD * 4;
const VERTICES: usize = 6;
/// `MTLPrimitiveTypeTriangle`.
const TRIANGLE: usize = 3;
/// `MTLLoadActionLoad` and `MTLStoreActionStore`: quads are drawn over
/// what the target holds.
const LOAD: usize = 1;
const STORE: usize = 1;
/// `MTLCommandBufferStatusCompleted` and `MTLCommandBufferStatusError`.
const COMPLETED: usize = 4;
const FAILED: usize = 5;

/// A draw of the `runs` of quads in `quads` over `target`, with glyphs
/// from `atlas`, each run with its image bound.
#[derive(Clone, Copy)]
pub(crate) struct Draw<'d> {
	pub pipeline: &'d Pipeline,
	pub quads: &'d Buffer,
	pub runs: &'d [(Range<u32>, &'d Texture)],
	pub atlas: &'d Texture,
	pub target: &'d Texture,
}

/// Whether `draw` may be drawn: its runs lie within its quads, the target
/// is made to draw to, of one layer, and neither the atlas nor an image.
pub(crate) fn drawable(draw: &Draw) -> bool {
	let end = draw.runs.iter().map(|(range, _)| range.end).max();
	let fits = end.is_none_or(|end| end as usize * QUAD_BYTES <= draw.quads.size);
	let ordered = draw.runs.iter().all(|(range, _)| range.start <= range.end);
	let target = draw.target;
	let apart = |texture: &Texture| !std::ptr::eq(texture, target);
	fits
		&& ordered
		&& target.usage == Usage::Target
		&& target.size[2] == 1
		&& apart(draw.atlas)
		&& draw.runs.iter().all(|(_, image)| apart(image))
}

/// A command buffer being recorded.
pub(crate) struct Frame {
	pub(super) buffer: Object,
}

impl Frame {
	/// A command buffer on `device`'s queue.
	pub(crate) fn new(device: &Device) -> Result<Self, Error> {
		let _pool = Pool::new();
		// SAFETY: `commandBuffer` takes nothing and returns a command buffer
		// the caller does not own, or nil.
		let buffer = unsafe { retained(device.queue.send(c"commandBuffer", ()), "command buffer") }?;
		Ok(Self { buffer })
	}

	/// Records `draw`, refused when not `drawable`.
	pub(crate) fn draw(&mut self, draw: Draw) -> Result<(), Error> {
		if !drawable(&draw) {
			return Err(Error::Draw);
		}
		let _pool = Pool::new();
		let class = crate::objc::class(c"MTLRenderPassDescriptor")?;
		let [width, height, _] = draw.target.size;
		let size = [width as f32, height as f32];
		// SAFETY: `renderPassDescriptor` returns a descriptor the caller
		// does not own; its `colorAttachments` and their first, which it
		// holds, take the target texture and the actions, enums of
		// `NSUInteger`s. `renderCommandEncoderWithDescriptor:` returns an
		// encoder the caller does not own, or nil; it takes the pipeline,
		// the buffers with an offset and index, the size as bytes, the
		// textures with an index, and draws of triangles with a first
		// vertex, vertex count, instance count and first instance, all
		// `NSUInteger`s. The draw was checked above: every instance lies in
		// the quads, and the textures bound are the shaders' kinds.
		unsafe {
			let pass: Raw = class.send(c"renderPassDescriptor", ());
			let pass = retained(pass, "render pass")?;
			let attachments = retained(pass.send(c"colorAttachments", ()), "color attachments")?;
			let first: Raw = attachments.send(c"objectAtIndexedSubscript:", (0usize,));
			let first = retained(first, "color attachment")?;
			let _: () = first.send(c"setTexture:", (draw.target.texture.raw(),));
			let _: () = first.send(c"setLoadAction:", (LOAD,));
			let _: () = first.send(c"setStoreAction:", (STORE,));
			let encoder: Raw = self
				.buffer
				.send(c"renderCommandEncoderWithDescriptor:", (pass.raw(),));
			let encoder = retained(encoder, "render encoder")?;
			let quads = draw.quads.buffer.raw();
			let _: () = encoder.send(c"setRenderPipelineState:", (draw.pipeline.state.raw(),));
			let _: () = encoder.send(c"setVertexBuffer:offset:atIndex:", (quads, 0usize, 0usize));
			let _: () = encoder.send(
				c"setFragmentBuffer:offset:atIndex:",
				(quads, 0usize, 0usize),
			);
			let _: () = encoder.send(
				c"setVertexBytes:length:atIndex:",
				(size.as_ptr(), size_of_val(&size), 1usize),
			);
			let _: () = encoder.send(
				c"setFragmentTexture:atIndex:",
				(draw.atlas.texture.raw(), 0usize),
			);
			for (range, image) in draw.runs.iter().filter(|(range, _)| !range.is_empty()) {
				let _: () = encoder.send(
					c"setFragmentTexture:atIndex:",
					(image.texture.raw(), 1usize),
				);
				let _: () = encoder.send(
					c"drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:",
					(
						TRIANGLE,
						0usize,
						VERTICES,
						range.len(),
						range.start as usize,
					),
				);
			}
			let _: () = encoder.send(c"endEncoding", ());
		}
		Ok(())
	}

	/// Has the GPU make what it drew to `texture` visible to the CPU once
	/// the frame is done: needed for a managed texture on a GPU with its own
	/// memory, harmless on Apple's.
	pub(crate) fn synchronize(&mut self, texture: &Texture) -> Result<(), Error> {
		let _pool = Pool::new();
		// SAFETY: `blitCommandEncoder` returns an encoder the caller does not
		// own, or nil; `synchronizeResource:` takes a managed resource, as
		// every `Texture` is.
		unsafe {
			let blit = retained(self.buffer.send(c"blitCommandEncoder", ()), "blit encoder")?;
			let _: () = blit.send(c"synchronizeResource:", (texture.texture.raw(),));
			let _: () = blit.send(c"endEncoding", ());
		}
		Ok(())
	}

	/// Commits the commands for the GPU to run.
	pub(crate) fn commit(self) -> Submitted {
		// SAFETY: `commit` takes nothing; every encoder ended.
		unsafe {
			let _: () = self.buffer.send(c"commit", ());
		}
		Submitted {
			buffer: self.buffer,
		}
	}
}

/// Commands the GPU runs, or ran.
pub(crate) struct Submitted {
	buffer: Object,
}

impl Submitted {
	/// `MTLCommandBufferStatus`.
	fn status(&self) -> usize {
		// SAFETY: `status` takes nothing and returns an enum of `NSUInteger`.
		unsafe { self.buffer.send(c"status", ()) }
	}

	/// Whether the GPU is done with the commands, done well or not.
	pub(crate) fn done(&self) -> bool {
		self.status() >= COMPLETED
	}

	/// Waits for the GPU to be done with the commands: what went wrong if
	/// it failed them.
	pub(crate) fn wait(&self) -> Result<(), Error> {
		let _pool = Pool::new();
		// SAFETY: `waitUntilCompleted` takes nothing, on a committed buffer;
		// `error` returns an `NSError` or nil.
		unsafe {
			let _: () = self.buffer.send(c"waitUntilCompleted", ());
			if self.status() == FAILED {
				let error: Raw = self.buffer.send(c"error", ());
				return Err(Error::Gpu(describe(error, "unknown error")));
			}
		}
		Ok(())
	}
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
