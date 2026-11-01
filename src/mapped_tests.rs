use std::{
	fs,
	io::ErrorKind,
	path::{Path, PathBuf},
	thread,
};

use super::*;

fn scratch(name: &str) -> PathBuf {
	let dir = std::env::temp_dir().join(format!("poopterm-mapped-{}-{name}", std::process::id()));
	let _ = fs::remove_dir_all(&dir);
	fs::create_dir_all(&dir).unwrap();
	dir
}

/// Bytes that differ at every position of a page, so an offset slip shows.
fn pattern(len: usize) -> Vec<u8> {
	(0..len).map(|i| (i * 7 + i / 251) as u8).collect()
}

fn page() -> usize {
	// SAFETY: sysconf only reads a system constant.
	unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
}

#[test]
fn a_mapping_holds_the_files_bytes_at_every_size_around_a_page() {
	let dir = scratch("sizes");
	for len in [1, 2, page() - 1, page(), page() + 1, 3 * page() + 17] {
		let path = dir.join(format!("{len}.bin"));
		fs::write(&path, pattern(len)).unwrap();
		let map = Mapped::open(&path).unwrap();
		assert_eq!(map.len(), len, "{len} bytes");
		assert_eq!(&*map, &pattern(len)[..], "{len} bytes");
	}
}

