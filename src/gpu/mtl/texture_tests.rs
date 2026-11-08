//! Tests of textures: regions checked to lie inside before any texel is
//! written, sizes and kinds refused before Metal sees them, and texels
//! written by the CPU read back from the GPU.
use super::{Region, Texture, Usage, region};
use crate::gpu::mtl::device::{Error, tests::device};

/// The texels of `texture`'s first layer, row by row, as the CPU or a
/// frame the caller waited for, which synchronized it, left them.
pub(crate) fn download(texture: &Texture) -> Vec<u32> {
	let [width, height, _] = texture.size;
	let mut texels = vec![0u32; width as usize * height as usize];
	let region = Region {
		origin: [0; 3],
		size: [width as usize, height as usize, 1],
	};
	// SAFETY: `getBytes:bytesPerRow:fromRegion:mipmapLevel:` takes a
	// pointer, an `NSUInteger`, a region and a level, and writes the
	// region's texels there, which has room for them all.
	unsafe {
		let _: () = texture.texture.send(
			c"getBytes:bytesPerRow:fromRegion:mipmapLevel:",
			(texels.as_mut_ptr(), width as usize * 4, region, 0usize),
		);
	}
	texels
}

#[test]
fn a_region_inside_the_texture_and_filled_by_its_texels_is_one() {
	assert_eq!(
		region([2, 3], [4, 5], [16, 8], 20),
		Some(Region {
			origin: [2, 3, 0],
			size: [4, 5, 1],
		})
	);
	// Reaching the far edges exactly.
	assert!(region([12, 3], [4, 5], [16, 8], 20).is_some());
	assert!(region([0, 0], [16, 8], [16, 8], 128).is_some());
	// Empty regions of no texels lie anywhere inside.
	assert!(region([16, 8], [0, 0], [16, 8], 0).is_some());
}

#[test]
fn a_region_past_an_edge_or_not_filled_by_its_texels_is_refused() {
	// One past the right, and the bottom.
	assert_eq!(region([13, 3], [4, 5], [16, 8], 20), None);
	assert_eq!(region([2, 4], [4, 5], [16, 8], 20), None);
	// Starting outside.
	assert_eq!(region([17, 0], [0, 0], [16, 8], 0), None);
	// Too few texels, or too many.
	assert_eq!(region([2, 3], [4, 5], [16, 8], 19), None);
	assert_eq!(region([2, 3], [4, 5], [16, 8], 21), None);
}

#[test]
fn a_region_whose_end_overflows_is_refused_not_wrapped() {
	assert_eq!(region([u32::MAX, 0], [2, 1], [16, 8], 2), None);
	assert_eq!(region([0, u32::MAX], [1, 2], [16, 8], 2), None);
	// The count never wraps to match either.
	assert_eq!(
		region([0, 0], [u32::MAX, u32::MAX], [u32::MAX, u32::MAX], 1),
		None
	);
}

#[test]
fn textures_of_no_texels_or_flat_ones_of_layers_are_refused_first() {
	let Some(device) = device() else { return };
	for size in [[0, 4, 1], [4, 0, 1], [4, 4, 0]] {
		let made = Texture::new(&device, size, true, Usage::Sampled);
		assert_eq!(made.err(), Some(Error::Region), "{size:?}");
	}
	let made = Texture::new(&device, [4, 4, 2], false, Usage::Sampled);
	assert_eq!(made.err(), Some(Error::Region));
	let atlas = Texture::new(&device, [64, 64, 4], true, Usage::Sampled).unwrap();
	assert_eq!((atlas.size, atlas.usage), ([64, 64, 4], Usage::Sampled));
	assert!(Texture::new(&device, [4, 4, 1], true, Usage::Target).is_ok());
}

