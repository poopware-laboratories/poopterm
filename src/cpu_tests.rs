//! The dispatch point: a `V3` proof exists exactly where the CPU has every
//! x86-64-v3 feature, and each copy runs its body once and returns its value.
use super::V3;

/// What `std` reports for each feature the v3 copies are compiled with.
#[cfg(target_arch = "x86_64")]
fn std_says_v3() -> bool {
	use std::arch::is_x86_feature_detected as has;
	[
		has!("avx2"),
		has!("bmi1"),
		has!("bmi2"),
		has!("lzcnt"),
		has!("fma"),
		has!("popcnt"),
	]
	.iter()
	.all(|&present| present)
}

#[cfg(target_arch = "x86_64")]
#[test]
fn the_proof_exists_exactly_where_std_detects_every_feature() {
	assert_eq!(V3::detect().is_some(), std_says_v3());
}

#[cfg(target_arch = "x86_64")]
#[test]
fn the_cached_answer_matches_a_fresh_detection() {
	let cached = super::has_v3();
	assert_eq!(cached, super::detect());
	assert_eq!(super::has_v3(), cached, "the cache must not change");
}

#[cfg(not(target_arch = "x86_64"))]
#[test]
fn other_architectures_never_get_the_proof() {
	assert_eq!(V3::detect(), None);
}

#[test]
fn detection_gives_the_same_answer_from_every_thread() {
	let first = V3::detect();
	let others: Vec<_> = (0..8)
		.map(|_| std::thread::spawn(V3::detect))
		.map(|thread| thread.join().unwrap())
		.collect();
	assert!(others.iter().all(|&other| other == first), "{others:?}");
}

#[test]
fn dispatch_runs_the_body_once_and_returns_its_value() {
	let mut calls = 0;
	let value = crate::cpu::dispatch!({
		calls += 1;
		calls * 7
	});
	assert_eq!((calls, value), (1, 7));
}

#[test]
fn the_v3_copy_runs_the_body_once_and_returns_its_value() {
	let Some(cpu) = V3::detect() else {
		return;
	};
	let mut calls = 0;
	let value = crate::cpu::run!(cpu, {
		calls += 1;
		format!("ran {calls}")
	});
	assert_eq!((calls, value.as_str()), (1, "ran 1"));
}

#[test]
fn bodies_may_borrow_mutably_and_move_values_out() {
	let mut cells = vec![0u64; 37];
	let owned = String::from("moved");
	let back = crate::cpu::dispatch!({
		cells
			.iter_mut()
			.enumerate()
			.for_each(|(i, cell)| *cell = i as u64 * 3);
		owned
	});
	assert_eq!(back, "moved");
	assert_eq!(cells.iter().sum::<u64>(), 3 * (0..37).sum::<u64>());
}

/// `return` and `?` inside a body leave the body, on either copy, never the
/// function that dispatched it.
#[test]
fn return_and_question_mark_leave_only_the_body_on_both_copies() {
	fn early(input: Option<u8>) -> (Option<u8>, &'static str) {
		let from_body = crate::cpu::dispatch!({
			let value = input?;
			if value == 0 {
				return Some(100);
			}
			Some(value + 1)
		});
		(from_body, "the caller went on")
	}
	assert_eq!(early(None), (None, "the caller went on"));
	assert_eq!(early(Some(0)), (Some(100), "the caller went on"));
	assert_eq!(early(Some(4)), (Some(5), "the caller went on"));
	let Some(cpu) = V3::detect() else {
		return;
	};
	let v3 = |input: Option<u8>| -> Option<u8> {
		crate::cpu::run!(cpu, {
			let value = input?;
			if value == 0 {
				return Some(100);
			}
			Some(value + 1)
		})
	};
	assert_eq!(
		[v3(None), v3(Some(0)), v3(Some(4))],
		[None, Some(100), Some(5)]
	);
}

#[test]
fn both_copies_compute_the_same_vector_results() {
	// Long enough for the v3 copy to use 256-bit vectors, with a remainder.
	let input: Vec<u32> = (0..1_003u32)
		.map(|i| i.wrapping_mul(2_654_435_761))
		.collect();
	let work = || -> (u32, u32, u64) {
		let max = input.iter().copied().max().unwrap();
		let ones = input.iter().map(|x| x.count_ones()).sum();
		let mixed = input
			.iter()
			.map(|&x| u64::from(x.leading_zeros() ^ x.rotate_left(5)))
			.sum();
		(max, ones, mixed)
	};
	let baseline = work();
	assert_eq!(crate::cpu::dispatch!(work()), baseline);
	if let Some(cpu) = V3::detect() {
		assert_eq!(crate::cpu::run!(cpu, work()), baseline);
	}
}

