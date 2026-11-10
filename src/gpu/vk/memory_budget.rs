//! `VK_EXT_memory_budget`: how much of each heap the driver lets this
//! program use and how much it uses, read from the device when the glyph
//! atlas grows, so the atlas never takes more than half of what is free.
use super::types::{MemoryProperties, RawPhysicalDevice};
use crate::gpu::atlas::BUDGET;
use std::{ffi::c_void, ptr};

pub(crate) const STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_PROPERTIES_2: i32 = 1_000_059_006;
pub(crate) const STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_BUDGET_PROPERTIES: i32 = 1_000_237_000;

/// `VkPhysicalDeviceMemoryProperties2`.
#[repr(C)]
pub(crate) struct MemoryProperties2 {
	pub s_type: i32,
	pub next: *mut c_void,
	pub properties: MemoryProperties,
}

/// `VkPhysicalDeviceMemoryBudgetPropertiesEXT`: per heap, the bytes this
/// program may use and those it uses.
#[repr(C)]
pub(crate) struct MemoryBudget {
	pub s_type: i32,
	pub next: *mut c_void,
	pub heap_budget: [u64; 16],
	pub heap_usage: [u64; 16],
}

/// `vkGetPhysicalDeviceMemoryProperties2`.
pub(crate) type GetPhysicalDeviceMemoryProperties2 =
	unsafe extern "system" fn(RawPhysicalDevice, *mut MemoryProperties2);

impl MemoryBudget {
	/// Nothing known, for the driver to fill in.
	pub(crate) fn empty() -> Self {
		Self {
			s_type: STRUCTURE_TYPE_PHYSICAL_DEVICE_MEMORY_BUDGET_PROPERTIES,
			next: ptr::null_mut(),
			heap_budget: [0; 16],
			heap_usage: [0; 16],
		}
	}

	/// The bytes `heap` has free for this program; `None` when the driver
	/// gave it no budget.
	pub(crate) fn free(&self, heap: usize) -> Option<u64> {
		let budget = *self.heap_budget.get(heap).filter(|&&budget| budget > 0)?;
		Some(budget.saturating_sub(self.heap_usage[heap]))
	}
}

/// The bytes the atlas may take: half of what its heap has `free`, counting
/// the `current` atlas it replaces, leaving the rest to everything else;
/// never more than [`BUDGET`], which is all there is without a budget.
pub(crate) fn atlas_budget(free: Option<u64>, current: usize) -> usize {
	let Some(free) = free else {
		return BUDGET;
	};
	let free = usize::try_from(free).unwrap_or(usize::MAX);
	BUDGET.min(free.saturating_add(current) / 2)
}