#[test]
fn an_empty_file_is_refused_as_mmap_cannot_map_nothing() {
	let dir = scratch("empty");
	let path = dir.join("empty.bin");
	fs::write(&path, b"").unwrap();
	let error = Mapped::open(&path).err().expect("an empty file is refused");
	assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[test]
fn a_missing_file_reports_not_found() {
	let dir = scratch("missing");
	let error = Mapped::open(&dir.join("nothing.ttf"))
		.err()
		.expect("missing");
	assert_eq!(error.kind(), ErrorKind::NotFound);
}

#[test]
fn a_directory_is_refused() {
	let dir = scratch("directory");
	let error = Mapped::open(&dir).err().expect("a directory is refused");
	assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[cfg(unix)]
#[test]
fn a_fifo_is_refused_without_blocking_on_it() {
	let dir = scratch("fifo");
	let path = dir.join("pipe");
	let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
	// SAFETY: `name` is a C string naming a path in our scratch directory.
	assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
	// Opening a FIFO for reading would wait for a writer; the test would hang.
	let error = Mapped::open(&path).err().expect("a FIFO is refused");
	assert_eq!(error.kind(), ErrorKind::InvalidInput);
}

#[test]
fn the_bytes_survive_the_file_being_deleted() {
	let dir = scratch("deleted");
	let path = dir.join("font.ttf");
	fs::write(&path, pattern(5000)).unwrap();
	let map = Mapped::open(&path).unwrap();
	fs::remove_file(&path).unwrap();
	assert_eq!(&*map, &pattern(5000)[..]);
}

#[test]
fn the_bytes_survive_the_file_being_replaced_by_a_rename() {
	// How package managers update fonts: write a new file, rename it over.
	let dir = scratch("replaced");
	let path = dir.join("font.ttf");
	fs::write(&path, pattern(5000)).unwrap();
	let map = Mapped::open(&path).unwrap();
	let new = dir.join("font.ttf.new");
	fs::write(&new, vec![0xaa; 9000]).unwrap();
	fs::rename(&new, &path).unwrap();
	assert_eq!(&*map, &pattern(5000)[..]);
	assert_eq!(Mapped::open(&path).unwrap().len(), 9000);
}

#[test]
fn a_mapping_can_be_read_from_another_thread() {
	let dir = scratch("thread");
	let path = dir.join("font.ttf");
	fs::write(&path, pattern(3 * page())).unwrap();
	let map = Mapped::open(&path).unwrap();
	let sum = thread::spawn(move || map.iter().map(|&b| u64::from(b)).sum::<u64>())
		.join()
		.unwrap();
	assert_eq!(
		sum,
		pattern(3 * page())
			.iter()
			.map(|&b| u64::from(b))
			.sum::<u64>()
	);
}

#[test]
fn the_file_descriptor_is_closed_once_mapped() {
	let dir = scratch("descriptor");
	let path = dir.join("font.ttf");
	fs::write(&path, pattern(100)).unwrap();
	let before = open_descriptors(&path);
	let map = Mapped::open(&path).unwrap();
	assert_eq!(open_descriptors(&path), before, "no descriptor stays open");
	drop(map);
}

/// How many of our descriptors name `path` (0 where we cannot list them).
fn open_descriptors(path: &Path) -> usize {
	let Ok(entries) = fs::read_dir("/proc/self/fd").or_else(|_| fs::read_dir("/dev/fd")) else {
		return 0;
	};
	entries
		.filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
		.filter(|target| target == path)
		.count()
}

#[cfg(target_os = "linux")]
mod linux {
	use super::*;

	/// The lines of `/proc/self/maps` that map `path`.
	fn maps_of(path: &Path) -> Vec<String> {
		let path = path.to_str().unwrap();
		fs::read_to_string("/proc/self/maps")
			.unwrap()
			.lines()
			.filter(|line| line.ends_with(path))
			.map(str::to_owned)
			.collect()
	}

	#[test]
	fn the_file_is_mapped_read_only_and_private() {
		let dir = scratch("perms");
		let path = dir.join("font.ttf");
		fs::write(&path, pattern(5000)).unwrap();
		let map = Mapped::open(&path).unwrap();
		let lines = maps_of(&path);
		assert_eq!(lines.len(), 1, "{lines:?}");
		let perms = lines[0].split_whitespace().nth(1).unwrap();
		assert_eq!(perms, "r--p", "read only, private");
		let start = usize::from_str_radix(lines[0].split('-').next().unwrap(), 16).unwrap();
		assert_eq!(
			map.as_ptr() as usize,
			start,
			"the bytes are the mapping itself"
		);
	}

	#[test]
	fn dropping_the_mapping_unmaps_the_file() {
		let dir = scratch("unmap");
		let path = dir.join("font.ttf");
		fs::write(&path, pattern(5000)).unwrap();
		let map = Mapped::open(&path).unwrap();
		assert_eq!(maps_of(&path).len(), 1);
		drop(map);
		assert!(maps_of(&path).is_empty(), "still mapped after drop");
	}

	#[test]
	fn proc_files_that_claim_no_size_are_refused() {
		// They report a size of 0 yet have contents; reading is the only way.
		let error = Mapped::open(Path::new("/proc/self/status"))
			.err()
			.expect("refused");
		assert_eq!(error.kind(), ErrorKind::InvalidInput);
	}
}

#[test]
fn a_symlink_is_followed_to_the_file_it_names() {
	// Font directories are often links into other packages' directories.
	let dir = scratch("symlink");
	let path = dir.join("real.ttf");
	fs::write(&path, pattern(4097)).unwrap();
	std::os::unix::fs::symlink(&path, dir.join("link.ttf")).unwrap();
	let map = Mapped::open(&dir.join("link.ttf")).unwrap();
	assert_eq!(&*map, &pattern(4097)[..]);
}

#[test]
fn a_dangling_symlink_reports_not_found() {
	let dir = scratch("dangling");
	std::os::unix::fs::symlink(dir.join("gone.ttf"), dir.join("link.ttf")).unwrap();
	let error = Mapped::open(&dir.join("link.ttf")).err().expect("dangling");
	assert_eq!(error.kind(), ErrorKind::NotFound);
}

#[test]
fn an_unreadable_file_reports_permission_denied() {
	use std::os::unix::fs::PermissionsExt;
	let dir = scratch("unreadable");
	let path = dir.join("font.ttf");
	fs::write(&path, pattern(100)).unwrap();
	fs::set_permissions(&path, fs::Permissions::from_mode(0o200)).unwrap();
	// SAFETY: geteuid takes no arguments and cannot fail.
	if unsafe { libc::geteuid() } == 0 {
		return; // root reads any file whatever its mode
	}
	let error = Mapped::open(&path).err().expect("unreadable");
	assert_eq!(error.kind(), ErrorKind::PermissionDenied);
}

#[test]
fn many_mappings_of_one_file_are_independent() {
	let dir = scratch("many");
	let path = dir.join("font.ttf");
	fs::write(&path, pattern(2 * page() + 3)).unwrap();
	let first = Mapped::open(&path).unwrap();
	let second = Mapped::open(&path).unwrap();
	assert_ne!(first.as_ptr(), second.as_ptr(), "two mappings");
	drop(first);
	assert_eq!(
		&*second,
		&pattern(2 * page() + 3)[..],
		"the other is intact"
	);
}
