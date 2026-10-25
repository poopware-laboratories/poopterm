//! The renderer's shaders: SPIR-V compiled from the GLSL in `shaders/`
//! by `scripts/shaders.sh` and checked in, read into words, and made into
//! shader modules, destroyed when dropped.
use super::{
	call::{Error, check},
	device::Device,
	pipeline_types::*,
};
use std::{ptr, rc::Rc};

/// The quad shaders: where each quad goes, and what fills it.
pub(crate) static QUAD_VERTEX: &[u8] = include_bytes!("../../../shaders/quad.vert.spv");
pub(crate) static QUAD_FRAGMENT: &[u8] = include_bytes!("../../../shaders/quad.frag.spv");

const MAGIC: u32 = 0x0723_0203;
/// The words of the header: magic, version, generator, bound and schema.
const HEADER: usize = 5;
/// SPIR-V 1.0 to 1.6, the versions Vulkan 1.3 takes, as the header writes
/// them.
const VERSIONS: std::ops::RangeInclusive<u32> = 0x0001_0000..=0x0001_0600;

/// The words of the SPIR-V module `bytes`, in either byte order, if it is
/// one Vulkan 1.3 takes.
pub(crate) fn words(bytes: &[u8]) -> Result<Vec<u32>, Error> {
	let (chunks, rest) = bytes.as_chunks::<4>();
	let read = match chunks.first() {
		Some(&first) if u32::from_le_bytes(first) == MAGIC => u32::from_le_bytes,
		Some(&first) if u32::from_be_bytes(first) == MAGIC => u32::from_be_bytes,
		_ => return Err(Error::Spirv),
	};
	let words: Vec<u32> = chunks.iter().map(|&chunk| read(chunk)).collect();
	if !rest.is_empty() || words.len() < HEADER || !VERSIONS.contains(&words[1]) {
		return Err(Error::Spirv);
	}
	Ok(words)
}

pub(crate) struct ShaderModule {
	handle: RawShaderModule,
	/// Dropped after the module is destroyed, as it was made from it.
	device: Rc<Device>,
}

impl ShaderModule {
	/// A module of `code`, the words of a SPIR-V module.
	pub(crate) fn new(device: &Rc<Device>, code: &[u32]) -> Result<Self, Error> {
		let info = ShaderModuleCreateInfo {
			s_type: STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
			next: ptr::null(),
			flags: 0,
			code_size: size_of_val(code),
			code: code.as_ptr(),
		};
		let mut handle = 0;
		let create = device.functions.create_module;
		// SAFETY: the device is live, `info` and the code it points to
		// outlive the call, and `handle` is writable.
		check(unsafe { create(device.handle, &info, ptr::null(), &mut handle) })?;
		Ok(Self {
			handle,
			device: device.clone(),
		})
	}

	pub(crate) fn handle(&self) -> RawShaderModule {
		self.handle
	}
}

impl Drop for ShaderModule {
	fn drop(&mut self) {
		let destroy = self.device.functions.destroy_module;
		// SAFETY: the module is the device's; pipelines made from it do not
		// need it any more.
		unsafe { destroy(self.device.handle, self.handle, ptr::null()) };
	}
}
