//! The fake driver's descriptor pool and set functions, for
//! `descriptors_tests`: each checks the specification's valid usage rules,
//! records the pools and sets made, runs out of room as a pool would, and
//! frees a pool's sets with it.
use super::{
	descriptor_types::*,
	driver_tests::{Driver, with},
	pipeline_driver_tests::SET_LAYOUTS,
	pipeline_types::RawDescriptorSetLayout,
	types::*,
};
use std::{
	ffi::{CStr, c_void},
	mem::transmute,
	slice,
};

/// The handles pools and sets are numbered from.
pub(super) const POOLS: u64 = 0x65_0000;
pub(super) const SETS: u64 = 0x66_0000;
/// `VK_ERROR_OUT_OF_POOL_MEMORY`: what a pool without room answers.
pub(super) const OUT_OF_POOL_MEMORY: i32 = -1_000_069_000;
/// `VK_DESCRIPTOR_TYPE_INPUT_ATTACHMENT`, the last of the core types.
const LAST_TYPE: i32 = 10;

/// The fake's descriptor function `name`, if it has it.
pub(super) fn function(name: &CStr) -> Option<VoidFunction> {
	// SAFETY: the bindings cast each back to its own type.
	unsafe {
		Some(match name.to_bytes() {
			b"vkCreateDescriptorPool" => transmute::<CreateDescriptorPool, VoidFunction>(create_pool),
			b"vkDestroyDescriptorPool" => transmute::<DestroyDescriptorPool, VoidFunction>(destroy_pool),
			b"vkAllocateDescriptorSets" => {
				transmute::<AllocateDescriptorSets, VoidFunction>(allocate_sets)
			}
			_ => return None,
		})
	}
}

impl Driver {
	/// The descriptors of each type that the live set layout `layout`
	/// holds, if it is one.
	fn descriptors(&self, layout: RawDescriptorSetLayout) -> Option<Vec<(i32, u32)>> {
		if !self.live_object(layout, SET_LAYOUTS) {
			return None;
		}
		let bindings = &self.pipeline_objects.set_layouts[(layout - SET_LAYOUTS - 1) as usize];
		Some(
			bindings
				.iter()
				.map(|&(_, kind, count, ..)| (kind, count))
				.collect(),
		)
	}

	/// Whether `pool` has room for one more set of each of `layouts`.
	fn room(&self, pool: RawDescriptorPool, layouts: &[RawDescriptorSetLayout]) -> bool {
		let (most, sizes) = &self.pipeline_objects.pools[(pool - POOLS - 1) as usize];
		let sets = &self.pipeline_objects.sets;
		let held: Vec<_> = (sets.iter())
			.filter(|&&(of, _)| of == pool)
			.map(|&(_, layout)| layout)
			.chain(layouts.iter().copied())
			.collect();
		let wanted: Vec<_> = (held.iter())
			.flat_map(|&layout| self.descriptors(layout).unwrap_or_default())
			.collect();
		let fits = |size: &DescriptorPoolSize| {
			let of_type = wanted
				.iter()
				.filter(|&&(kind, _)| kind == size.descriptor_type);
			of_type.map(|&(_, count)| count).sum::<u32>() <= size.count
		};
		let known = wanted
			.iter()
			.all(|&(kind, _)| sizes.iter().any(|size| size.descriptor_type == kind));
		held.len() <= *most as usize && known && sizes.iter().all(fits)
	}
}

pub(super) unsafe extern "system" fn create_pool(
	device: RawDevice,
	info: *const DescriptorPoolCreateInfo,
	allocator: *const c_void,
	pool: *mut RawDescriptorPool,
) -> i32 {
	// SAFETY: the bindings pass a create info whose sizes live across the
	// call.
	let (info, sizes) = unsafe {
		let info = &*info;
		let count = info.pool_size_count as usize;
		let sizes = match count {
			0 => &[][..],
			_ => slice::from_raw_parts(info.pool_sizes, count),
		};
		(info, sizes)
	};
	with(|driver| {
		let call = c"vkCreateDescriptorPool";
		let rules = [
			(info.next.is_null() && info.flags == 0, "pNext and flags"),
			(info.max_sets > 0, "room for a set"),
			(!sizes.is_empty(), "sizes"),
			(
				(sizes.iter())
					.all(|size| size.count > 0 && (0..=LAST_TYPE).contains(&size.descriptor_type)),
				"a known type and a count in each size",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkCreateDescriptorPool: {rule}"));
		}
		let what = (
			info.s_type,
			STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
			allocator,
		);
		let count = driver.pipeline_objects.pools.len();
		match driver.make_object(device, call, what, POOLS, count) {
			Ok(handle) => {
				(driver.pipeline_objects.pools).push((info.max_sets, sizes.to_vec()));
				// SAFETY: the bindings pass a writable handle.
				unsafe { *pool = handle };
				0
			}
			Err(code) => code,
		}
	})
}

pub(super) unsafe extern "system" fn destroy_pool(
	device: RawDevice,
	pool: RawDescriptorPool,
	allocator: *const c_void,
) {
	with(|driver| {
		let what = (pool, POOLS, allocator);
		driver.release_object(device, what, "vkDestroyDescriptorPool", "descriptor pool");
	})
}

pub(super) unsafe extern "system" fn allocate_sets(
	device: RawDevice,
	info: *const DescriptorSetAllocateInfo,
	sets: *mut RawDescriptorSet,
) -> i32 {
	// SAFETY: the bindings pass an allocate info whose layouts live across
	// the call, one a set.
	let (info, layouts) = unsafe {
		let info = &*info;
		let count = info.set_count as usize;
		let layouts = match count {
			0 => &[][..],
			_ => slice::from_raw_parts(info.set_layouts, count),
		};
		(info, layouts)
	};
	with(|driver| {
		driver.check_live_device(device, "vkAllocateDescriptorSets");
		let live_pool = driver.live_object(info.pool, POOLS);
		let rules = [
			(
				info.s_type == STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO && info.next.is_null(),
				"sType and pNext",
			),
			(live_pool, "a live pool"),
			(!layouts.is_empty(), "a set"),
			(
				(layouts.iter()).all(|&layout| driver.descriptors(layout).is_some()),
				"live set layouts",
			),
		];
		for (holds, rule) in rules {
			driver.require(holds, || format!("vkAllocateDescriptorSets: {rule}"));
		}
		if let Some(code) = driver.failing(c"vkAllocateDescriptorSets") {
			return code;
		}
		if !live_pool || !driver.room(info.pool, layouts) {
			return OUT_OF_POOL_MEMORY;
		}
		for (index, &layout) in layouts.iter().enumerate() {
			let handle = SETS + driver.pipeline_objects.sets.len() as u64 + 1;
			driver.pipeline_objects.sets.push((info.pool, layout));
			// SAFETY: the bindings pass room for one handle a layout.
			unsafe { *sets.add(index) = handle };
		}
		0
	})
}
