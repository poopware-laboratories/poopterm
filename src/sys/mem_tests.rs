//! Tests of memory mappings and shared memory objects: what a mapping
//! shows, who sees writes through it, what is refused, and the life of
//! named objects.
use super::*;
use std::{
	ffi::CString,
	fs::{File, OpenOptions},
	io::ErrorKind,
	os::{fd::AsFd, unix::fs::FileExt},
	path::PathBuf,
};

/// A file holding `bytes`, removed when the returned guard drops.
struct Temp(PathBuf);

impl Drop for Temp {
	fn drop(&mut self) {
		let _ = std::fs::remove_file(&self.0);
	}
}

fn temp(name: &str, bytes: &[u8]) -> (Temp, File) {
	let path = std::env::temp_dir().join(format!("poopterm-mem-{}-{name}", std::process::id()));
	std::fs::write(&path, bytes).unwrap();
	let file = OpenOptions::new()
		.read(true)
		.write(true)
		.open(&path)
		.unwrap();
	(Temp(path), file)
}

/// A shared memory name unique to this process and test.
fn shm_name(test: &str) -> CString {
	CString::new(format!("/poopterm-mem-{}-{test}", std::process::id())).unwrap()
}

#[test]
fn a_read_mapping_shows_the_file() {
	let (_temp, file) = temp("read", b"glyph outlines");
	for shared in [false, true] {
		let map = Map::read(file.as_fd(), 14, shared).unwrap();
		assert_eq!(map.bytes(), b"glyph outlines", "shared: {shared}");
	}
}

#[test]
fn a_mapping_may_cover_only_the_start() {
	let (_temp, file) = temp("start", b"0123456789");
	let map = Map::read(file.as_fd(), 4, false).unwrap();
	assert_eq!(map.bytes(), b"0123");
}

#[test]
fn an_empty_mapping_is_refused() {
	let (_temp, file) = temp("empty", b"");
	for result in [
		Map::read(file.as_fd(), 0, false),
		Map::shared(file.as_fd(), 0),
	] {
		assert_eq!(
			result.err().map(|error| error.kind()),
			Some(ErrorKind::InvalidInput)
		);
	}
}

#[test]
fn a_directory_cannot_be_mapped() {
	let dir = File::open(std::env::temp_dir()).unwrap();
	assert!(Map::read(dir.as_fd(), 4096, false).is_err());
}

#[test]
fn a_pipe_cannot_be_mapped() {
	let (reader, writer) = std::io::pipe().unwrap();
	assert!(Map::read(reader.as_fd(), 4096, false).is_err());
	assert!(Map::read(reader.as_fd(), 4096, true).is_err());
	assert!(Map::shared(writer.as_fd(), 4096).is_err());
}

#[test]
fn a_file_open_only_to_write_cannot_be_mapped() {
	let (temp, _file) = temp("write-only", b"data");
	let file = OpenOptions::new().write(true).open(&temp.0).unwrap();
	let refused = Map::read(file.as_fd(), 4, false)
		.err()
		.map(|error| error.kind());
	assert_eq!(refused, Some(ErrorKind::PermissionDenied));
}

#[test]
fn a_file_open_only_to_read_cannot_be_mapped_to_write() {
	let (temp, _file) = temp("read-only", b"data");
	let file = File::open(&temp.0).unwrap();
	let refused = Map::shared(file.as_fd(), 4).err().map(|error| error.kind());
	assert_eq!(refused, Some(ErrorKind::PermissionDenied));
}

#[test]
fn a_read_mapping_cannot_be_written() {
	let (_temp, file) = temp("no-write", b"data");
	let mut map = Map::read(file.as_fd(), 4, true).unwrap();
	assert!(map.bytes_mut().is_none());
	assert!(map.words_mut().is_none());
}

#[test]
fn writes_through_a_shared_mapping_reach_the_file() {
	let (_temp, file) = temp("shared", b"........");
	let mut map = Map::shared(file.as_fd(), 8).unwrap();
	map.bytes_mut().unwrap()[2..5].copy_from_slice(b"abc");
	let mut read = [0; 8];
	file.read_exact_at(&mut read, 0).unwrap();
	assert_eq!(&read, b"..abc...");
}

#[test]
fn a_shared_mapping_sees_later_writes_to_the_file_and_other_mappings() {
	let (_temp, file) = temp("visibility", b"before");
	let shared = Map::read(file.as_fd(), 6, true).unwrap();
	file.write_all_at(b"after!", 0).unwrap();
	assert_eq!(shared.bytes(), b"after!");
	let mut other = Map::shared(file.as_fd(), 6).unwrap();
	other.bytes_mut().unwrap()[0] = b'A';
	assert_eq!(shared.bytes(), b"After!");
}

