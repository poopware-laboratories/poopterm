//! Where packages come from unless the user says otherwise: the `packages`
//! release of Poopterm's own repository on GitHub, one index per platform
//! (`ios-index`, `android-aarch64-index`, ...) beside the archives it
//! lists, so the iOS and Android ports share one place to publish to.

/// The release every platform's packages are published to.
pub(crate) const REPOSITORY: &str =
	"https://github.com/poopware-laboratories/poopterm/releases/download/packages/";

/// The platform packages are built for, as their file names start: the
/// one this build runs on, if packages are made for it.
pub(crate) fn platform() -> Option<&'static str> {
	platform_of(
		std::env::consts::OS,
		std::env::consts::ARCH,
		cfg!(target_abi = "sim"),
	)
}

/// `platform` for an `os` and `arch` (Rust's names), `simulator` when the
/// build is for Apple's simulator.
pub(crate) fn platform_of(os: &str, arch: &str, simulator: bool) -> Option<&'static str> {
	match (os, arch, simulator) {
		("ios", "aarch64", false) => Some("ios"),
		("ios", "aarch64", true) => Some("ios-simulator"),
		("macos", "aarch64", _) => Some("macos"),
		("android", "aarch64", _) => Some("android-aarch64"),
		("android", "x86_64", _) => Some("android-x86_64"),
		_ => None,
	}
}

/// The index of `platform`'s packages in `REPOSITORY`.
pub(crate) fn index_for(platform: &str) -> String {
	format!("{REPOSITORY}{platform}-index")
}

/// The source this build uses when none is set.
pub(crate) fn default() -> Option<String> {
	platform().map(index_for)
}
