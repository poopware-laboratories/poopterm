//! The startup check: which compiled features a CPU lacks, judged from the
//! CPUID bits of real CPU generations, what the user is told, and that
//! x86-64 builds really target x86-64-v2.
use super::{Feature, features, missing, refuse};

/// CPUID leaf 1 ECX bits: SSE3 0, SSSE3 9, CMPXCHG16B 13, SSE4.1 19,
/// SSE4.2 20, POPCNT 23.
const SSE3: u32 = 1 << 0;
const SSSE3: u32 = 1 << 9;
const CX16: u32 = 1 << 13;
const SSE4_1: u32 = 1 << 19;
const SSE4_2: u32 = 1 << 20;
const POPCNT: u32 = 1 << 23;
/// Bits outside the level (MONITOR, VMX, AES, AVX...), which must not count.
const UNRELATED: u32 = 1 << 3 | 1 << 5 | 1 << 25 | 1 << 28;

/// What a CPU of each generation lacks, were this a v2 build.
fn lacks_in_a_v2_build(ecx: u32) -> Vec<&'static str> {
	let as_v2 = features(ecx).map(|(name, _, present)| (name, true, present));
	missing(&as_v2).collect()
}

/// What `refuse` writes for `features`, and whether it refused.
fn refusal(features: &[Feature]) -> (bool, String) {
	let mut out = Vec::new();
	let refused = refuse(features, &mut out);
	(refused, String::from_utf8(out).unwrap())
}

#[test]
fn nehalem_and_later_have_the_whole_level() {
	let nehalem = SSE3 | SSSE3 | CX16 | SSE4_1 | SSE4_2 | POPCNT;
	assert_eq!(lacks_in_a_v2_build(nehalem), Vec::<&str>::new());
	assert_eq!(lacks_in_a_v2_build(nehalem | UNRELATED), Vec::<&str>::new());
}

#[test]
fn a_core_2_from_2006_lacks_sse4_and_popcnt() {
	let merom = SSE3 | SSSE3 | CX16;
	assert_eq!(lacks_in_a_v2_build(merom), ["SSE4.1", "SSE4.2", "POPCNT"]);
}

#[test]
fn a_penryn_core_2_lacks_only_sse4_2_and_popcnt() {
	let penryn = SSE3 | SSSE3 | CX16 | SSE4_1;
	assert_eq!(lacks_in_a_v2_build(penryn), ["SSE4.2", "POPCNT"]);
}

#[test]
fn the_first_x86_64_cpus_lack_everything_but_sse2() {
	assert_eq!(
		lacks_in_a_v2_build(0),
		["SSE3", "SSSE3", "CMPXCHG16B", "SSE4.1", "SSE4.2", "POPCNT"]
	);
	assert_eq!(
		lacks_in_a_v2_build(UNRELATED).len(),
		6,
		"unrelated bits are not features"
	);
}

#[test]
fn each_feature_reads_its_own_bit() {
	let bits = [SSE3, SSSE3, CX16, SSE4_1, SSE4_2, POPCNT];
	for (index, bit) in bits.iter().enumerate() {
		let present: Vec<bool> = features(*bit)
			.iter()
			.map(|&(.., present)| present)
			.collect();
		let expected: Vec<bool> = (0..bits.len()).map(|other| other == index).collect();
		assert_eq!(present, expected, "bit {bit:#x}");
	}
}

#[test]
fn features_the_build_does_not_use_may_be_absent() {
	// A baseline x86-64 build on an old CPU: nothing compiled in, nothing
	// required.
	let features = [("SSSE3", false, false), ("POPCNT", false, false)];
	assert_eq!(missing(&features).count(), 0);
	assert_eq!(refusal(&features), (false, String::new()));
}

#[test]
fn every_compiled_feature_the_cpu_lacks_is_named_in_order() {
	let features = [
		("SSE3", true, true),
		("SSSE3", true, false),
		("SSE4.1", false, false),
		("SSE4.2", true, false),
		("POPCNT", true, true),
	];
	assert_eq!(missing(&features).collect::<Vec<_>>(), ["SSSE3", "SSE4.2"]);
}

#[test]
fn the_refusal_names_the_level_the_missing_features_and_the_way_out() {
	let (refused, message) = refusal(&[
		("SSE4.1", true, true),
		("SSE4.2", true, false),
		("POPCNT", true, false),
	]);
	assert!(refused);
	assert!(
		message.starts_with("poopterm: this build needs an x86-64-v2 CPU"),
		"{message}"
	);
	assert!(message.contains("lacks SSE4.2, POPCNT."), "{message}");
	assert!(
		message.contains("RUSTFLAGS=-Ctarget-cpu=x86-64 cargo build --release"),
		"{message}"
	);
	assert_eq!(message.lines().count(), 2, "{message}");
	assert!(message.ends_with(".\n"), "{message}");
}

/// Tests build through `.cargo/config.toml`, like CI and the packages: a
/// lost or overridden setting shows up here.
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_64_builds_target_x86_64_v2() {
	let compiled = features(0).map(|(name, compiled, _)| (name, compiled));
	assert!(
		compiled.iter().all(|&(_, on)| on),
		"not compiled for v2: {compiled:?}"
	);
}

/// The machine running the tests runs this build, so the check passes here,
/// and CPUID agrees with what `std` detects at run time.
#[cfg(target_arch = "x86_64")]
#[test]
fn this_cpu_has_everything_the_build_uses() {
	let ecx = super::leaf_1_ecx();
	assert_eq!(missing(&features(ecx)).count(), 0);
	assert_eq!(refusal(&features(ecx)), (false, String::new()));
	assert_eq!(
		ecx & SSE4_2 != 0,
		std::arch::is_x86_feature_detected!("sse4.2")
	);
}

#[test]
fn a_single_missing_feature_has_no_separator() {
	let (refused, message) = refusal(&[("POPCNT", true, false)]);
	assert!(refused);
	assert!(
		message.contains("and this one lacks POPCNT.\n"),
		"{message}"
	);
}

#[test]
fn a_core_2_is_told_exactly_what_it_lacks() {
	let merom = SSE3 | SSSE3 | CX16;
	let as_v2 = features(merom).map(|(name, _, present)| (name, true, present));
	let (refused, message) = refusal(&as_v2);
	assert!(refused);
	assert_eq!(
		message.lines().next().unwrap(),
		"poopterm: this build needs an x86-64-v2 CPU (from about 2009 on), \
		 and this one lacks SSE4.1, SSE4.2, POPCNT."
	);
}

/// A writer that fails, like a closed standard error: the check still
/// decides, and does not panic.
#[test]
fn a_closed_standard_error_does_not_stop_the_refusal() {
	struct Closed;
	impl std::io::Write for Closed {
		fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
			Err(std::io::ErrorKind::BrokenPipe.into())
		}
		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}
	assert!(refuse(&[("SSE3", true, false)], &mut Closed));
	assert!(!refuse(&[("SSE3", true, true)], &mut Closed));
}
