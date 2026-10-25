//! Tests of choosing a memory type: on the memory real drivers report, with
//! the types a resource allows, and against a reference written as tiers
//! of preference on random memory.
use super::memory::{
	DEVICE_LOCAL, HOST_CACHED, HOST_COHERENT, HOST_VISIBLE, MemoryType, Usage, choose_memory,
};

const ALL: u32 = u32::MAX;
const USAGES: [Usage; 3] = [Usage::Upload, Usage::Device, Usage::Readback];

/// Memory types with these property flags, each on the heap that follows
/// from them: 0 for device-local memory, 1 for the rest.
fn types(flags: &[u32]) -> Vec<MemoryType> {
	(flags.iter())
		.map(|&flags| MemoryType {
			flags,
			heap: u32::from(flags & DEVICE_LOCAL == 0),
		})
		.collect()
}

/// The type chosen for each usage, in the order of `USAGES`.
fn chosen(types: &[MemoryType], allowed: u32) -> [Option<u32>; 3] {
	USAGES.map(|usage| choose_memory(types, allowed, usage))
}

#[test]
fn an_integrated_gpu_uploads_into_its_own_memory_and_reads_back_cached() {
	// Intel ADL-N on Mesa: device-local only, + host-visible coherent,
	// + cached, protected, device-local again.
	let intel = types(&[0x1, 0x7, 0xf, 0x21, 0x1]);
	assert_eq!(chosen(&intel, ALL), [Some(1), Some(0), Some(2)]);
	// llvmpipe: one type that is everything.
	assert_eq!(chosen(&types(&[0xf]), ALL), [Some(0); 3]);
}

#[test]
fn a_discrete_gpu_uploads_through_its_bar_and_keeps_images_in_vram() {
	// RADV with resizable BAR: VRAM, GTT, BAR, cached GTT, and AMD's
	// device-coherent uncached variants, which are slow and never used.
	let radv = types(&[0x1, 0x6, 0x7, 0xe, 0xc1, 0xc6, 0xc7]);
	assert_eq!(chosen(&radv, ALL), [Some(2), Some(0), Some(3)]);
	// NVIDIA: a type with no flags first, then VRAM, host, cached host, BAR.
	let nvidia = types(&[0x0, 0x1, 0x6, 0xe, 0x7]);
	assert_eq!(chosen(&nvidia, ALL), [Some(4), Some(1), Some(3)]);
	// Without a BAR, uploads go through host memory.
	let old = types(&[0x1, 0x6, 0xe]);
	assert_eq!(chosen(&old, ALL), [Some(1), Some(0), Some(2)]);
}

#[test]
fn only_the_types_a_resource_allows_are_chosen() {
	let intel = types(&[0x1, 0x7, 0xf, 0x21, 0x1]);
	assert_eq!(chosen(&intel, 0), [None; 3]);
	// Without type 1, uploads take the cached type and images type 4.
	assert_eq!(chosen(&intel, !0b11), [Some(2), Some(4), Some(2)]);
	// Images take host-visible memory when nothing else is allowed.
	assert_eq!(chosen(&intel, 0b110), [Some(1), Some(1), Some(2)]);
	// Nothing the CPU can see: no uploads or readback.
	assert_eq!(chosen(&intel, 0b10001), [None, Some(0), None]);
	// Bits for types the device does not have are ignored.
	assert_eq!(chosen(&types(&[0x1]), ALL), [None, Some(0), None]);
}

#[test]
fn readback_needs_coherent_memory_the_cpu_sees_and_prefers_it_cached() {
	// Host-visible; + coherent; + cached; + both.
	let host = types(&[0x2, 0x6, 0xa, 0xe]);
	let order: Vec<_> = (0..4)
		.map(|_| ())
		.scan(ALL, |allowed, ()| {
			let index = choose_memory(&host, *allowed, Usage::Readback)?;
			*allowed &= !(1 << index);
			Some(index)
		})
		.collect();
	// Reading needs no invalidate call, so cached memory that is not
	// coherent is never used.
	assert_eq!(order, [3, 1]);
	// Uploads need coherent memory too, so writes need no flush.
	assert_eq!(choose_memory(&host, 0b101, Usage::Upload), None);
	assert_eq!(choose_memory(&host, 0b111, Usage::Upload), Some(1));
}

