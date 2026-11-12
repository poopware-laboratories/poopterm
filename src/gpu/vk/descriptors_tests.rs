//! Tests of the quad pipeline's descriptor set against the fake driver of
//! `driver_tests`: the pool sized for the quad set layout, the set
//! allocated from it, the order they go in, and failures, and that the
//! fake functions of `descriptor_driver_tests` report each rule they
//! check. The real driver allocates the set too, under the validation
//! layer.
use super::handle::RawDescriptorSetLayout;
use super::{
	buffer_tests::{INTEL, device},
	call::Error,
	choose::{Device as Described, GRAPHICS, Kind},
	descriptor_driver_tests::*,
	descriptor_types::*,
	descriptors::QuadSet,
	device::Device,
	driver_tests::{family, finish, install, with, with_devices},
	image::Format,
	loader_tests::{entry, offscreen},
	pipeline::QuadPipeline,
	version::api_version,
};
use std::{ptr, rc::Rc};

/// `VkDescriptorType`: a combined image sampler and a storage buffer.
const COMBINED_IMAGE_SAMPLER: i32 = 1;
const STORAGE_BUFFER: i32 = 7;

/// The quad pipeline on the fake Intel device.
fn pipeline() -> Rc<QuadPipeline> {
	let device = device(&INTEL, |_| {});
	Rc::new(QuadPipeline::new(&device, Format::Bgra).unwrap())
}

#[test]
fn the_set_comes_from_a_pool_with_room_for_it_alone() {
	let pipeline = pipeline();
	let set = QuadSet::new(&pipeline).unwrap();
	assert_eq!(set.handle(), RawDescriptorSet(SETS + 1));
	drop(set);
	let released = with(|driver| driver.released.split_off(2));
	assert_eq!(released, ["descriptor pool 0x650001"]);
	drop(pipeline);
	let objects = finish().pipeline_objects;
	// One set, one storage buffer and one combined image sampler: the quad
	// set layout's bindings.
	let sizes = vec![
		DescriptorPoolSize {
			descriptor_type: STORAGE_BUFFER,
			count: 1,
		},
		DescriptorPoolSize {
			descriptor_type: COMBINED_IMAGE_SAMPLER,
			count: 1,
		},
	];
	assert_eq!(objects.pools, [(1, sizes)]);
	assert_eq!(
		objects.sets,
		[(
			RawDescriptorPool(POOLS + 1),
			RawDescriptorSetLayout(0x62_0001)
		)]
	);
}

#[test]
fn a_set_keeps_its_pipeline_and_device_alive_and_goes_first() {
	let set = QuadSet::new(&pipeline()).unwrap();
	assert!(
		with(|driver| driver.released.len()) == 2,
		"only the modules went"
	);
	drop(set);
	let driver = finish();
	assert_eq!(
		driver.released[2..],
		[
			"descriptor pool 0x650001",
			"pipeline 0x640001",
			"pipeline layout 0x630001",
			"set layout 0x620001",
			"set layout 0x620002",
			"sampler 0x610001",
		]
	);
	assert_eq!(driver.destroyed_devices.len(), 1);
}

#[test]
fn a_failure_releases_what_was_made_before_it() {
	let pipeline = pipeline();
	with(|driver| driver.fails.push((c"vkCreateDescriptorPool", -2)));
	assert_eq!(QuadSet::new(&pipeline).err(), Some(Error::Failed(-2)));
	with(|driver| {
		assert!(driver.pipeline_objects.pools.is_empty());
		driver.fails.clear();
		driver
			.fails
			.push((c"vkAllocateDescriptorSets", OUT_OF_POOL_MEMORY));
	});
	let failed = QuadSet::new(&pipeline);
	assert_eq!(failed.err(), Some(Error::Failed(OUT_OF_POOL_MEMORY)));
	let released = with(|driver| driver.released.split_off(2));
	assert_eq!(released, ["descriptor pool 0x650001"]);
	drop(pipeline);
	assert!(finish().pipeline_objects.sets.is_empty());
}