/// The permissions `/proc/self/maps` shows for the one mapping of `path`.
#[cfg(target_os = "linux")]
fn permissions(path: &std::path::Path) -> String {
	let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
	let lines: Vec<_> = maps
		.lines()
		.filter(|line| line.ends_with(path.to_str().unwrap()))
		.collect();
	assert_eq!(lines.len(), 1, "{lines:?}");
	lines[0].split_whitespace().nth(1).unwrap().to_owned()
}

#[cfg(target_os = "linux")]
#[test]
fn mappings_have_the_protection_and_sharing_asked_for() {
	let (temp, file) = temp("permissions", b"data");
	// Each mapping is dropped before the next, so it is the only one shown.
	let shown = |map: io::Result<Map>| map.map(|_map| permissions(&temp.0)).unwrap();
	assert_eq!(shown(Map::read(file.as_fd(), 4, false)), "r--p");
	assert_eq!(shown(Map::read(file.as_fd(), 4, true)), "r--s");
	assert_eq!(shown(Map::shared(file.as_fd(), 4)), "rw-s");
}

#[test]
fn words_are_the_whole_words_of_the_mapping() {
	let (_temp, file) = temp("words", &[0; 10]);
	let mut map = Map::shared(file.as_fd(), 10).unwrap();
	let words = map.words_mut().unwrap();
	assert_eq!(
		words.len(),
		2,
		"the two bytes past the last word are left out"
	);
	words[1] = u32::from_ne_bytes(*b"wxyz");
	assert_eq!(&map.bytes()[4..8], b"wxyz");
	assert_eq!(&map.bytes()[8..], [0, 0]);
}

#[test]
fn mappings_outlive_the_descriptor_they_came_from() {
	let (_temp, file) = temp("outlive", b"kept");
	let map = Map::read(file.as_fd(), 4, false).unwrap();
	drop(file);
	assert_eq!(map.bytes(), b"kept");
}

#[test]
fn maps_move_between_threads() {
	let (_temp, file) = temp("threads", b"moved");
	let map = Map::read(file.as_fd(), 5, false).unwrap();
	let bytes = std::thread::spawn(move || map.bytes().to_vec())
		.join()
		.unwrap();
	assert_eq!(bytes, b"moved");
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn anonymous_objects_start_empty_and_are_closed_on_exec() {
	use std::os::fd::AsRawFd;
	let fd = anonymous(c"poopterm-test").unwrap();
	let file = File::from(fd);
	assert_eq!(file.metadata().unwrap().len(), 0);
	// SAFETY: reads the flags of a descriptor this test owns.
	let flags = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETFD) };
	assert_eq!(flags & libc::FD_CLOEXEC, libc::FD_CLOEXEC);
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn each_anonymous_object_is_its_own() {
	let first = File::from(anonymous(c"poopterm-test").unwrap());
	let second = File::from(anonymous(c"poopterm-test").unwrap());
	first.set_len(4).unwrap();
	second.set_len(4).unwrap();
	let mut one = Map::shared(first.as_fd(), 4).unwrap();
	let two = Map::shared(second.as_fd(), 4).unwrap();
	one.bytes_mut().unwrap().copy_from_slice(b"mine");
	assert_eq!(two.bytes(), [0; 4]);
}

#[test]
fn a_named_object_is_shared_until_unlinked() {
	let name = shm_name("named");
	let _ = shm_unlink(&name);
	let writer = File::from(shm_open(&name, true).unwrap());
	writer.set_len(5).unwrap();
	Map::shared(writer.as_fd(), 5)
		.unwrap()
		.bytes_mut()
		.unwrap()
		.copy_from_slice(b"hello");
	let reader = shm_open(&name, false).unwrap();
	shm_unlink(&name).unwrap();
	// The name is gone, but the open object and its contents are not.
	assert_eq!(
		Map::read(reader.as_fd(), 5, true).unwrap().bytes(),
		b"hello"
	);
	let gone = shm_open(&name, false).err().map(|error| error.kind());
	assert_eq!(gone, Some(ErrorKind::NotFound));
}

#[test]
fn opening_a_missing_object_to_read_does_not_create_it() {
	let name = shm_name("missing");
	let _ = shm_unlink(&name);
	let missing = shm_open(&name, false).err().map(|error| error.kind());
	assert_eq!(missing, Some(ErrorKind::NotFound));
	assert_eq!(
		shm_unlink(&name).err().map(|error| error.kind()),
		Some(ErrorKind::NotFound)
	);
}

#[test]
fn an_object_opened_to_read_cannot_be_mapped_to_write() {
	let name = shm_name("read-only");
	let _ = shm_unlink(&name);
	let writer = File::from(shm_open(&name, true).unwrap());
	writer.set_len(4).unwrap();
	let reader = shm_open(&name, false).unwrap();
	shm_unlink(&name).unwrap();
	let refused = Map::shared(reader.as_fd(), 4)
		.err()
		.map(|error| error.kind());
	assert_eq!(refused, Some(ErrorKind::PermissionDenied));
}