#[test]
fn protected_lazy_and_amd_device_coherent_memory_is_never_chosen() {
	// Each is otherwise everything a usage wants.
	for avoided in [0x10, 0x20, 0x40, 0x80] {
		let memory = types(&[0xf | avoided, 0x7 | avoided]);
		assert_eq!(chosen(&memory, ALL), [None; 3], "{avoided:#x}");
	}
	// A type with flags no version knows yet is still fine.
	let memory = types(&[0xf | 0x200]);
	assert_eq!(chosen(&memory, ALL), [Some(0); 3]);
}

#[test]
fn ties_go_to_the_first_type_and_types_past_32_are_ignored() {
	let memory = types(&[0x7, 0x7, 0x7]);
	assert_eq!(chosen(&memory, ALL), [Some(0); 3]);
	assert_eq!(chosen(&memory, 0b100), [Some(2); 3]);
	// Vulkan has at most 32; a 33rd cannot be named in memoryTypeBits.
	let mut many = types(&[0x0; 32]);
	many.push(MemoryType {
		flags: 0xf,
		heap: 0,
	});
	assert_eq!(chosen(&many, ALL), [None, Some(0), None]);
}

/// What `usage` prefers, best first: a type is chosen from the first tier
/// any allowed type is in.
fn tiers(usage: Usage) -> Vec<fn(u32) -> bool> {
	match usage {
		Usage::Upload => vec![|f| f & 7 == 7, |f| {
			f & (HOST_VISIBLE | HOST_COHERENT) == HOST_VISIBLE | HOST_COHERENT
		}],
		Usage::Device => vec![
			|f| f & DEVICE_LOCAL != 0 && f & HOST_VISIBLE == 0,
			|f| f & DEVICE_LOCAL != 0,
			|f| f & HOST_VISIBLE == 0,
			|_| true,
		],
		Usage::Readback => vec![
			|f| f & (HOST_VISIBLE | HOST_CACHED | HOST_COHERENT) == 14,
			|f| f & (HOST_VISIBLE | HOST_COHERENT) == HOST_VISIBLE | HOST_COHERENT,
		],
	}
}

fn expected(types: &[MemoryType], allowed: u32, usage: Usage) -> Option<u32> {
	let usable = |index: usize| {
		// Lazily allocated, protected, AMD device-coherent and uncached.
		index < 32 && allowed & (1 << index) != 0 && types[index].flags & 0xf0 == 0
	};
	for tier in tiers(usage) {
		if let Some(index) = (0..types.len()).find(|&i| usable(i) && tier(types[i].flags)) {
			return Some(index as u32);
		}
	}
	None
}

#[test]
fn random_memory_is_chosen_from_as_the_tiers_of_preference_say() {
	let mut rng = crate::bench::corpus::Rng::new(29);
	let bits = [
		DEVICE_LOCAL,
		HOST_VISIBLE,
		HOST_COHERENT,
		HOST_CACHED,
		0x10,
		0x20,
		0x40,
		0x80,
		0x100,
	];
	let mut found = [0; 3];
	for _ in 0..5000 {
		let memory: Vec<_> = (0..1 + rng.below(32))
			.map(|_| {
				// Mostly the four common flags, sometimes a rarer one.
				let mut flags = rng.below(16) as u32;
				if rng.below(8) == 0 {
					flags |= *rng.pick(&bits);
				}
				MemoryType {
					flags,
					heap: rng.below(3) as u32,
				}
			})
			.collect();
		let allowed = match rng.below(3) {
			0 => ALL,
			1 => rng.next() as u32,
			_ => 1 << rng.below(32),
		};
		for (usage, found) in USAGES.into_iter().zip(&mut found) {
			let choice = choose_memory(&memory, allowed, usage);
			assert_eq!(
				choice,
				expected(&memory, allowed, usage),
				"{usage:?} {allowed:#x} {memory:?}"
			);
			*found += usize::from(choice.is_some());
		}
	}
	// Every usage finds memory often, and misses sometimes.
	assert!(
		found.iter().all(|&found| (1000..5000).contains(&found)),
		"{found:?}"
	);
}
