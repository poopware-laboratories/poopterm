//! Which memory type to allocate from for what the memory is used for:
//! from plain data about the device's types, so it is tested without a
//! driver.

/// `VkMemoryPropertyFlagBits`.
pub(crate) const DEVICE_LOCAL: u32 = 1;
pub(crate) const HOST_VISIBLE: u32 = 2;
pub(crate) const HOST_COHERENT: u32 = 4;
pub(crate) const HOST_CACHED: u32 = 8;
/// Lazily allocated (only for transient attachments), protected, and
/// AMD's device-coherent and uncached memory (slow): never used.
const AVOIDED: u32 = 0x10 | 0x20 | 0x40 | 0x80;

/// `VkMemoryType`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct MemoryType {
	pub flags: u32,
	pub heap: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Usage {
	/// Written by the CPU and read by the GPU, such as instances and
	/// glyphs: coherent, so writes need no flush, and in the GPU's own
	/// memory when the CPU can write there (unified memory, a BAR).
	Upload,
	/// Used by the GPU alone: in its own memory, out of the CPU's way, if
	/// it has any.
	Device,
	/// Written by the GPU and read by the CPU, such as pixels for tests:
	/// coherent, so reading needs no invalidate, and cached if it can be,
	/// so reading is fast.
	Readback,
}

/// The index of the type in `types` to allocate from for `usage`, among
/// those `allowed` (a resource's `memoryTypeBits`): the most preferred,
/// then the first. `None` when none will do.
pub(crate) fn choose_memory(types: &[MemoryType], allowed: u32, usage: Usage) -> Option<u32> {
	let candidates = (types.iter().take(32).enumerate()).filter_map(|(index, kind)| {
		let has = |bit| kind.flags & bit != 0;
		let usable = allowed >> index & 1 != 0 && kind.flags & AVOIDED == 0;
		let (wanted, rank) = match usage {
			Usage::Upload => (
				has(HOST_VISIBLE) && has(HOST_COHERENT),
				!has(DEVICE_LOCAL) as u8,
			),
			Usage::Device => (true, 2 * !has(DEVICE_LOCAL) as u8 + has(HOST_VISIBLE) as u8),
			Usage::Readback => (
				has(HOST_VISIBLE) && has(HOST_COHERENT),
				!has(HOST_CACHED) as u8,
			),
		};
		(usable && wanted).then_some((rank, index as u32))
	});
	candidates.min().map(|(_, index)| index)
}
