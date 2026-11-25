//! The default package source: one release, an index per platform.
use super::source::{REPOSITORY, default, index_for, platform, platform_of};
use crate::pkg::index::{allowed, archive_url};

#[test]
fn each_platform_has_a_name_of_its_own() {
	assert_eq!(platform_of("ios", "aarch64", false), Some("ios"));
	assert_eq!(platform_of("ios", "aarch64", true), Some("ios-simulator"));
	assert_eq!(platform_of("macos", "aarch64", false), Some("macos"));
	assert_eq!(
		platform_of("android", "aarch64", false),
		Some("android-aarch64")
	);
	assert_eq!(
		platform_of("android", "x86_64", false),
		Some("android-x86_64")
	);
}

#[test]
fn platforms_no_packages_are_built_for_have_none() {
	assert_eq!(platform_of("linux", "x86_64", false), None);
	assert_eq!(platform_of("macos", "x86_64", false), None, "Intel Macs");
	assert_eq!(platform_of("android", "arm", false), None, "32-bit phones");
	assert_eq!(platform_of("freebsd", "aarch64", false), None);
}

#[test]
fn the_index_is_in_the_packages_release_of_poopterms_repository() {
	assert_eq!(
		index_for("ios"),
		"https://github.com/poopware-laboratories/poopterm/releases/download/packages/ios-index"
	);
	assert!(allowed(&index_for("android-aarch64")), "over https");
	assert!(REPOSITORY.ends_with('/'));
}

#[test]
fn archives_are_found_beside_the_index_in_the_same_release() {
	// build_release.sh names each archive after its platform, as the index.
	assert_eq!(
		archive_url(&index_for("ios"), "ios-jq-1.7.1.tar.gz"),
		format!("{REPOSITORY}ios-jq-1.7.1.tar.gz")
	);
}

#[test]
fn this_build_uses_its_own_platforms_index() {
	assert_eq!(default(), platform().map(index_for));
	#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
	assert_eq!(default().as_deref(), Some(index_for("macos").as_str()));
}
