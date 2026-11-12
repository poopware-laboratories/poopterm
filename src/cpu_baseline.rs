//! The instruction set this build requires, checked before anything else
//! runs. x86-64 builds target x86-64-v2 (`.cargo/config.toml`,
//! `scripts/build.sh`); on an older CPU the program says so and exits
//! instead of dying of an illegal instruction somewhere later.
//!
//! The CPU is asked with CPUID itself: `is_x86_feature_detected!` answers
//! `true` without asking for any feature the build already enables, so it
//! cannot tell a v2 build that the CPU is older.
// Places the check in `.init_array`; docs/unsafe.md lists it.
#![allow(unsafe_code)]
#![cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]

/// A feature the build may require: its name, whether this build was
/// compiled to use it, and whether the CPU has it.
pub(crate) type Feature = (&'static str, bool, bool);

/// The x86-64-v2 features, as compiled and as the bits of ECX that CPUID
/// leaf 1 returns report them. LAHF/SAHF complete the level, but the
/// compiler never emits them.
pub(crate) fn features(leaf_1_ecx: u32) -> [Feature; 6] {
	let has = |bit: u32| leaf_1_ecx & (1 << bit) != 0;
	[
		("SSE3", cfg!(target_feature = "sse3"), has(0)),
		("SSSE3", cfg!(target_feature = "ssse3"), has(9)),
		("CMPXCHG16B", cfg!(target_feature = "cmpxchg16b"), has(13)),
		("SSE4.1", cfg!(target_feature = "sse4.1"), has(19)),
		("SSE4.2", cfg!(target_feature = "sse4.2"), has(20)),
		("POPCNT", cfg!(target_feature = "popcnt"), has(23)),
	]
}

/// The features the build uses that the CPU lacks.
pub(crate) fn missing(features: &[Feature]) -> impl Iterator<Item = &'static str> + '_ {
	features
		.iter()
		.filter(|&&(_, compiled, present)| compiled && !present)
		.map(|&(name, ..)| name)
}

/// Tells the user which features the CPU lacks; false when it lacks none.
/// Allocates nothing: the allocator's C code is compiled for the same
/// features, so on an older CPU the first allocation would crash first.
pub(crate) fn refuse(features: &[Feature], out: &mut impl std::io::Write) -> bool {
	let mut lacking = missing(features).peekable();
	if lacking.peek().is_none() {
		return false;
	}
	let mut write = |text: &str| {
		// Nothing more can be done if standard error is gone.
		let _ = out.write_all(text.as_bytes());
	};
	write("poopterm: this build needs an x86-64-v2 CPU (from about 2009 on), and this one lacks ");
	for (index, name) in lacking.enumerate() {
		write(if index == 0 { "" } else { ", " });
		write(name);
	}
	write(".\nBuild it for older CPUs with RUSTFLAGS=-Ctarget-cpu=x86-64 cargo build --release.\n");
	true
}

/// ECX of CPUID leaf 1, where this CPU reports the x86-64-v2 features.
#[cfg(target_arch = "x86_64")]
pub(crate) fn leaf_1_ecx() -> u32 {
	std::arch::x86_64::__cpuid(1).ecx
}

/// Exits with an explanation when the CPU cannot run this build.
pub(crate) fn check() {
	#[cfg(target_arch = "x86_64")]
	if refuse(&features(leaf_1_ecx()), &mut std::io::stderr()) {
		std::process::exit(1);
	}
}

/// Calls [`check`] as the program loads, before every other constructor:
/// mimalloc's (priority 101), [`crate::allocator`]'s (100), and the ones of
/// dependencies at the default priority, such as tiny-xlib's, which
/// allocates. All of that code is compiled for x86-64-v2 too, so a check in
/// `main` would come after an older CPU had already crashed. Not in tests.
#[cfg(all(
	target_arch = "x86_64",
	any(target_os = "linux", target_os = "freebsd"),
	not(test)
))]
#[used]
// SAFETY: a plain function pointer in `.init_array`, which the loader
// calls once, single-threaded, before `main`; the linker orders sections
// of lower priority numbers first.
#[unsafe(link_section = ".init_array.00099")]
static CHECK_FIRST: extern "C" fn() = {
	extern "C" fn first() {
		check();
	}
	first
};

#[cfg(test)]
#[path = "cpu_baseline_tests.rs"]
mod tests;
