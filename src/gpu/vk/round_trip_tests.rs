//! Copies through a real GPU: pixels uploaded to an image layer, part of
//! them overwritten, and read back, which must match texel for texel. Run
//! by the loader's tests and by its validation child.
use super::{
	buffer::Buffer,
	call::Error,
	commands::Commands,
	device::Device,
	image::{Format, Image},
	loader_tests::{entry, offscreen},
	memory::Usage,
	recording::Region,
	timeline::Timeline,
};
use std::{num::NonZeroUsize, rc::Rc};

/// `VK_BUFFER_USAGE_TRANSFER_SRC_BIT` and `_DST_BIT`, which image usage
/// shares, and `VK_IMAGE_USAGE_SAMPLED_BIT`, without which images have no
/// view.
const TRANSFER_SRC: u32 = 1;
const TRANSFER_DST: u32 = 2;
const SAMPLED: u32 = 4;
/// The image copied through: 16×8 texels, the second of two layers.
const WIDTH: u32 = 16;
const HEIGHT: u32 = 8;
const LAYER: u32 = 1;
/// The part overwritten: 4×2 texels at 3,2.
const PATCH: Region = Region {
	layer: LAYER,
	at: [3, 2],
	size: [4, 2],
};

/// A texel no two others share.
fn texel(i: u32) -> u32 {
	0x8000_0000 | i.wrapping_mul(0x9e37_79b9) >> 1
}

/// The words the image should hold once the whole layer and then the
/// patch, from its own words, were uploaded.
fn expected() -> Vec<u32> {
	let mut words: Vec<u32> = (0..WIDTH * HEIGHT).map(texel).collect();
	let [x, y] = PATCH.at;
	let [width, height] = PATCH.size;
	for row in 0..height {
		for column in 0..width {
			let index = (y + row) * WIDTH + x + column;
			words[index as usize] = !texel(row * width + column);
		}
	}
	words
}

/// Uploads the layer and the patch, reads the layer back, and says
/// whether it holds what it should.
pub(super) fn round_trip(device: &Rc<Device>) -> Result<bool, Error> {
	let layer = (WIDTH * HEIGHT * 4) as usize;
	let size = |bytes| NonZeroUsize::new(bytes).unwrap();
	let mut staging = Buffer::new(device, size(2 * layer), TRANSFER_SRC, Usage::Upload)?;
	let bytes = staging.bytes_mut().ok_or(Error::NoMemory)?;
	let [width, height] = PATCH.size;
	let words = (0..WIDTH * HEIGHT).map(texel);
	let patch = (0..width * height).map(|i| !texel(i));
	for (chunk, word) in bytes
		.as_chunks_mut::<4>()
		.0
		.iter_mut()
		.zip(words.chain(patch))
	{
		*chunk = word.to_ne_bytes();
	}
	let staging = Rc::new(staging);
	let readback = Rc::new(Buffer::new(
		device,
		size(layer),
		TRANSFER_DST,
		Usage::Readback,
	)?);
	let usage = TRANSFER_SRC | TRANSFER_DST | SAMPLED;
	let image = Rc::new(Image::new(device, Format::Bgra, [WIDTH, HEIGHT, 2], usage)?);
	let timeline = Rc::new(Timeline::new(device)?);
	let mut commands = Commands::new(&timeline)?;
	let whole = Region {
		layer: LAYER,
		at: [0, 0],
		size: [WIDTH, HEIGHT],
	};
	let mut recording = commands.record()?;
	recording.upload(&staging, 0, &image, whole)?;
	recording.submit()?;
	// The patch and the read back in a submission of their own, which
	// starts from the layout the first left the image in.
	let mut recording = commands.record()?;
	recording.upload(&staging, layer as u64, &image, PATCH)?;
	recording.download(&image, whole, &readback, 0)?;
	recording.submit()?;
	commands.finish()?;
	let read = readback.bytes().ok_or(Error::NoMemory)?;
	let read: Vec<u32> = read
		.as_chunks::<4>()
		.0
		.iter()
		.map(|&chunk| u32::from_ne_bytes(chunk))
		.collect();
	Ok(read == expected())
}

#[test]
fn copies_through_the_gpu_come_back_texel_for_texel() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	assert_eq!(round_trip(&device), Ok(true));
}

#[test]
fn the_expected_layer_is_the_pattern_with_the_patch_inverted_over_it() {
	let words = expected();
	assert_eq!(words.len(), 128);
	// Row 2 from column 3 is the patch's first row; its neighbours are not.
	assert_eq!(words[2 * 16 + 2], texel(34));
	assert_eq!(
		words[2 * 16 + 3..2 * 16 + 7],
		[0, 1, 2, 3].map(|i| !texel(i))
	);
	assert_eq!(
		words[3 * 16 + 3..3 * 16 + 7],
		[4, 5, 6, 7].map(|i| !texel(i))
	);
	assert_eq!(words[4 * 16 + 3], texel(67));
	let distinct: std::collections::HashSet<_> = words.iter().collect();
	assert_eq!(distinct.len(), 128);
}