#[test]
fn texels_outside_a_texture_or_its_layers_are_refused() {
	let Some(device) = device() else { return };
	let atlas = Texture::new(&device, [8, 8, 2], true, Usage::Sampled).unwrap();
	let four = [0xffu32; 4];
	assert_eq!(atlas.upload(0, [6, 6], [2, 2], &four), Ok(()));
	assert_eq!(atlas.upload(1, [0, 0], [2, 2], &four), Ok(()));
	assert_eq!(atlas.upload(2, [0, 0], [2, 2], &four), Err(Error::Region));
	assert_eq!(atlas.upload(0, [7, 6], [2, 2], &four), Err(Error::Region));
	assert_eq!(
		atlas.upload(0, [0, 0], [2, 2], &four[..3]),
		Err(Error::Region)
	);
	// Nothing to write is written nowhere.
	assert_eq!(atlas.upload(0, [8, 8], [0, 0], &[]), Ok(()));
}

#[test]
fn texels_the_cpu_writes_it_reads_back_in_place() {
	let Some(device) = device() else { return };
	let texture = Texture::new(&device, [4, 3, 1], false, Usage::Target).unwrap();
	let all: Vec<u32> = (0..12).map(|i| 0x8000_0000 | (i * 0x01_0203)).collect();
	texture.upload(0, [0, 0], [4, 3], &all).unwrap();
	// A corner over it, row by row.
	texture
		.upload(
			0,
			[2, 1],
			[2, 2],
			&[0xff11_2233, 0xff44_5566, 0xff77_8899, 0xffaa_bbcc],
		)
		.unwrap();
	// Written and read by the CPU, the texels need no synchronizing.
	let mut expected = all.clone();
	expected[6] = 0xff11_2233;
	expected[7] = 0xff44_5566;
	expected[10] = 0xff77_8899;
	expected[11] = 0xffaa_bbcc;
	assert_eq!(download(&texture), expected);
}

#[test]
fn a_texture_is_asked_for_in_bgra_managed_of_its_kind_size_and_usage() {
	use crate::gpu::mtl::fake_tests::{Made, fake, made};
	let device = fake(0);
	let texture = |kind, width, height, layers, usage| Made::Texture {
		kind,
		format: 80,
		width,
		height,
		layers,
		storage: 1,
		usage,
	};
	// The atlas: a 2D array of pages, sampled.
	Texture::new(&device, [1024, 1024, 4], true, Usage::Sampled).unwrap();
	assert_eq!(made(), [texture(3, 1024, 1024, 4, 1)]);
	// An image: flat, sampled.
	Texture::new(&device, [7, 3, 1], false, Usage::Sampled).unwrap();
	assert_eq!(made(), [texture(2, 7, 3, 1, 1)]);
	// A target: flat, drawn to and sampled.
	Texture::new(&device, [16384, 1, 1], false, Usage::Target).unwrap();
	assert_eq!(made(), [texture(2, 16384, 1, 1, 5)]);
	// An array of one page stays an array, as the shaders read the atlas.
	Texture::new(&device, [8, 8, 1], true, Usage::Sampled).unwrap();
	assert_eq!(made(), [texture(3, 8, 8, 1, 1)]);
}

#[test]
fn a_texture_refused_for_its_size_never_reaches_metal() {
	use crate::gpu::mtl::fake_tests::{fake, made};
	let device = fake(0);
	for (size, array) in [
		([0, 1, 1], true),
		([1, 0, 1], false),
		([1, 1, 0], true),
		([1, 1, 2], false),
	] {
		let refused = Texture::new(&device, size, array, Usage::Sampled);
		assert_eq!(refused.err(), Some(Error::Region), "{size:?}");
	}
	assert_eq!(made(), []);
}

#[test]
fn a_texture_metal_refuses_is_refused() {
	use crate::gpu::mtl::fake_tests::{TEXTURE, fake, made};
	let device = fake(TEXTURE);
	let refused = Texture::new(&device, [4, 4, 1], false, Usage::Target);
	assert_eq!(refused.err(), Some(Error::Refused("texture")));
	// It was asked for, once.
	assert_eq!(made().len(), 1);
}