/// Runs of every length from 0 to 100 around the 32-byte blocks, with a
/// character placed across each block boundary, cut short, or broken.
#[cfg(target_arch = "x86_64")]
#[test]
fn the_utf8_kernel_agrees_with_std_across_block_boundaries() {
	let Some(cpu) = V3::detect() else {
		return;
	};
	let characters: [&[u8]; 6] = [
		"é".as_bytes(),
		"日".as_bytes(),
		"🎉".as_bytes(),
		b"\xe6\x97",
		b"\xed\xa0\x80",
		b"\xff",
	];
	for len in 0..=100usize {
		for character in characters {
			for at in [0, len.saturating_sub(1), len / 2, 31.min(len), 32.min(len)] {
				let mut text = vec![b'a'; len];
				text.splice(at..at, character.iter().copied());
				assert_eq!(
					cpu.utf8_is_valid(&text),
					std::str::from_utf8(&text).is_ok(),
					"{character:?} at {at} of {len}"
				);
			}
		}
	}
}

/// The symbols of this test binary that name a v3 copy, with their sizes in
/// bytes, from the ELF symbol table; `None` when it was stripped.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn v3_copies() -> Option<Vec<(String, u64)>> {
	let elf = std::fs::read("/proc/self/exe").unwrap();
	let u16_at = |at: usize| u16::from_le_bytes(elf[at..at + 2].try_into().unwrap());
	let u32_at = |at: usize| u32::from_le_bytes(elf[at..at + 4].try_into().unwrap());
	let u64_at = |at: usize| u64::from_le_bytes(elf[at..at + 8].try_into().unwrap());
	assert_eq!(&elf[..5], b"\x7fELF\x02", "a 64-bit ELF file");
	let sections = u64_at(0x28) as usize;
	let (size, count) = (u16_at(0x3a) as usize, u16_at(0x3c) as usize);
	let header = |index: usize| sections + index * size;
	// The symbol table (type 2) and the string table its link names.
	let symtab = (0..count).map(header).find(|&at| u32_at(at + 4) == 2)?;
	let strings = u64_at(header(u32_at(symtab + 0x28) as usize) + 0x18) as usize;
	let (start, len) = (
		u64_at(symtab + 0x18) as usize,
		u64_at(symtab + 0x20) as usize,
	);
	let copies = elf[start..start + len]
		.as_chunks::<24>()
		.0
		.iter()
		.filter_map(|symbol| {
			let name = &elf[strings + u32_at_slice(symbol) as usize..];
			let name = std::str::from_utf8(&name[..name.iter().position(|&byte| byte == 0)?]).ok()?;
			let size = u64::from_le_bytes(symbol[16..24].try_into().unwrap());
			name.contains("run_v3").then(|| (name.to_owned(), size))
		})
		.collect();
	Some(copies)
}

/// A symbol's name: the offset of its first byte in the string table.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn u32_at_slice(symbol: &[u8; 24]) -> u32 {
	u32::from_le_bytes(symbol[..4].try_into().unwrap())
}

/// Each hot path's v3 copy holds its body. Without `inline(always)` on the
/// closure, the compiler built the tokenizer's body once, for the baseline,
/// and its v3 copy was a five-byte jump to it.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn every_v3_copy_holds_its_body_rather_than_a_jump_to_the_baseline() {
	let Some(copies) = v3_copies() else {
		return; // A stripped build, such as the `fast` profile.
	};
	// Mangled names: a length, then the identifier.
	for path in [
		"6Parser7advance",
		"4Term10print_text",
		"7Painter5glyph",
		"4Cell4fill",
	] {
		let sizes: Vec<_> = copies
			.iter()
			.filter(|(name, _)| name.contains(path))
			.map(|&(_, size)| size)
			.collect();
		assert!(!sizes.is_empty(), "no v3 copy of {path} in {copies:?}");
		assert!(
			sizes.iter().all(|&size| size > 64),
			"{path}'s v3 copy is only {sizes:?} bytes: its body was not inlined"
		);
	}
}
