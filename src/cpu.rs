//! Runtime CPU dispatch: the one place that calls code compiled for CPU
//! features checked at run time. Hot paths are compiled twice, for baseline
//! x86-64 and for x86-64-v3 (AVX2, BMI1/2, LZCNT, POPCNT, FMA); callers use
//! [`dispatch!`] or [`run!`] and never write `unsafe`.
#![allow(unsafe_code)]
#![cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]

/// Whether the CPU runs the x86-64-v3 variants, detected once: asking `std`
/// for each of the six features per check cost text floods with frequent
/// fills 1.5% more instructions.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn has_v3() -> bool {
	static V3_CPU: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*V3_CPU.get_or_init(detect)
}

/// Asks the standard library for every x86-64-v3 feature.
#[cfg(target_arch = "x86_64")]
#[cold]
fn detect() -> bool {
	use std::arch::is_x86_feature_detected as has;
	has!("avx2") && has!("bmi1") && has!("bmi2") && has!("lzcnt") && has!("fma") && has!("popcnt")
}

/// Proof that the CPU runs x86-64-v3 code: only [`V3::detect`] makes one,
/// and only after checking the CPU, so running code compiled for those
/// features through it is sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct V3 {
	_checked: (),
}

impl V3 {
	/// The proof, when the CPU has every x86-64-v3 feature.
	#[inline(always)]
	pub(crate) fn detect() -> Option<Self> {
		#[cfg(target_arch = "x86_64")]
		if has_v3() {
			return Some(Self { _checked: () });
		}
		None
	}

	/// Runs `body` compiled for x86-64-v3. Call it through [`run!`], which
	/// makes `body` a closure that is always inlined, and let the body call
	/// only `#[inline(always)]` code, so all of it lands in this copy.
	#[inline(always)]
	pub(crate) fn run<R>(self, body: impl FnOnce() -> R) -> R {
		#[cfg(target_arch = "x86_64")]
		{
			// SAFETY: a `V3` exists only where `detect` found the features.
			unsafe { run_v3(body) }
		}
		#[cfg(not(target_arch = "x86_64"))]
		body()
	}

	/// Whether `bytes` is valid UTF-8, by the AVX2 kernel, which uses
	/// `std::arch` intrinsics: `std::simd` lowers its byte lookups to
	/// `vpshufb` only when the standard library itself is built for SSSE3.
	#[cfg(target_arch = "x86_64")]
	#[inline]
	pub(crate) fn utf8_is_valid(self, bytes: &[u8]) -> bool {
		// SAFETY: a `V3` exists only where `detect` found AVX2.
		unsafe { crate::vt::utf8::x86::is_valid(bytes) }
	}
}

/// `body` inlined into a function compiled for x86-64-v3.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,bmi1,bmi2,lzcnt,popcnt,fma")]
#[inline]
fn run_v3<R>(body: impl FnOnce() -> R) -> R {
	body()
}

/// `run!(v3, expression)`: evaluates the expression compiled for
/// x86-64-v3, given a [`V3`] proof. The closure must be `inline(always)`:
/// otherwise a closure the compiler wants to share is compiled once, for
/// the baseline, and the v3 copy only jumps to it.
macro_rules! run {
	($v3:expr, $body:expr) => {
		$crate::cpu::V3::run(
			$v3,
			#[inline(always)]
			|| $body,
		)
	};
}
pub(crate) use run;

/// Runs `body` as compiled for the baseline, for [`dispatch!`].
#[inline(always)]
pub(crate) fn baseline<R>(body: impl FnOnce() -> R) -> R {
	body()
}

/// `dispatch!(expression)`: evaluates the expression compiled for the best
/// level the CPU supports. The expression is compiled once per level, as a
/// closure on both, so `return` and `?` in it mean the same on each.
macro_rules! dispatch {
	($body:expr) => {
		match $crate::cpu::V3::detect() {
			Some(v3) => $crate::cpu::run!(v3, $body),
			None => $crate::cpu::baseline(
				#[inline(always)]
				|| $body,
			),
		}
	};
}
pub(crate) use dispatch;

#[cfg(test)]
#[path = "cpu_tests.rs"]
mod tests;
