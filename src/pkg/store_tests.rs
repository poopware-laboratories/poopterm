//! Installing and removing packages in a directory of their own: archives
//! made by the system's `tar` and `gzip`, the checks before anything is
//! written, and the commands each package puts on `$PATH`.
use std::{
	path::{Path, PathBuf},
	sync::atomic::{AtomicUsize, Ordering},
};

use super::{index::Package, sha256::sha256, store::Store};

/// A fresh, empty directory for one test.
fn scratch(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("poopterm-pkg-{name}-{}", std::process::id()));
	let _ = std::fs::remove_dir_all(&dir);
	std::fs::create_dir_all(&dir).unwrap();
	dir
}

/// A `tar.gz` of `files` (path, contents), as a package is published.
pub(super) fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
	static MADE: AtomicUsize = AtomicUsize::new(0);
	let dir = scratch(&format!("src-{}", MADE.fetch_add(1, Ordering::Relaxed)));
	for (path, data) in files {
		let path = dir.join(path);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(path, data).unwrap();
	}
	let out = std::process::Command::new("tar")
		.env("COPYFILE_DISABLE", "1")
		.args(["-czf", "-", "-C"])
		.arg(&dir)
		.arg(".")
		.output()
		.unwrap();
	std::fs::remove_dir_all(&dir).unwrap();
	assert!(out.status.success());
	out.stdout
}

/// The index entry of `archive` as package `name` at `version`.
pub(super) fn listed(name: &str, version: &str, archive: &[u8]) -> Package {
	Package {
		name: name.into(),
		version: version.into(),
		sha256: sha256(archive),
		size: archive.len() as u64,
		archive: format!("{name}-{version}.tar.gz"),
		summary: String::new(),
	}
}