#[test]
fn a_device_missing_a_descriptor_function_is_not_made() {
	let names = [
		c"vkCreateDescriptorPool",
		c"vkDestroyDescriptorPool",
		c"vkAllocateDescriptorSets",
	];
	for name in names {
		let mut driver = with_devices(&[Described {
			name: "lacking".into(),
			kind: Kind::Integrated,
			api: api_version(1, 3),
			families: vec![family(GRAPHICS, 1)],
			swapchain: true,
		}]);
		driver.lacks.push(name);
		let instance = Rc::new(install(driver).create_instance(c"poopterm").unwrap());
		let physical = instance.physical_devices().unwrap()[0];
		let made = Device::new(instance, physical, 0);
		assert_eq!(made.err(), Some(Error::Missing(name)));
		assert_eq!(finish().destroyed_devices.len(), 1, "{name:?}");
	}
}

/// A pool create info for `max_sets` sets and `sizes`.
fn pool_info(max_sets: u32, flags: u32, sizes: &[DescriptorPoolSize]) -> DescriptorPoolCreateInfo {
	DescriptorPoolCreateInfo {
		s_type: STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
		next: ptr::null(),
		flags,
		max_sets,
		pool_size_count: sizes.len() as u32,
		pool_sizes: sizes.as_ptr(),
	}
}

/// An allocate info for one set of each of `layouts` from `pool`.
fn allocate_info(
	pool: RawDescriptorPool,
	layouts: &[RawDescriptorSetLayout],
) -> DescriptorSetAllocateInfo {
	DescriptorSetAllocateInfo {
		s_type: STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
		next: ptr::null(),
		pool,
		set_count: layouts.len() as u32,
		set_layouts: layouts.as_ptr(),
	}
}

#[test]
fn a_pool_runs_out_of_sets_and_descriptors_as_a_driver_would() {
	// Running out is a result the driver answers, not a broken rule.
	let pipeline = pipeline();
	let handle = with(|driver| driver.live_devices[0]);
	let layout = pipeline.set_layout();
	let size = |descriptor_type, count| DescriptorPoolSize {
		descriptor_type,
		count,
	};
	let (mut pool, mut sets) = (RawDescriptorPool::NULL, [RawDescriptorSet::NULL; 2]);
	// The sizes live in their own bindings: the infos point at them.
	let sizes = [
		// Room for two sets: the second allocation has none left.
		vec![size(STORAGE_BUFFER, 2), size(COMBINED_IMAGE_SAMPLER, 2)],
		// Room for one set's samplers but not its buffer.
		vec![size(COMBINED_IMAGE_SAMPLER, 1)],
		// Room for one buffer of two sets' worth.
		vec![size(STORAGE_BUFFER, 1), size(COMBINED_IMAGE_SAMPLER, 2)],
	];
	let pools =
		[(2, &sizes[0]), (1, &sizes[1]), (2, &sizes[2])].map(|(max, sizes)| pool_info(max, 0, sizes));
	let mut results = Vec::new();
	for info in &pools {
		// SAFETY: the infos and handles outlive each call.
		unsafe { create_pool(handle, info, ptr::null(), &mut pool) };
		let one = [layout];
		let one = allocate_info(pool, &one);
		// SAFETY: as above.
		unsafe { results.push(allocate_sets(handle, &one, sets.as_mut_ptr())) };
		// SAFETY: as above.
		unsafe { results.push(allocate_sets(handle, &one, sets.as_mut_ptr())) };
		// SAFETY: as above.
		unsafe { destroy_pool(handle, pool, ptr::null()) };
	}
	// Two sets in one call.
	// SAFETY: as above.
	unsafe { create_pool(handle, &pools[0], ptr::null(), &mut pool) };
	let two = [layout, layout];
	let two = allocate_info(pool, &two);
	// SAFETY: as above.
	unsafe { results.push(allocate_sets(handle, &two, sets.as_mut_ptr())) };
	// SAFETY: as above.
	unsafe { destroy_pool(handle, pool, ptr::null()) };
	let full = OUT_OF_POOL_MEMORY;
	assert_eq!(results, [0, 0, full, full, 0, full, 0]);
	assert_eq!(sets.map(|set| set.0), [SETS + 4, SETS + 5]);
	drop(pipeline);
	finish();
}

