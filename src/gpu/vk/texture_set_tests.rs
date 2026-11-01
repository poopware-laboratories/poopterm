//! Tests of the image texture sets against the fake driver of
//! `driver_tests`: each from a pool of its own, of the pipeline's texture
//! layout, written once to point at its image read only by shaders, holding
//! the image while it lives, and failures releasing what was made. The
//! real driver makes and writes one too, under the validation layer.
use super::{
	buffer_tests::{INTEL, device},
	call::Error,
	descriptor_driver_tests::*,
	descriptor_types::*,
	descriptors::TextureSet,
	driver_tests::{finish, with},
	image::{Format, Image},
	loader_tests::{entry, offscreen},
	pipeline::QuadPipeline,
};
use std::rc::Rc;

/// `VkDescriptorType`: a combined image sampler.
const COMBINED_IMAGE_SAMPLER: i32 = 1;
/// `VK_IMAGE_USAGE_SAMPLED_BIT` and `_TRANSFER_DST_BIT`.
const TEXTURE: u32 = 0x4 | 0x2;

/// The quad pipeline on the fake Intel device.
fn pipeline() -> Rc<QuadPipeline> {
	let device = device(&INTEL, |_| {});
	Rc::new(QuadPipeline::new(&device, Format::Bgra).unwrap())
}

/// A 2×2 image to sample, on `pipeline`'s device.
fn image(pipeline: &QuadPipeline) -> Rc<Image> {
	Rc::new(Image::new(&pipeline.device, Format::Bgra, [2, 2, 1], TEXTURE).unwrap())
}

/// The descriptor writes recorded.
fn writes() -> Vec<String> {
	with(|driver| {
		(driver.recorded.iter())
			.filter(|call| call.starts_with("write"))
			.cloned()
			.collect()
	})
}

#[test]
fn a_texture_set_comes_from_a_pool_of_its_own_and_points_at_its_image() {
	let pipeline = pipeline();
	let image = image(&pipeline);
	let texture = TextureSet::new(&pipeline, &image).unwrap();
	assert_eq!(texture.handle(), SETS + 1);
	assert!(Rc::ptr_eq(texture.pipeline(), &pipeline));
	assert!(Rc::ptr_eq(&texture.image, &image));
	// Written once, made: the image's view, in the shader read only layout.
	assert_eq!(writes(), ["write 0x660001 binding 0: 0xf00001 layout 0x5"]);
	drop((texture, image, pipeline));
	let objects = finish().pipeline_objects;
	// One combined image sampler, of the texture layout, the pipeline's
	// second set layout.
	let sizes = vec![DescriptorPoolSize {
		descriptor_type: COMBINED_IMAGE_SAMPLER,
		count: 1,
	}];
	assert_eq!(objects.pools, [(1, sizes)]);
	assert_eq!(objects.sets, [(POOLS + 1, 0x62_0002)]);
}

#[test]
fn each_texture_set_is_its_own_and_writes_only_itself() {
	let pipeline = pipeline();
	let (first, second) = (image(&pipeline), image(&pipeline));
	let a = TextureSet::new(&pipeline, &first).unwrap();
	let b = TextureSet::new(&pipeline, &second).unwrap();
	assert_eq!((a.handle(), b.handle()), (SETS + 1, SETS + 2));
	let expected = [
		"write 0x660001 binding 0: 0xf00001 layout 0x5",
		"write 0x660002 binding 0: 0xf00002 layout 0x5",
	];
	assert_eq!(writes(), expected);
	// The first goes alone, with its own pool.
	drop(a);
	let released = with(|driver| driver.released.split_off(2));
	assert_eq!(released, ["descriptor pool 0x650001"]);
	drop((b, first, second, pipeline));
	finish();
}

#[test]
fn a_texture_set_holds_its_image_until_it_goes() {
	let pipeline = pipeline();
	let image = image(&pipeline);
	let texture = TextureSet::new(&pipeline, &image).unwrap();
	drop(image);
	let live = || with(|driver| (driver.live_images.len(), driver.live_views.len()));
	assert_eq!(live(), (1, 1), "the set still points at it");
	drop(texture);
	assert_eq!(live(), (0, 0));
	drop(pipeline);
	finish();
}

#[test]
fn a_texture_set_that_fails_writes_nothing_and_releases_what_it_made() {
	let pipeline = pipeline();
	let image = image(&pipeline);
	with(|driver| driver.fails.push((c"vkCreateDescriptorPool", -2)));
	let failed = TextureSet::new(&pipeline, &image);
	assert_eq!(failed.err(), Some(Error::Failed(-2)));
	with(|driver| {
		assert!(driver.pipeline_objects.pools.is_empty());
		driver.fails.clear();
		driver
			.fails
			.push((c"vkAllocateDescriptorSets", OUT_OF_POOL_MEMORY));
	});
	let failed = TextureSet::new(&pipeline, &image);
	assert_eq!(failed.err(), Some(Error::Failed(OUT_OF_POOL_MEMORY)));
	let released = with(|driver| driver.released.split_off(2));
	assert_eq!(released, ["descriptor pool 0x650001"]);
	// Neither kept the image, nor wrote a set.
	assert_eq!(Rc::strong_count(&image), 1);
	assert!(writes().is_empty());
	drop((image, pipeline));
	assert!(finish().pipeline_objects.sets.is_empty());
}

#[test]
fn a_real_device_makes_and_writes_a_texture_set() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	let pipeline = Rc::new(QuadPipeline::new(&device, Format::Bgra).expect("pipeline"));
	let image = Image::new(&device, Format::Bgra, [2, 2, 1], TEXTURE).expect("image");
	let texture = TextureSet::new(&pipeline, &Rc::new(image)).expect("set");
	assert_ne!(texture.handle(), 0);
}
