//! Tests of `VK_EXT_memory_budget`: the structures laid out as the
//! specification declares them, and the atlas kept to half the memory its
//! heap has free, never past the fixed cap.
use super::{driver_tests::with, memory_budget::*, types::RawPhysicalDevice};
use crate::gpu::atlas::BUDGET;
use std::mem::{offset_of, size_of};

const MB: u64 = 1 << 20;

#[test]
fn the_budget_structure_is_laid_out_as_vulkan_declares_it() {
	assert_eq!(size_of::<MemoryBudget>(), 16 + 2 * 16 * 8);
	assert_eq!(offset_of!(MemoryBudget, heap_budget), 16);
	assert_eq!(offset_of!(MemoryBudget, heap_usage), 16 + 16 * 8);
	// `VkPhysicalDeviceMemoryProperties2`: the 1.0 structure after the link.
	assert_eq!(offset_of!(MemoryProperties2, properties), 16);
	assert_eq!(
		STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_PROPERTIES_2,
		1_000_059_006
	);
	assert_eq!(
		STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_BUDGET_PROPERTIES,
		1_000_237_000
	);
}

#[test]
fn without_a_budget_the_atlas_keeps_the_fixed_cap() {
	assert_eq!(atlas_budget(None, 0), BUDGET);
	assert_eq!(atlas_budget(None, 64 << 20), BUDGET);
}

#[test]
fn the_atlas_takes_at_most_half_of_what_its_heap_has_free() {
	let cases = [
		// Plenty free: the fixed cap.
		(8192 * MB, 16 << 20, BUDGET),
		(512 * MB, 0, 256 << 20),
		// Little free: half of it, counting the atlas being replaced.
		(100 * MB, 16 << 20, 58 << 20),
		(100 * MB, 0, 50 << 20),
		(0, 16 << 20, 8 << 20),
		(0, 0, 0),
	];
	for (free, current, budget) in cases {
		assert_eq!(
			atlas_budget(Some(free), current),
			budget,
			"{free} {current}"
		);
	}
}

#[test]
fn huge_or_overflowing_numbers_stay_at_the_cap() {
	assert_eq!(atlas_budget(Some(u64::MAX), usize::MAX), BUDGET);
	assert_eq!(atlas_budget(Some(u64::MAX), 0), BUDGET);
}

#[test]
fn free_memory_is_the_budget_less_the_usage_never_below_zero() {
	let mut budget = MemoryBudget::empty();
	budget.heap_budget[1] = 900 * MB;
	budget.heap_usage[1] = 300 * MB;
	budget.heap_budget[2] = 100 * MB;
	// Other programs may push the usage past the budget.
	budget.heap_usage[2] = 150 * MB;
	assert_eq!(budget.free(1), Some(600 * MB));
	assert_eq!(budget.free(2), Some(0));
	// A heap the driver said nothing of is unknown.
	assert_eq!(budget.free(0), None);
	assert_eq!(budget.free(16), None);
}

/// `vkGetPhysicalDeviceMemoryProperties2`, the fake's: the 1.0 properties,
/// and the budget of `heap_budgets` into a budget structure chained, which
/// only a device offering `VK_EXT_memory_budget` may be asked for.
pub(super) unsafe extern "system" fn memory_properties2(
	device: RawPhysicalDevice,
	chain: *mut MemoryProperties2,
) {
	// SAFETY: the bindings pass a writable chain.
	let head = unsafe { &mut *chain };
	const CALL: &str = "vkGetPhysicalDeviceMemoryProperties2";
	if head.s_type != STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_PROPERTIES_2 {
		with(|driver| driver.invalid.push(format!("{CALL}: sType")));
	}
	// SAFETY: as above.
	unsafe { super::memory_driver_tests::memory_properties(device, &mut head.properties) };
	let next = head.next;
	if next.is_null() {
		return;
	}
	// SAFETY: every chained structure starts with its sType.
	if unsafe { *next.cast::<i32>() } != STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_BUDGET_PROPERTIES {
		with(|driver| driver.invalid.push(format!("{CALL}: an unknown structure")));
		return;
	}
	// SAFETY: a structure of this type is a budget.
	let budget = unsafe { &mut *next.cast::<MemoryBudget>() };
	with(|driver| {
		let offered = driver.device_extensions.contains(&"VK_EXT_memory_budget");
		driver.require(offered, || format!("{CALL}: a budget the device offers"));
		driver.require(budget.next.is_null(), || format!("{CALL}: one structure"));
		for (heap, &(given, used)) in driver.heap_budgets.iter().enumerate() {
			budget.heap_budget[heap] = given;
			budget.heap_usage[heap] = used;
		}
	});
}

#[test]
fn the_atlas_budget_agrees_with_a_model_and_only_grows_with_free_memory() {
	let mut state = 0x9b05_688c_2b3e_6c1f_u64;
	let mut next = || {
		state ^= state << 13;
		state ^= state >> 7;
		state ^= state << 17;
		state
	};
	for case in 0..5000 {
		// Free memory from nothing to past the cap, in pages of 4 KB.
		let free = (next() % (1 << 18)) << 12;
		let current = (next() % (1 << 16)) as usize * 4096;
		let budget = atlas_budget(Some(free), current);
		let model = BUDGET.min((free as usize + current) / 2);
		assert_eq!(budget, model, "case {case}: {free} {current}");
		assert!(budget <= BUDGET, "case {case}");
		// More free memory never makes for a smaller atlas.
		let more = atlas_budget(Some(free + (next() % (1 << 20))), current);
		assert!(more >= budget, "case {case}: {more} < {budget}");
	}
}

#[test]
fn an_atlas_grown_again_and_again_stays_within_its_budget() {
	use crate::gpu::atlas::{Wanted, grown};
	let more_pages = Wanted {
		side: 0,
		pages: true,
	};
	for free in [0, 20 * MB, 100 * MB, 300 * MB, 8192 * MB] {
		let (mut atlas, mut steps) = ((1024, 4), 0);
		loop {
			let bytes = atlas.0 * atlas.0 * 4 * atlas.1;
			// The atlas being replaced counts as free.
			let budget = atlas_budget(Some(free), bytes);
			let Some(next) = grown(atlas, more_pages, [16384, 2048], budget) else {
				break;
			};
			assert!(next.0 * next.0 * 4 * next.1 <= budget, "{free}: {next:?}");
			(atlas, steps) = (next, steps + 1);
		}
		// With plenty free it grows to the fixed cap: 64 pages of 4 MB.
		let expected = if free >= 8192 * MB { (1024, 64) } else { atlas };
		assert_eq!(atlas, expected, "{free} after {steps}");
	}
}

#[test]
fn a_heap_index_past_what_the_structure_holds_is_unknown() {
	let mut budget = MemoryBudget::empty();
	budget.heap_budget = [MB; 16];
	assert_eq!(budget.free(15), Some(MB));
	assert_eq!(budget.free(16), None);
	assert_eq!(budget.free(usize::MAX), None);
}