#[test]
fn the_fake_driver_reports_broken_descriptor_rules() {
	// Each case breaks one rule the fake checks; were it blind to them,
	// the tests above would prove nothing.
	let pipeline = pipeline();
	let handle = with(|driver| driver.live_devices[0]);
	let layout = pipeline.set_layout();
	let buffers = |count, descriptor_type| {
		[DescriptorPoolSize {
			descriptor_type,
			count,
		}]
	};
	let good = buffers(1, STORAGE_BUFFER);
	let (mut pool, mut set) = (RawDescriptorPool::NULL, RawDescriptorSet::NULL);
	let mut wrong_type = pool_info(1, 0, &good);
	wrong_type.s_type = 0;
	// SAFETY: the infos and handles outlive each call; the bad handles are
	// only compared.
	unsafe { create_pool(handle, &wrong_type, ptr::null(), &mut pool) };
	// SAFETY: as above.
	unsafe { create_pool(handle, &pool_info(1, 1, &good), ptr::null(), &mut pool) };
	// SAFETY: as above.
	unsafe { create_pool(handle, &pool_info(0, 0, &good), ptr::null(), &mut pool) };
	// SAFETY: as above.
	unsafe { create_pool(handle, &pool_info(1, 0, &[]), ptr::null(), &mut pool) };
	// SAFETY: as above.
	unsafe {
		create_pool(
			handle,
			&pool_info(1, 0, &buffers(0, STORAGE_BUFFER)),
			ptr::null(),
			&mut pool,
		);
	}
	// SAFETY: as above.
	unsafe {
		create_pool(
			handle,
			&pool_info(1, 0, &buffers(1, 11)),
			ptr::null(),
			&mut pool,
		);
	}
	let one = [layout];
	let mut wrong_type = allocate_info(pool, &one);
	wrong_type.s_type = 0;
	// SAFETY: as above.
	unsafe { allocate_sets(handle, &wrong_type, &mut set) };
	// SAFETY: as above.
	unsafe {
		allocate_sets(
			handle,
			&allocate_info(RawDescriptorPool(POOLS + 9), &[layout]),
			&mut set,
		)
	};
	// SAFETY: as above.
	unsafe { allocate_sets(handle, &allocate_info(pool, &[]), &mut set) };
	// SAFETY: as above.
	unsafe {
		allocate_sets(
			handle,
			&allocate_info(pool, &[RawDescriptorSetLayout(0x62_0009)]),
			&mut set,
		)
	};
	// SAFETY: as above.
	unsafe { destroy_pool(handle, RawDescriptorPool(POOLS + 9), ptr::null()) };
	let invalid = with(|driver| std::mem::take(&mut driver.invalid));
	assert_eq!(
		invalid,
		[
			"vkCreateDescriptorPool: sType",
			"vkCreateDescriptorPool: pNext and flags",
			"vkCreateDescriptorPool: room for a set",
			"vkCreateDescriptorPool: sizes",
			"vkCreateDescriptorPool: a known type and a count in each size",
			"vkCreateDescriptorPool: a known type and a count in each size",
			"vkAllocateDescriptorSets: sType and pNext",
			"vkAllocateDescriptorSets: a live pool",
			"vkAllocateDescriptorSets: a set",
			"vkAllocateDescriptorSets: live set layouts",
			"vkDestroyDescriptorPool: 0x650009, which is not live",
		]
	);
	// The pools the broken calls made are still live.
	let made = with(|driver| driver.pipeline_objects.pools.len() as u64);
	// SAFETY: as above.
	for pool in POOLS + 1..=POOLS + made {
		// SAFETY: as above.
		unsafe { destroy_pool(handle, RawDescriptorPool(pool), ptr::null()) };
	}
	drop(pipeline);
	finish();
}

#[test]
fn a_real_device_allocates_the_quad_set() {
	let Some(entry) = entry() else { return };
	let instance = Rc::new(entry.create_instance(c"poopterm tests").expect("instance"));
	let device = Rc::new(offscreen(instance).expect("device"));
	let pipeline = Rc::new(QuadPipeline::new(&device, Format::Bgra).expect("pipeline"));
	let set = QuadSet::new(&pipeline).expect("set");
	assert_ne!(set.handle(), RawDescriptorSet::NULL);
}
