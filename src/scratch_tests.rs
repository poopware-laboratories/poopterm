//! Scratch directories for tests, removed when dropped so test runs leave
//! nothing behind in the temporary directory.
use std::{
	fs,
	ops::Deref,
	path::{Path, PathBuf},
};

/// A fresh, empty directory `poopterm-<kind>-<pid>-<name>` under the
/// temporary directory, removed with everything in it when dropped. The
/// process id keeps concurrent test runs apart.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
	pub(crate) fn new(kind: &str, name: &str) -> Self {
		let dir = std::env::temp_dir().join(format!("poopterm-{kind}-{}-{name}", std::process::id()));
		let _ = fs::remove_dir_all(&dir);
		fs::create_dir_all(&dir).unwrap();
		Self(dir)
	}
}

impl Deref for Scratch {
	type Target = Path;

	fn deref(&self) -> &Path {
		&self.0
	}
}

impl AsRef<Path> for Scratch {
	fn as_ref(&self) -> &Path {
		&self.0
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

#[test]
fn a_scratch_directory_starts_empty_and_is_removed_with_its_contents() {
	let path = {
		let dir = Scratch::new("scratch", "removed");
		assert!(dir.is_dir());
		assert_eq!(fs::read_dir(&*dir).unwrap().count(), 0);
		fs::create_dir(dir.join("nested")).unwrap();
		fs::write(dir.join("nested/file"), b"x").unwrap();
		dir.to_path_buf()
	};
	assert!(!path.exists(), "{path:?} left behind");
}

#[test]
fn leftovers_from_an_earlier_run_with_the_same_name_are_cleared() {
	let first = Scratch::new("scratch", "reused");
	fs::write(first.join("stale"), b"x").unwrap();
	// A crashed run drops nothing; its directory is found again.
	std::mem::forget(first);
	let second = Scratch::new("scratch", "reused");
	assert_eq!(fs::read_dir(&*second).unwrap().count(), 0);
}

#[test]
fn scratch_directories_of_different_names_are_apart() {
	let (a, b) = (Scratch::new("scratch", "a"), Scratch::new("scratch", "b"));
	assert_ne!(&*a, &*b);
	let name = a.file_name().unwrap().to_string_lossy().into_owned();
	assert!(
		name.starts_with("poopterm-scratch-") && name.ends_with("-a"),
		"{name}"
	);
	assert!(name.contains(&std::process::id().to_string()));
}