#[test]
fn a_texture_lets_go_of_its_descriptor_once_made_and_of_itself_when_dropped() {
	use crate::{
		gpu::mtl::fake_tests::{fake, passed},
		objc::probe_tests::Weak,
	};
	let device = fake(0);
	let texture = Texture::new(&device, [4, 4, 2], true, Usage::Sampled).unwrap();
	let mut descriptors = passed();
	assert_eq!(descriptors.len(), 1);
	assert!(!descriptors[0].alive());
	let mut made = Weak::new(&texture.texture);
	assert!(made.alive());
	drop(texture);
	assert!(!made.alive());
}

#[test]
fn a_texture_metal_refuses_leaves_no_descriptor_behind() {
	use crate::gpu::mtl::fake_tests::{TEXTURE, fake, passed};
	let device = fake(TEXTURE);
	assert!(Texture::new(&device, [4, 4, 1], false, Usage::Sampled).is_err());
	let mut descriptors = passed();
	assert_eq!(descriptors.len(), 1);
	assert!(!descriptors[0].alive());
}

#[test]
fn a_region_is_one_exactly_when_every_texel_it_names_lies_inside() {
	// Against a brute-force oracle: every texel of the region, one by one.
	// Every region past the edges is tried, empty ones included.
	let bounds = [3, 2];
	for at in (0..5).flat_map(|x| (0..4).map(move |y| [x, y])) {
		for size in (0..5).flat_map(|w| (0..4).map(move |h| [w, h])) {
			let count = (size[0] * size[1]) as usize;
			let texels = (0..size[0]).flat_map(|x| (0..size[1]).map(move |y| (at[0] + x, at[1] + y)));
			let inside = texels.clone().all(|(x, y)| x < bounds[0] && y < bounds[1]);
			// An empty region names no texel, but its edges must still lie
			// within the texture's, as Metal checks.
			let edges = (0..2).all(|i| at[i] + size[i] <= bounds[i]);
			let expected = if count > 0 { inside } else { edges };
			for given in [count, count + 1, count.saturating_sub(1)] {
				let got = region(at, size, bounds, given);
				assert_eq!(
					got.is_some(),
					expected && given == count,
					"{at:?} {size:?} {given}"
				);
				if let Some(got) = got {
					assert_eq!(got.origin, [at[0] as usize, at[1] as usize, 0]);
					assert_eq!(got.size, [size[0] as usize, size[1] as usize, 1]);
				}
			}
		}
	}
}

#[test]
fn texels_written_to_one_layer_leave_the_others_as_they_were() {
	let Some(device) = device() else { return };
	let atlas = Texture::new(&device, [2, 2, 3], true, Usage::Sampled).unwrap();
	atlas.upload(0, [0, 0], [2, 2], &[0xff00_0001; 4]).unwrap();
	atlas.upload(1, [0, 0], [2, 2], &[0xff00_0002; 4]).unwrap();
	atlas.upload(2, [1, 1], [1, 1], &[0xff00_0003]).unwrap();
	// The first layer, which `download` reads, kept its own.
	assert_eq!(download(&atlas), [0xff00_0001; 4]);
}

#[test]
fn a_texel_at_each_corner_lands_at_that_corner() {
	let Some(device) = device() else { return };
	let (width, height) = (64u32, 48u32);
	let texture = Texture::new(&device, [width, height, 1], false, Usage::Target).unwrap();
	texture
		.upload(
			0,
			[0, 0],
			[width, height],
			&vec![0; (width * height) as usize],
		)
		.unwrap();
	let corners = [
		(0, 0),
		(width - 1, 0),
		(0, height - 1),
		(width - 1, height - 1),
	];
	for (i, &(x, y)) in corners.iter().enumerate() {
		texture
			.upload(0, [x, y], [1, 1], &[0xff00_0000 | (i as u32 + 1)])
			.unwrap();
	}
	let read = download(&texture);
	for (i, &(x, y)) in corners.iter().enumerate() {
		assert_eq!(
			read[(y * width + x) as usize],
			0xff00_0000 | (i as u32 + 1),
			"{x}, {y}"
		);
	}
	assert_eq!(read.iter().filter(|&&texel| texel != 0).count(), 4);
}