fn read(path: &Path) -> Vec<u8> {
	std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn an_install_unpacks_the_package_and_puts_its_commands_on_the_path() {
	let root = scratch("install");
	let bytes = archive(&[("bin/jot", b"jot library"), ("share/jot.txt", b"manual")]);
	let store = Store::new(root.clone());
	let commands = store
		.install(&listed("jot", "1.0", &bytes), &bytes)
		.unwrap();
	assert_eq!(commands, ["jot"]);
	assert_eq!(read(&root.join("jot/bin/jot")), b"jot library");
	assert_eq!(read(&root.join("jot/share/jot.txt")), b"manual");
	assert_eq!(
		read(&root.join("bin/jot")),
		b"jot library",
		"through the link"
	);
	assert_eq!(store.installed(), [("jot".to_owned(), "1.0".to_owned())]);
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_archive_that_does_not_match_its_index_entry_writes_nothing() {
	let root = scratch("mismatch");
	let bytes = archive(&[("bin/jot", b"jot")]);
	let store = Store::new(root.clone());
	let mut wrong_hash = listed("jot", "1", &bytes);
	wrong_hash.sha256[0] ^= 1;
	assert!(
		store
			.install(&wrong_hash, &bytes)
			.unwrap_err()
			.contains("SHA-256")
	);
	let mut wrong_size = listed("jot", "1", &bytes);
	wrong_size.size += 1;
	assert!(
		store
			.install(&wrong_size, &bytes)
			.unwrap_err()
			.contains("size")
	);
	assert!(!root.join("jot").exists() && !root.join("bin/jot").exists());
	assert_eq!(store.installed(), []);
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_archive_that_is_not_a_tar_gz_is_refused() {
	let root = scratch("garbage");
	let store = Store::new(root.clone());
	let bytes = b"not gzip at all".to_vec();
	let error = store
		.install(&listed("x", "1", &bytes), &bytes)
		.unwrap_err();
	assert!(error.contains("gzip"), "{error}");
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_package_without_commands_or_with_nested_ones_is_refused() {
	let root = scratch("nocommands");
	let store = Store::new(root.clone());
	for files in [
		&[("share/readme", &b"just docs"[..])][..],
		&[("bin/sub/tool", &b"nested"[..])][..],
	] {
		let bytes = archive(files);
		let error = store
			.install(&listed("docs", "1", &bytes), &bytes)
			.unwrap_err();
		assert!(error.contains("bin"), "{error}");
	}
	assert!(!root.join("docs").exists());
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_command_another_package_owns_is_refused_and_that_package_kept() {
	let root = scratch("conflict");
	let store = Store::new(root.clone());
	let first = archive(&[("bin/tool", b"first")]);
	store.install(&listed("one", "1", &first), &first).unwrap();
	let second = archive(&[("bin/tool", b"second"), ("bin/other", b"other")]);
	let error = store
		.install(&listed("two", "1", &second), &second)
		.unwrap_err();
	assert!(error.contains("tool") && error.contains("one"), "{error}");
	assert_eq!(read(&root.join("bin/tool")), b"first");
	assert!(!root.join("bin/other").exists() && !root.join("two").exists());
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_upgrade_replaces_the_files_and_drops_commands_it_no_longer_has() {
	let root = scratch("upgrade");
	let store = Store::new(root.clone());
	let old = archive(&[("bin/a", b"a1"), ("bin/b", b"b1"), ("share/old", b"o")]);
	store.install(&listed("pair", "1", &old), &old).unwrap();
	let new = archive(&[("bin/a", b"a2")]);
	assert_eq!(
		store.install(&listed("pair", "2", &new), &new).unwrap(),
		["a"]
	);
	assert_eq!(read(&root.join("bin/a")), b"a2");
	assert!(!root.join("bin/b").exists() && !root.join("bin/b").is_symlink());
	assert!(!root.join("pair/share/old").exists());
	assert_eq!(store.installed(), [("pair".to_owned(), "2".to_owned())]);
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn removing_a_package_takes_its_files_links_and_record() {
	let root = scratch("remove");
	let store = Store::new(root.clone());
	let keep = archive(&[("bin/keep", b"k")]);
	store.install(&listed("keep", "1", &keep), &keep).unwrap();
	let gone = archive(&[("bin/gone", b"g"), ("bin/also", b"a")]);
	store.install(&listed("gone", "3", &gone), &gone).unwrap();
	let mut removed = store.remove("gone").unwrap();
	removed.sort();
	assert_eq!(removed, ["also", "gone"]);
	assert!(!root.join("gone").exists() && !root.join("bin/gone").is_symlink());
	assert_eq!(read(&root.join("bin/keep")), b"k");
	assert_eq!(store.installed(), [("keep".to_owned(), "1".to_owned())]);
	assert!(store.remove("gone").unwrap_err().contains("not installed"));
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn installed_packages_are_listed_by_name() {
	let root = scratch("listed");
	let store = Store::new(root.clone());
	assert_eq!(store.installed(), []);
	for name in ["zed", "abc", "mid"] {
		let bytes = archive(&[(&format!("bin/{name}") as &str, b"x")]);
		store.install(&listed(name, "1", &bytes), &bytes).unwrap();
	}
	let names: Vec<_> = store
		.installed()
		.into_iter()
		.map(|(name, _)| name)
		.collect();
	assert_eq!(names, ["abc", "mid", "zed"]);
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn leftovers_of_an_interrupted_install_do_not_stop_the_next() {
	let root = scratch("leftover");
	std::fs::create_dir_all(root.join(".jot.new/bin")).unwrap();
	std::fs::write(root.join(".jot.new/bin/stale"), b"stale").unwrap();
	let store = Store::new(root.clone());
	let bytes = archive(&[("bin/jot", b"fresh")]);
	store.install(&listed("jot", "1", &bytes), &bytes).unwrap();
	assert!(!root.join("jot/bin/stale").exists());
	assert!(!root.join(".jot.new").exists());
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_package_name_that_could_leave_the_directory_is_refused() {
	let root = scratch("badname");
	let store = Store::new(root.clone());
	let bytes = archive(&[("bin/x", b"x")]);
	assert!(store.install(&listed("../x", "1", &bytes), &bytes).is_err());
	assert!(store.remove("../x").is_err());
	std::fs::remove_dir_all(root).unwrap();
}
