//! Pass the release codename from Cargo metadata into the application.
use std::fs;

#[path = "build/codename.rs"]
mod codename;

fn main() {
	if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
		println!("cargo:rustc-link-lib=framework=Foundation");
		println!("cargo:rustc-link-lib=framework=PhotosUI");
	}
	println!("cargo:rerun-if-changed=Cargo.toml");
	println!("cargo:rerun-if-changed=build/codename.rs");
	let manifest = fs::read_to_string("Cargo.toml").expect("read Cargo.toml");
	let codename =
		codename::package_codename(&manifest).expect("package metadata must define a codename");
	println!("cargo:rustc-env=POOPTERM_CODENAME={codename}");
}
