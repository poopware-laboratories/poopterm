//! Metal textures of BGRA texels: the glyph atlas, an array of pages; an
//! image's; and targets to draw to and read back. Texels are written from
//! the CPU, region by region, checked to lie inside the texture.
use super::device::{Device, Error, owned};
use crate::objc::{Object, Raw};

/// `MTLPixelFormatBGRA8Unorm`: a `u32` 0xAARRGGBB in memory, as our
/// bitmaps hold their texels.
pub(crate) const BGRA: usize = 80;
/// `MTLTextureType2D` and `MTLTextureType2DArray`.
const FLAT: usize = 2;
const ARRAY: usize = 3;
/// `MTLStorageModeManaged`: the CPU writes and reads it, on Intel GPUs
/// through copies Metal makes, on Apple's in the memory both share.
const MANAGED: usize = 1;

/// What a texture is for: `MTLTextureUsageShaderRead`, and
/// `MTLTextureUsageRenderTarget` with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Usage {
	Sampled = 1,
	Target = 1 | 4,
}

/// `MTLRegion`: an origin and a size, in texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct Region {
	pub origin: [usize; 3],
	pub size: [usize; 3],
}

/// The region `size` texels large at `at` in a texture `bounds` large,
/// for `texels` texels: `None` when it does not lie inside, or the texels
/// do not fill it.
pub(crate) fn region(
	at: [u32; 2],
	size: [u32; 2],
	bounds: [u32; 2],
	texels: usize,
) -> Option<Region> {
	let inside = (0..2).all(|i| {
		at[i]
			.checked_add(size[i])
			.is_some_and(|end| end <= bounds[i])
	});
	let count = size[0] as usize * size[1] as usize;
	(inside && count == texels).then_some(Region {
		origin: [at[0] as usize, at[1] as usize, 0],
		size: [size[0] as usize, size[1] as usize, 1],
	})
}

pub(crate) struct Texture {
	pub(super) texture: Object,
	/// Width, height and layers.
	pub(crate) size: [u32; 3],
	pub(crate) usage: Usage,
}

impl Texture {
	/// A texture `size` texels large (width, height and layers), an array
	/// when `array`, for `usage`.
	pub(crate) fn new(
		device: &Device,
		size: [u32; 3],
		array: bool,
		usage: Usage,
	) -> Result<Self, Error> {
		let [width, height, layers] = size.map(|n| n as usize);
		if size.contains(&0) || (!array && layers != 1) {
			return Err(Error::Region);
		}
		let class = crate::objc::class(c"MTLTextureDescriptor")?;
		// SAFETY: a texture descriptor takes `new`, and each setter takes an
		// `NSUInteger` or enum of that size; `newTextureWithDescriptor:`
		// takes the descriptor and returns a texture the caller owns, or nil.
		let texture = unsafe {
			let descriptor = class
				.instance()
				.ok_or(Error::Refused("texture descriptor"))?;
			let kind = if array { ARRAY } else { FLAT };
			let _: () = descriptor.send(c"setTextureType:", (kind,));
			let _: () = descriptor.send(c"setPixelFormat:", (BGRA,));
			let _: () = descriptor.send(c"setWidth:", (width,));
			let _: () = descriptor.send(c"setHeight:", (height,));
			let _: () = descriptor.send(c"setArrayLength:", (layers,));
			let _: () = descriptor.send(c"setStorageMode:", (MANAGED,));
			let _: () = descriptor.send(c"setUsage:", (usage as usize,));
			let made: Raw = device
				.device
				.send(c"newTextureWithDescriptor:", (descriptor.raw(),));
			owned(made, "texture")?
		};
		Ok(Self {
			texture,
			size,
			usage,
		})
	}

	/// Writes `texels`, row by row, to the region `size` large at `at` of
	/// `layer`, unless it lies outside the texture.
	pub(crate) fn upload(
		&self,
		layer: u32,
		at: [u32; 2],
		size: [u32; 2],
		texels: &[u32],
	) -> Result<(), Error> {
		let [width, height, layers] = self.size;
		let region = region(at, size, [width, height], texels.len()).ok_or(Error::Region)?;
		if layer >= layers {
			return Err(Error::Region);
		}
		if texels.is_empty() {
			return Ok(());
		}
		let row = size[0] as usize * 4;
		// SAFETY: `replaceRegion:mipmapLevel:slice:withBytes:bytesPerRow:
		// bytesPerImage:` takes a region, two `NSUInteger`s, a pointer and two
		// more, and copies the region's texels from it before it returns; the
		// region and slice lie inside the texture, as checked above, and the
		// texels fill it.
		unsafe {
			let _: () = self.texture.send(
				c"replaceRegion:mipmapLevel:slice:withBytes:bytesPerRow:bytesPerImage:",
				(
					region,
					0usize,
					layer as usize,
					texels.as_ptr(),
					row,
					row * size[1] as usize,
				),
			);
		}
		Ok(())
	}

	/// The texels of the first layer, row by row, as the GPU last left them
	/// in a frame the caller waited for, which synchronized the texture.
	pub(crate) fn download(&self) -> Vec<u32> {
		let [width, height, _] = self.size;
		let mut texels = vec![0u32; width as usize * height as usize];
		let region = Region {
			origin: [0; 3],
			size: [width as usize, height as usize, 1],
		};
		// SAFETY: `getBytes:bytesPerRow:fromRegion:mipmapLevel:` takes a
		// pointer, an `NSUInteger`, a region and a level, and writes the
		// region's texels there, which has room for them all.
		unsafe {
			let _: () = self.texture.send(
				c"getBytes:bytesPerRow:fromRegion:mipmapLevel:",
				(texels.as_mut_ptr(), width as usize * 4, region, 0usize),
			);
		}
		texels
	}
}

#[cfg(test)]
#[path = "texture_tests.rs"]
mod tests;