/// The lines of `/proc/self/maps` that map `path`.
#[cfg(target_os = "linux")]
fn maps_of(path: &std::path::Path) -> usize {
	let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
	let path = path.to_str().unwrap();
	maps.lines().filter(|line| line.ends_with(path)).count()
}

#[cfg(target_os = "linux")]
#[test]
fn dropping_a_map_unmaps_every_page_of_it() {
	// Several pages and a partial one, so unmapping only the first, or a
	// length rounded down, would leave some behind.
	let (temp, file) = temp("drop", &vec![7; 3 * 4096 + 5]);
	for shared in [false, true] {
		let map = Map::read(file.as_fd(), 3 * 4096 + 5, shared).unwrap();
		assert_eq!(maps_of(&temp.0), 1);
		drop(map);
		assert_eq!(maps_of(&temp.0), 0, "shared: {shared}");
	}
	let map = Map::shared(file.as_fd(), 3 * 4096 + 5).unwrap();
	drop(map);
	assert_eq!(maps_of(&temp.0), 0);
}

#[test]
fn a_map_outlives_its_file_being_removed_or_replaced() {
	// Package managers replace fonts by renaming a new file over the old.
	let (temp, file) = temp("replaced", b"old font");
	let map = Map::read(file.as_fd(), 8, false).unwrap();
	let new = temp.0.with_extension("new");
	std::fs::write(&new, b"new font").unwrap();
	std::fs::rename(&new, &temp.0).unwrap();
	assert_eq!(map.bytes(), b"old font");
	std::fs::remove_file(&temp.0).unwrap();
	assert_eq!(map.bytes(), b"old font");
}

#[test]
fn a_mapping_ends_where_asked_within_its_last_page() {
	let (_temp, file) = temp("tail", &[1; 4100]);
	for len in [1, 4095, 4096, 4097, 4100] {
		let map = Map::read(file.as_fd(), len, false).unwrap();
		assert_eq!(map.bytes().len(), len);
		assert!(map.bytes().iter().all(|&byte| byte == 1), "{len} bytes");
	}
}

#[test]
fn words_cover_every_page_of_a_large_mapping() {
	let len = 3 * 4096 + 8;
	let (_temp, file) = temp("large-words", &vec![0; len]);
	let mut map = Map::shared(file.as_fd(), len).unwrap();
	let words = map.words_mut().unwrap();
	assert_eq!(words.len(), len / 4);
	*words.last_mut().unwrap() = u32::from_ne_bytes(*b"last");
	let mut tail = [0; 4];
	file.read_exact_at(&mut tail, (len - 4) as u64).unwrap();
	assert_eq!(&tail, b"last");
}

#[test]
fn threads_read_one_map_at_once() {
	let (_temp, file) = temp("sync", b"shared by all");
	let map = Map::read(file.as_fd(), 13, true).unwrap();
	std::thread::scope(|scope| {
		let readers: Vec<_> = (0..4)
			.map(|_| scope.spawn(|| map.bytes().to_vec()))
			.collect();
		for reader in readers {
			assert_eq!(reader.join().unwrap(), b"shared by all");
		}
	});
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn two_mappings_of_one_anonymous_object_see_each_other() {
	// As the compositor maps a window buffer through the descriptor sent.
	let object = File::from(anonymous(c"poopterm-test").unwrap());
	object.set_len(8).unwrap();
	let mut ours = Map::shared(object.as_fd(), 8).unwrap();
	let theirs = Map::read(object.as_fd(), 8, true).unwrap();
	ours.words_mut().unwrap()[1] = u32::from_ne_bytes(*b"seen");
	assert_eq!(&theirs.bytes()[4..], b"seen");
}

#[test]
fn reopening_a_named_object_to_create_keeps_its_contents() {
	let name = shm_name("reopen");
	let _ = shm_unlink(&name);
	let first = File::from(shm_open(&name, true).unwrap());
	first.set_len(4).unwrap();
	Map::shared(first.as_fd(), 4)
		.unwrap()
		.bytes_mut()
		.unwrap()
		.copy_from_slice(b"kept");
	let again = File::from(shm_open(&name, true).unwrap());
	shm_unlink(&name).unwrap();
	// Not truncated: macOS rounds the size up to a page, others keep 4.
	assert!(again.metadata().unwrap().len() >= 4);
	assert_eq!(Map::read(again.as_fd(), 4, true).unwrap().bytes(), b"kept");
}

/// macOS lets any descriptor size a shared memory object once.
#[cfg(not(target_os = "macos"))]
#[test]
fn a_named_object_opened_to_read_cannot_be_resized() {
	let name = shm_name("resize");
	let _ = shm_unlink(&name);
	drop(shm_open(&name, true).unwrap());
	let reader = File::from(shm_open(&name, false).unwrap());
	shm_unlink(&name).unwrap();
	assert!(reader.set_len(4096).is_err());
	assert_eq!(reader.metadata().unwrap().len(), 0);
}
